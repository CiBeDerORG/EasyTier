use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};

use super::{AssetsStore, common::{AssetError, AssetResult, ensure_device, ensure_node, ensure_owner, now, service_json}, idempotency::Operation, publications, validation};

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

async fn validate_adb(connection: &mut SqliteConnection, owner: i32, device: &str, excluded: &str, payload: &Value) -> AssetResult<()> {
    if payload["service_type"] != "adb" { return Ok(()); }
    if payload["protocols"] != json!(["tcp"]) { return Err(AssetError::field("protocols","An ADB service must use exactly one TCP port.")); }
    let phone: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices d JOIN asset_device_types t ON t.id=d.device_type_id AND t.owner_user_id=d.owner_user_id WHERE d.id=? AND d.owner_user_id=? AND t.semantic_kind='phone' AND d.is_gateway_self=0)")
        .bind(device).bind(owner).fetch_one(&mut *connection).await?;
    if !phone { return Err(AssetError::field("service_type","Confirm the device's phone type before classifying an ADB service.")); }
    let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_services WHERE device_id=? AND service_type='adb' AND id<>?)")
        .bind(device).bind(excluded).fetch_one(&mut *connection).await?;
    if duplicate { return Err(AssetError::conflict("adb_service_exists","This phone already has an ADB service. Edit that service instead.")); }
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
    pub(crate) async fn service_catalog(&self, owner: i32) -> AssetResult<Value> {
        let mut connection = self.pool.acquire().await?;
        ensure_owner(&mut connection, owner).await?;
        // A registered machine can exist under several accounts. Match both
        // halves of every parent key so the global view remains owner-scoped.
        let rows = sqlx::query(
            "SELECT s.*, d.overlay_ip, d.lan_ip, d.name AS device_name, \
             COALESCE(NULLIF(n.alias, ''), NULLIF(n.hostname, ''), n.machine_id) AS node_name \
             FROM asset_services s \
             JOIN asset_devices d ON d.id = s.device_id AND d.owner_user_id = s.owner_user_id AND d.machine_id = s.machine_id \
             JOIN devices n ON n.user_id = s.owner_user_id AND n.machine_id = s.machine_id \
             WHERE s.owner_user_id = ? ORDER BY s.created_at DESC, s.id DESC",
        ).bind(owner).fetch_all(&mut *connection).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut item = service_json(&mut connection, &row).await?;
            item["node_name"] = json!(row.try_get::<String, _>("node_name")?);
            item["device_name"] = json!(row.try_get::<String, _>("device_name")?);
            items.push(item);
        }
        Ok(json!({"items": items}))
    }

    pub(crate) async fn set_service_navigation(&self, owner: i32, node: &str, id: &str, body: Value) -> AssetResult<Value> {
        let enabled = validation::service_navigation_payload(&body)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_node(&mut transaction, owner, node).await?;
        let current = sqlx::query("SELECT access_url FROM asset_services WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_optional(&mut *transaction).await?
            .ok_or_else(AssetError::not_found)?;
        if enabled {
            let link: Option<String> = current.try_get("access_url")?;
            let link = link.ok_or_else(|| AssetError::field("access_url", "Add an HTTP or HTTPS URL before enabling navigation."))?;
            validation::service_access_url(&link)?;
        }
        // Changing visibility must not replay a stale service form or touch
        // protocol bindings and public-port publication ownership.
        sqlx::query("UPDATE asset_services SET show_in_navigation = ?, updated_at = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(enabled).bind(now()).bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        let row = sqlx::query(
            "SELECT s.*, d.overlay_ip, d.lan_ip FROM asset_services s \
             JOIN asset_devices d ON d.id = s.device_id AND d.owner_user_id = s.owner_user_id AND d.machine_id = s.machine_id \
             WHERE s.id = ? AND s.owner_user_id = ? AND s.machine_id = ?",
        ).bind(id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = service_json(&mut transaction, &row).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn list_services(&self, owner: i32, node: &str, device: &str) -> AssetResult<Value> {
        let mut connection = self.pool.acquire().await?;
        ensure_device(&mut connection, owner, node, device).await?;
        let rows = sqlx::query(
            "SELECT s.*, d.overlay_ip, d.lan_ip FROM asset_services s \
             JOIN asset_devices d ON d.id = s.device_id AND d.owner_user_id = s.owner_user_id AND d.machine_id = s.machine_id \
             WHERE s.owner_user_id = ? AND s.machine_id = ? AND s.device_id = ? ORDER BY s.created_at DESC,s.id DESC",
        )
            .bind(owner).bind(node).bind(device).fetch_all(&mut *connection).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows { items.push(service_json(&mut connection, &row).await?); }
        Ok(json!({"items":items}))
    }

    pub(crate) async fn create_service(&self, actor: i32, owner: i32, node: &str, device: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::service_payload(&body, true)?;
        let operation = Operation::new(actor, owner, "create_service", json!({"machine_id":node,"device_id":device}), &body, &payload)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(mut response) = operation.replay(&mut transaction).await? {
            // Preserve the creation result while inherited addresses and navigation
            // reflect current state, including operations saved before these fields existed.
            let (overlay_ip, lan_ip) = sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT overlay_ip, lan_ip FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?",
            )
                .bind(device).bind(owner).bind(node).fetch_optional(&mut *transaction).await?
                .ok_or_else(AssetError::not_found)?;
            response["overlay_ip"] = json!(overlay_ip);
            response["lan_ip"] = json!(lan_ip);
            if let Some(id) = response["id"].as_str().map(str::to_owned) {
                // A retry returns current publication ownership without replaying
                // side effects or reviving a publication removed after creation.
                response["publication"] = publications::response(&mut transaction, owner, node, &id).await?;
                // Keep the current URL paired with current visibility, rather than
                // returning a newly enabled card with an old empty creation URL.
                let current = sqlx::query(
                    "SELECT show_in_navigation, access_url FROM asset_services WHERE id = ? AND owner_user_id = ? AND machine_id = ?",
                ).bind(&id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
                response["show_in_navigation"] = json!(current.try_get::<bool, _>("show_in_navigation")?);
                response["access_url"] = json!(current.try_get::<Option<String>, _>("access_url")?);
            }
            transaction.commit().await?;
            return Ok(response);
        }
        ensure_device(&mut transaction, owner, node, device).await?;
        validate_adb(&mut transaction, owner, device, "", &payload).await?;
        available_bindings(&mut transaction, owner, node, device, "", &payload).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        sqlx::query("INSERT INTO asset_services(id,owner_user_id,machine_id,device_id,name,port,remarks,access_url,icon,show_in_navigation,created_at,updated_at,service_type,connection_mode) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(owner).bind(node).bind(device).bind(payload["name"].as_str()).bind(payload["port"].as_i64())
            .bind(payload["remarks"].as_str()).bind(payload["access_url"].as_str()).bind(payload["icon"].as_str())
            .bind(payload["show_in_navigation"].as_bool().unwrap_or(false))
            .bind(&timestamp).bind(&timestamp).bind(payload["service_type"].as_str().unwrap_or("generic")).bind(payload["connection_mode"].as_str().unwrap_or("tcp")).execute(&mut *transaction).await?;
        insert_bindings(&mut transaction, owner, node, device, &id, &payload).await?;
        publications::sync_service(&mut transaction, owner, node, device, &id, &payload, self.publication_runtime_proof.as_ref()).await?;
        let row = sqlx::query(
            "SELECT s.*, d.overlay_ip, d.lan_ip FROM asset_services s \
             JOIN asset_devices d ON d.id = s.device_id AND d.owner_user_id = s.owner_user_id AND d.machine_id = s.machine_id \
             WHERE s.id = ? AND s.owner_user_id = ? AND s.machine_id = ?",
        )
            .bind(&id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = service_json(&mut transaction, &row).await?;
        operation.finish(&mut transaction, "service", &id, &response).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn update_service(&self, owner: i32, node: &str, id: &str, body: Value) -> AssetResult<Value> {
        let mut payload = validation::service_payload(&body, false)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let original = sqlx::query("SELECT device_id, show_in_navigation, service_type, connection_mode FROM asset_services WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_optional(&mut *transaction).await?.ok_or_else(AssetError::not_found)?;
        let device: String = original.try_get("device_id")?;
        for field in ["service_type","connection_mode"] { if payload.get(field).is_none() { payload[field]=json!(original.try_get::<String,_>(field)?); } }
        validate_adb(&mut transaction, owner, &device, id, &payload).await?;
        let show_in_navigation = payload["show_in_navigation"].as_bool().unwrap_or(
            original.try_get::<bool, _>("show_in_navigation")? && payload["access_url"].is_string(),
        );
        available_bindings(&mut transaction, owner, node, &device, id, &payload).await?;
        sqlx::query("DELETE FROM asset_service_bindings WHERE service_id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        sqlx::query("UPDATE asset_services SET name = ?, port = ?, remarks = ?, access_url = ?, icon = ?, show_in_navigation = ?, updated_at = ?, service_type = ?, connection_mode = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(payload["name"].as_str()).bind(payload["port"].as_i64()).bind(payload["remarks"].as_str())
            .bind(payload["access_url"].as_str()).bind(payload["icon"].as_str()).bind(show_in_navigation).bind(now()).bind(payload["service_type"].as_str()).bind(payload["connection_mode"].as_str()).bind(id).bind(owner).bind(node)
            .execute(&mut *transaction).await?;
        insert_bindings(&mut transaction, owner, node, &device, id, &payload).await?;
        publications::sync_service(&mut transaction, owner, node, &device, id, &payload, self.publication_runtime_proof.as_ref()).await?;
        let row = sqlx::query(
            "SELECT s.*, d.overlay_ip, d.lan_ip FROM asset_services s \
             JOIN asset_devices d ON d.id = s.device_id AND d.owner_user_id = s.owner_user_id AND d.machine_id = s.machine_id \
             WHERE s.id = ? AND s.owner_user_id = ? AND s.machine_id = ?",
        )
            .bind(id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = service_json(&mut transaction, &row).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn delete_service(&self, owner: i32, node: &str, id: &str) -> AssetResult<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        publications::remove_service(&mut transaction, owner, node, id).await?;
        let result = sqlx::query("DELETE FROM asset_services WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        if result.rows_affected() == 0 { return Err(AssetError::not_found()); }
        transaction.commit().await?;
        Ok(())
    }
}
