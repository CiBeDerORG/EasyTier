use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};

use super::{AssetsStore, common::{AssetError, AssetResult, ensure_device, now, service_json}, idempotency::Operation, validation};

async fn available_bindings(connection: &mut SqliteConnection, owner: i32, node: &str, device: &str, excluded_id: &str, payload: &Value) -> AssetResult<()> {
    for protocol in payload["protocols"].as_array().into_iter().flatten() {
        let occupied: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_service_bindings WHERE owner_user_id = ? AND machine_id = ? AND device_id = ? AND protocol = ? AND port = ? AND service_id <> ?)")
            .bind(owner).bind(node).bind(device).bind(protocol.as_str()).bind(payload["port"].as_i64())
            .bind(excluded_id).fetch_one(&mut *connection).await?;
        if occupied != 0 {
            let mut error = AssetError::conflict("service_port_in_use", "Another service on this device already uses this protocol and port.");
            error.field_errors.insert("port".into(), format!("{} {} is already assigned to another service.", protocol.as_str().unwrap_or_default().to_uppercase(), payload["port"]));
            return Err(error);
        }
    }
    Ok(())
}

async fn insert_bindings(connection: &mut SqliteConnection, owner: i32, node: &str, device: &str, id: &str, payload: &Value) -> AssetResult<()> {
    for protocol in payload["protocols"].as_array().into_iter().flatten() {
        sqlx::query("INSERT INTO asset_service_bindings(service_id,owner_user_id,machine_id,device_id,protocol,port) VALUES(?,?,?,?,?,?)")
            .bind(id).bind(owner).bind(node).bind(device).bind(protocol.as_str()).bind(payload["port"].as_i64())
            .execute(&mut *connection).await?;
    }
    Ok(())
}

impl AssetsStore {
    pub(crate) async fn list_services(&self, owner: i32, node: &str, device: &str) -> AssetResult<Value> {
        let mut connection = self.pool.acquire().await?;
        ensure_device(&mut connection, owner, node, device).await?;
        let rows = sqlx::query("SELECT * FROM asset_services WHERE owner_user_id = ? AND machine_id = ? AND device_id = ? ORDER BY created_at DESC,id DESC")
            .bind(owner).bind(node).bind(device).fetch_all(&mut *connection).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows { items.push(service_json(&mut connection, &row).await?); }
        Ok(json!({"items":items}))
    }

    pub(crate) async fn create_service(&self, actor: i32, owner: i32, node: &str, device: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::service_payload(&body, true)?;
        let operation = Operation::new(actor, owner, "create_service", json!({"machine_id":node,"device_id":device}), &body, &payload)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(response) = operation.replay(&mut transaction).await? {
            transaction.commit().await?;
            return Ok(response);
        }
        ensure_device(&mut transaction, owner, node, device).await?;
        available_bindings(&mut transaction, owner, node, device, "", &payload).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        sqlx::query("INSERT INTO asset_services(id,owner_user_id,machine_id,device_id,name,port,remarks,access_url,icon,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(owner).bind(node).bind(device).bind(payload["name"].as_str()).bind(payload["port"].as_i64())
            .bind(payload["remarks"].as_str()).bind(payload["access_url"].as_str()).bind(payload["icon"].as_str())
            .bind(&timestamp).bind(&timestamp).execute(&mut *transaction).await?;
        insert_bindings(&mut transaction, owner, node, device, &id, &payload).await?;
        let row = sqlx::query("SELECT * FROM asset_services WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(&id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = service_json(&mut transaction, &row).await?;
        operation.finish(&mut transaction, "service", &id, &response).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn update_service(&self, owner: i32, node: &str, id: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::service_payload(&body, false)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let original = sqlx::query("SELECT device_id FROM asset_services WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_optional(&mut *transaction).await?.ok_or_else(AssetError::not_found)?;
        let device: String = original.try_get("device_id")?;
        available_bindings(&mut transaction, owner, node, &device, id, &payload).await?;
        sqlx::query("DELETE FROM asset_service_bindings WHERE service_id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        sqlx::query("UPDATE asset_services SET name = ?, port = ?, remarks = ?, access_url = ?, icon = ?, updated_at = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(payload["name"].as_str()).bind(payload["port"].as_i64()).bind(payload["remarks"].as_str())
            .bind(payload["access_url"].as_str()).bind(payload["icon"].as_str()).bind(now()).bind(id).bind(owner).bind(node)
            .execute(&mut *transaction).await?;
        insert_bindings(&mut transaction, owner, node, &device, id, &payload).await?;
        let row = sqlx::query("SELECT * FROM asset_services WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = service_json(&mut transaction, &row).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn delete_service(&self, owner: i32, node: &str, id: &str) -> AssetResult<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let result = sqlx::query("DELETE FROM asset_services WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        if result.rows_affected() == 0 { return Err(AssetError::not_found()); }
        transaction.commit().await?;
        Ok(())
    }
}
