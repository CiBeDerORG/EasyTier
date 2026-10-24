//! Persisted, explicitly requested OpenWrt discovery operations. No background jobs.
mod adb;
mod apply;
mod preview;
use super::{
    Db,
    assets::{
        RuntimeObservations, binding_ops,
        common::{AssetError, AssetResult, now},
    },
};
use crate::discovery::{Runtime, luci_client};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::net::Ipv4Addr;
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct Store {
    pub pool: SqlitePool,
    pub observations: RuntimeObservations,
}
#[derive(Clone)]
pub(crate) struct Gateway {
    pub id: String,
    pub owner: i32,
    pub node: String,
    pub group: String,
    pub context: Value,
    pub snapshot: Value,
}
pub(crate) fn hash(v: &Value) -> String {
    format!("{:x}", Sha256::digest(v.to_string().as_bytes()))
}
pub(crate) fn parse(s: &str) -> AssetResult<Value> {
    serde_json::from_str(s).map_err(|_| {
        AssetError::new(
            500,
            "discovery_record_invalid",
            "Saved discovery data is invalid.",
        )
    })
}
pub(crate) fn conflict(message: &str) -> AssetError {
    AssetError::conflict("discovery_stale", message)
}
pub(crate) fn request_id(body: &Value) -> AssetResult<String> {
    Uuid::parse_str(body["request_id"].as_str().unwrap_or(""))
        .map(|v| v.to_string())
        .map_err(|_| AssetError::field("request_id", "A UUID request ID is required."))
}
pub(crate) fn error_value(e: AssetError) -> Value {
    json!({"state":"failed","code":e.code,"message":e.message,"field_errors":e.field_errors})
}
impl Store {
    pub fn new(db: &Db, observations: RuntimeObservations) -> Self {
        Self {
            pool: db.inner(),
            observations,
        }
    }
    pub async fn gateway(&self, owner: i32, id: &str) -> AssetResult<Gateway> {
        let mut c = self.pool.acquire().await?;
        self.gateway_on(&mut c, owner, id).await
    }
    pub async fn gateway_on(
        &self,
        c: &mut SqliteConnection,
        owner: i32,
        id: &str,
    ) -> AssetResult<Gateway> {
        let row=sqlx::query("SELECT d.*,t.semantic_kind,g.binding_snapshot_json AS parent_snapshot,g.updated_at AS group_updated,COALESCE(a.config_revision,0) AS access_revision FROM asset_devices d JOIN asset_device_types t ON t.id=d.device_type_id AND t.owner_user_id=d.owner_user_id JOIN asset_gateway_groups g ON g.id=d.gateway_group_id AND g.owner_user_id=d.owner_user_id AND g.machine_id=d.machine_id LEFT JOIN discovery_gateway_access a ON a.gateway_device_id=d.id WHERE d.id=? AND d.owner_user_id=? AND d.is_gateway_self=1 AND t.semantic_kind='openwrt'")
            .bind(id).bind(owner).fetch_optional(&mut *c).await?.ok_or_else(||AssetError::new(422,"openwrt_gateway_required","Select a gateway self device with a confirmed OpenWrt type."))?;
        let node: String = row.try_get("machine_id")?;
        let group: String = row.try_get("gateway_group_id")?;
        let snapshot = row
            .try_get::<Option<String>, _>("parent_snapshot")?
            .map(|s| parse(&s))
            .transpose()?
            .unwrap_or(Value::Null);
        let binding = binding_ops::binding_info(c, &row, "device", &self.observations).await?;
        let context = json!({"owner":owner,"gateway":id,"node":node,"group":group,"access_revision":row.try_get::<i64,_>("access_revision")?,"gateway_updated":row.try_get::<String,_>("updated_at")?,"group_updated":row.try_get::<String,_>("group_updated")?,"binding":binding,"snapshot":snapshot});
        Ok(Gateway {
            id: id.into(),
            owner,
            node,
            group,
            context,
            snapshot,
        })
    }
    pub async fn access(&self, g: &Gateway) -> AssetResult<Value> {
        let row=sqlx::query("SELECT endpoint_url,ca_pem,secret_cipher IS NOT NULL AS has_secret,config_revision,last_result_json FROM discovery_gateway_access WHERE gateway_device_id=? AND owner_user_id=?").bind(&g.id).bind(g.owner).fetch_optional(&self.pool).await?;
        if let Some(r) = row {
            Ok(
                json!({"endpoint_url":r.try_get::<String,_>("endpoint_url")?,"ca_pem":r.try_get::<String,_>("ca_pem")?,"has_secret":r.try_get::<bool,_>("has_secret")?,"config_revision":r.try_get::<i64,_>("config_revision")?,"last_result":r.try_get::<Option<String>,_>("last_result_json")?.map(|s|parse(&s)).transpose()?,"username":"easyadb"}),
            )
        } else {
            let ip: String = sqlx::query_scalar("SELECT overlay_ip FROM asset_devices WHERE id=?")
                .bind(&g.id)
                .fetch_one(&self.pool)
                .await?;
            Ok(
                json!({"endpoint_url":format!("https://{ip}:443/ubus"),"ca_pem":"","has_secret":false,"config_revision":0,"last_result":null,"username":"easyadb"}),
            )
        }
    }
    pub async fn save_access(
        &self,
        g: &Gateway,
        body: &Value,
        runtime: &Runtime,
    ) -> AssetResult<Value> {
        super::assets::validation::keys(
            body,
            &[
                "endpoint_url",
                "ca_pem",
                "secret_action",
                "secret",
                "expected_revision",
            ],
        )?;
        let endpoint = body["endpoint_url"]
            .as_str()
            .ok_or_else(|| AssetError::field("endpoint_url", "An HTTP or HTTPS endpoint is required."))?;
        let ca = body["ca_pem"].as_str().unwrap_or("");
        if ca.len() > 128 * 1024 {
            return Err(AssetError::field(
                "ca_pem",
                "Certificate bundle exceeds 128 KiB.",
            ));
        }
        let url = luci_client::endpoint(endpoint)?;
        let ca = if url.scheme() == "https" { ca.trim() } else { "" };
        luci_client::LuciClient::new(endpoint, ca)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.gateway_on(&mut tx, g.owner, &g.id).await?;
        let row=sqlx::query("SELECT config_revision,secret_cipher FROM discovery_gateway_access WHERE gateway_device_id=?").bind(&g.id).fetch_optional(&mut *tx).await?;
        let revision = row
            .as_ref()
            .map(|r| r.try_get::<i64, _>("config_revision"))
            .transpose()?
            .unwrap_or(0);
        if body["expected_revision"].as_i64() != Some(revision) {
            return Err(conflict("Access settings changed. Reload before saving."));
        }
        let old = row
            .as_ref()
            .map(|r| r.try_get::<Option<String>, _>("secret_cipher"))
            .transpose()?
            .flatten();
        let secret = match body["secret_action"].as_str() {
            Some("preserve") if body.get("secret").is_none() => old,
            Some("clear") if body.get("secret").is_none() => None,
            Some("replace") => {
                let s = body["secret"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 4096)
                    .ok_or_else(|| {
                        AssetError::field(
                            "secret",
                            "Supply the exact authorization password (1–4096 bytes).",
                        )
                    })?;
                Some(
                    runtime
                        .credentials
                        .seal(&format!("discovery:{}:{}", g.owner, g.id), s.as_bytes())
                        .map_err(|_| {
                            AssetError::new(
                                500,
                                "credential_unavailable",
                                "Credential protection is unavailable.",
                            )
                        })?,
                )
            }
            _ => {
                return Err(AssetError::field(
                    "secret_action",
                    "Choose preserve, replace or clear. Only replace accepts a secret.",
                ));
            }
        };
        sqlx::query("INSERT INTO discovery_gateway_access(gateway_device_id,owner_user_id,endpoint_url,ca_pem,secret_cipher,config_revision,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(gateway_device_id) DO UPDATE SET endpoint_url=excluded.endpoint_url,ca_pem=excluded.ca_pem,secret_cipher=excluded.secret_cipher,config_revision=excluded.config_revision,last_result_json=NULL,updated_at=excluded.updated_at")
            .bind(&g.id).bind(g.owner).bind(endpoint).bind(ca).bind(secret).bind(revision+1).bind(now()).execute(&mut *tx).await?;
        tx.commit().await?;
        self.access(g).await
    }
    pub async fn client(
        &self,
        g: &Gateway,
        runtime: &Runtime,
    ) -> AssetResult<(luci_client::LuciClient, String)> {
        let row=sqlx::query("SELECT endpoint_url,ca_pem,secret_cipher FROM discovery_gateway_access WHERE gateway_device_id=? AND owner_user_id=?").bind(&g.id).bind(g.owner).fetch_optional(&self.pool).await?.ok_or_else(||AssetError::field("access","Save discovery access settings first."))?;
        let cipher: Option<String> = row.try_get("secret_cipher")?;
        let cipher = cipher.ok_or_else(|| {
            AssetError::field("secret", "Save the easyadb authorization password first.")
        })?;
        let secret = runtime
            .credentials
            .open(&format!("discovery:{}:{}", g.owner, g.id), &cipher)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(|| {
                AssetError::new(
                    500,
                    "credential_unavailable",
                    "Cannot decrypt this gateway's credentials.",
                )
            })?;
        Ok((
            luci_client::LuciClient::new(
                &row.try_get::<String, _>("endpoint_url")?,
                &row.try_get::<String, _>("ca_pem")?,
            )?,
            secret,
        ))
    }
    /// Reserve before network I/O: an interrupted request can be inspected, never implicitly replayed.
    pub async fn start(
        &self,
        actor: i32,
        g: &Gateway,
        kind: &str,
        body: &Value,
        origin: Option<&str>,
    ) -> AssetResult<(String, bool)> {
        let request = request_id(body)?;
        let digest = hash(body);
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(row)=sqlx::query("SELECT id,request_hash FROM discovery_operations WHERE actor_user_id=? AND owner_user_id=? AND gateway_device_id=? AND kind=? AND request_id=?").bind(actor).bind(g.owner).bind(&g.id).bind(kind).bind(&request).fetch_optional(&mut *tx).await?{
            if row.try_get::<String,_>("request_hash")?!=digest{return Err(conflict("Request ID was already used for a different payload."));}
            return Ok((row.try_get("id")?,false));
        }
        let id = Uuid::new_v4().to_string();
        let expires = (chrono::Utc::now() + chrono::Duration::minutes(10))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        sqlx::query("INSERT INTO discovery_operations(id,actor_user_id,owner_user_id,gateway_device_id,machine_id,group_id,kind,request_id,request_hash,status,context_json,originating_operation_id,expires_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'requested',?,?,?,?,?)")
            .bind(&id).bind(actor).bind(g.owner).bind(&g.id).bind(&g.node).bind(&g.group).bind(kind).bind(request).bind(digest).bind(g.context.to_string()).bind(origin).bind(expires).bind(now()).bind(now()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok((id, true))
    }
    pub async fn finish(
        &self,
        id: &str,
        snapshot: Option<&Value>,
        result: &Value,
    ) -> AssetResult<()> {
        sqlx::query("UPDATE discovery_operations SET snapshot_json=COALESCE(?,snapshot_json),result_json=?,status='recorded',revision=revision+1,updated_at=? WHERE id=?").bind(snapshot.map(Value::to_string)).bind(result.to_string()).bind(now()).bind(id).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn operation(&self, g: &Gateway, id: &str) -> AssetResult<Value> {
        let r=sqlx::query("SELECT * FROM discovery_operations WHERE id=? AND owner_user_id=? AND gateway_device_id=?").bind(id).bind(g.owner).bind(&g.id).fetch_optional(&self.pool).await?.ok_or_else(AssetError::not_found)?;
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT preview_json FROM discovery_preview_rows WHERE operation_id=? ORDER BY row_id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let events=sqlx::query("SELECT event_id,device_id,endpoint_revision,publication_json,cleanup_json,connection_json FROM discovery_endpoint_events WHERE operation_id=? ORDER BY seq").bind(id).fetch_all(&self.pool).await?;
        let events=events.iter().map(|r|Ok(json!({"event_id":r.try_get::<String,_>("event_id")?,"device_id":r.try_get::<String,_>("device_id")?,"endpoint_revision":r.try_get::<i64,_>("endpoint_revision")?,"publication":parse(&r.try_get::<String,_>("publication_json")?)?,"cleanup":parse(&r.try_get::<String,_>("cleanup_json")?)?,"connection":parse(&r.try_get::<String,_>("connection_json")?)?}))).collect::<AssetResult<Vec<_>>>()?;
        Ok(
            json!({"operation_id":id,"preview_id":id,"preview_revision":r.try_get::<i64,_>("revision")?,"revision":r.try_get::<i64,_>("revision")?,"status":r.try_get::<String,_>("status")?,"kind":r.try_get::<String,_>("kind")?,"expires_at":r.try_get::<String,_>("expires_at")?,"originating_operation_id":r.try_get::<Option<String>,_>("originating_operation_id")?,"result":parse(&r.try_get::<String,_>("result_json")?)?,"snapshot":r.try_get::<Option<String>,_>("snapshot_json")?.map(|s|parse(&s)).transpose()?,"rows":rows.iter().map(|s|parse(s)).collect::<AssetResult<Vec<_>>>()?,"events":events}),
        )
    }
    pub async fn assert_operation(
        &self,
        c: &mut SqliteConnection,
        g: &Gateway,
        id: &str,
    ) -> AssetResult<Value> {
        let r=sqlx::query("SELECT * FROM discovery_operations WHERE id=? AND owner_user_id=? AND gateway_device_id=?").bind(id).bind(g.owner).bind(&g.id).fetch_optional(&mut *c).await?.ok_or_else(AssetError::not_found)?;
        if r.try_get::<String, _>("expires_at")? < now() {
            return Err(conflict(
                "This snapshot expired. Start a new discovery operation.",
            ));
        }
        let current = self.gateway_on(c, g.owner, &g.id).await?;
        if parse(&r.try_get::<String, _>("context_json")?)? != current.context {
            return Err(conflict(
                "Gateway settings or binding changed. Start a new discovery operation.",
            ));
        }
        Ok(r.try_get::<Option<String>, _>("snapshot_json")?
            .map(|s| parse(&s))
            .transpose()?
            .unwrap_or(Value::Null))
    }
    pub async fn replay(
        &self,
        actor: i32,
        owner: i32,
        scope: &str,
        body: &Value,
    ) -> AssetResult<Option<Value>> {
        let request = request_id(body)?;
        let row=sqlx::query("SELECT payload_hash,response_json FROM discovery_requests WHERE actor_user_id=? AND owner_user_id=? AND scope=? AND request_id=?").bind(actor).bind(owner).bind(scope).bind(request).fetch_optional(&self.pool).await?;
        row.map(|r| {
            if r.try_get::<String, _>("payload_hash")? != hash(body) {
                return Err(conflict("Request ID was used for a different payload."));
            }
            parse(&r.try_get::<String, _>("response_json")?)
        })
        .transpose()
    }
    pub async fn remember(
        &self,
        actor: i32,
        owner: i32,
        scope: &str,
        body: &Value,
        result: &Value,
    ) -> AssetResult<()> {
        sqlx::query("INSERT INTO discovery_requests(actor_user_id,owner_user_id,scope,request_id,payload_hash,response_json) VALUES(?,?,?,?,?,?)").bind(actor).bind(owner).bind(scope).bind(request_id(body)?).bind(hash(body)).bind(result.to_string()).execute(&self.pool).await?;
        Ok(())
    }
}
pub(crate) fn single_ip(row: &Value) -> Option<String> {
    let ips = row["ipv4"].as_array()?;
    if ips.len() != 1 {
        return None;
    }
    let ip = ips[0].as_str()?.parse::<Ipv4Addr>().ok()?;
    if ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast() {
        None
    } else {
        Some(ip.to_string())
    }
}
#[cfg(test)]
pub(crate) mod tests;
