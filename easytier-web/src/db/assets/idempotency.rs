use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

use super::common::{AssetError, AssetResult, now};

pub(super) struct Operation {
    actor: i32,
    owner: i32,
    operation: &'static str,
    request_id: String,
    hash: String,
}

impl Operation {
    pub(super) fn new(actor: i32, owner: i32, operation: &'static str, scope: Value, body: &Value, payload: &Value) -> AssetResult<Self> {
        let request_id = body.get("request_id").and_then(Value::as_str)
            .and_then(|value| uuid::Uuid::parse_str(value).ok()).map(|value| value.to_string())
            .ok_or_else(|| AssetError::field("request_id", "A UUID identifying this creation attempt is required."))?;
        let canonical = json!({"scope":scope,"payload":payload}).to_string();
        let hash = format!("{:x}", Sha256::digest(canonical.as_bytes()));
        Ok(Self { actor, owner, operation, request_id, hash })
    }

    pub(super) async fn replay(&self, connection: &mut SqliteConnection) -> AssetResult<Option<Value>> {
        let row = sqlx::query("SELECT payload_hash, resource_kind, resource_id, response_json FROM asset_operations WHERE actor_user_id = ? AND owner_user_id = ? AND operation = ? AND request_id = ?")
            .bind(self.actor).bind(self.owner).bind(self.operation).bind(&self.request_id)
            .fetch_optional(&mut *connection).await?;
        let Some(row) = row else { return Ok(None); };
        let stored_hash: String = row.try_get("payload_hash")?;
        if stored_hash != self.hash {
            return Err(AssetError::conflict("request_payload_changed", "This request ID was already used with different data. Retry the original attempt or start a new creation."));
        }
        let resource_kind: String = row.try_get("resource_kind")?;
        let table = match resource_kind.as_str() {
            "group" => "asset_gateway_groups",
            "type" => "asset_device_types",
            "device" => "asset_devices",
            "service" => "asset_services",
            _ => return Err(AssetError::new(500, "invalid_operation_record", "The previous creation result cannot be loaded.")),
        };
        let exists: i64 = sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ? AND owner_user_id = ?)"))
            .bind(row.try_get::<String,_>("resource_id")?).bind(self.owner).fetch_one(&mut *connection).await?;
        if exists == 0 {
            return Err(AssetError::new(410, "created_resource_deleted", "The resource created by this attempt was deleted; it will not be recreated by retrying."));
        }
        let response = serde_json::from_str(&row.try_get::<String,_>("response_json")?)
            .map_err(|_| AssetError::new(500, "invalid_operation_record", "The previous creation result cannot be loaded."))?;
        Ok(Some(response))
    }

    pub(super) async fn finish(&self, connection: &mut SqliteConnection, kind: &str, id: &str, response: &Value) -> AssetResult<()> {
        sqlx::query("INSERT INTO asset_operations(actor_user_id, owner_user_id, operation, request_id, payload_hash, resource_kind, resource_id, response_json, created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(self.actor).bind(self.owner).bind(self.operation).bind(&self.request_id).bind(&self.hash)
            .bind(kind).bind(id).bind(response.to_string()).bind(now()).execute(&mut *connection).await?;
        Ok(())
    }
}
