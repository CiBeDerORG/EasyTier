use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Row, Sqlite, SqliteConnection};

use super::{AssetsStore, common::{AssetError, AssetResult, address_warning, device_json, ensure_device, ensure_group, ensure_node, ensure_type, group_subnets, now}, idempotency::Operation, validation};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct DeviceListQuery {
    pub(crate) page: Option<u32>,
    pub(crate) page_size: Option<u32>,
    pub(crate) group_id: Option<String>,
    pub(crate) type_id: Option<String>,
    pub(crate) keyword: Option<String>,
    pub(crate) locate_id: Option<String>,
}

fn filter(query: &mut QueryBuilder<'_, Sqlite>, owner: i32, node: &str, options: &DeviceListQuery) {
    query.push("owner_user_id = ").push_bind(owner).push(" AND machine_id = ").push_bind(node.to_owned());
    if let Some(id) = options.group_id.as_ref().filter(|id| !id.is_empty()) {
        query.push(" AND gateway_group_id = ").push_bind(id.clone());
    }
    if let Some(id) = options.type_id.as_ref().filter(|id| !id.is_empty()) {
        query.push(" AND device_type_id = ").push_bind(id.clone());
    }
}

async fn validate_relations(connection: &mut SqliteConnection, owner: i32, node: &str, excluded_id: &str, payload: &Value) -> AssetResult<()> {
    if let Some(id) = payload["gateway_group_id"].as_str() { ensure_group(connection, owner, node, id).await?; }
    if let Some(id) = payload["device_type_id"].as_str() { ensure_type(connection, owner, id).await?; }
    if let Some(lan) = payload["lan_ip"].as_str() {
        let duplicate: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices WHERE owner_user_id = ? AND machine_id = ? AND lan_ip = ? AND id <> ?)")
            .bind(owner).bind(node).bind(lan).bind(excluded_id).fetch_one(&mut *connection).await?;
        if duplicate != 0 {
            let mut error = AssetError::conflict("lan_ip_in_use", "This LAN IP is already assigned to another device in this network node.");
            error.field_errors.insert("lan_ip".into(), "The LAN IP must be unique within this network node.".into());
            return Err(error);
        }
    }
    if payload["is_gateway_self"].as_bool() == Some(true) {
        let duplicate: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices WHERE owner_user_id = ? AND machine_id = ? AND gateway_group_id = ? AND is_gateway_self = 1 AND id <> ?)")
            .bind(owner).bind(node).bind(payload["gateway_group_id"].as_str()).bind(excluded_id).fetch_one(&mut *connection).await?;
        if duplicate != 0 {
            let mut error = AssetError::conflict("gateway_self_in_use", "This gateway group already has a self device.");
            error.field_errors.insert("is_gateway_self".into(), "Each gateway group can have at most one self device.".into());
            return Err(error);
        }
    }
    Ok(())
}

impl AssetsStore {
    pub(crate) async fn list_devices(&self, owner: i32, node: &str, options: DeviceListQuery) -> AssetResult<Value> {
        let mut connection = self.pool.acquire().await?;
        ensure_node(&mut connection, owner, node).await?;
        if let Some(id) = options.group_id.as_ref().filter(|id| !id.is_empty()) { ensure_group(&mut connection, owner, node, id).await?; }
        if let Some(id) = options.type_id.as_ref().filter(|id| !id.is_empty()) { ensure_type(&mut connection, owner, id).await?; }
        let page_size = options.page_size.unwrap_or(20).clamp(1, 100) as i64;
        let mut page = options.page.unwrap_or(1).max(1) as i64;
        if let Some(keyword) = options.keyword.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
            // SQLite lower() folds ASCII only. The initial deployment has at
            // most a handful of assets per node, so apply Unicode lowercasing
            // to this scoped result on the server before counting and paging.
            let keyword = keyword.to_lowercase();
            let mut candidates = QueryBuilder::<Sqlite>::new("SELECT * FROM asset_devices WHERE ");
            filter(&mut candidates, owner, node, &options);
            candidates.push(" ORDER BY created_at DESC,id DESC");
            let rows = candidates.build().fetch_all(&mut *connection).await?;
            let locate_id = options.locate_id.as_ref().filter(|id| !id.is_empty());
            let mut matches = Vec::new();
            let mut located_index = None;
            for row in rows {
                let fields = [
                    row.try_get::<String,_>("name")?,
                    row.try_get::<Option<String>,_>("model")?.unwrap_or_default(),
                    row.try_get::<String,_>("overlay_ip")?,
                    row.try_get::<Option<String>,_>("lan_ip")?.unwrap_or_default(),
                ];
                if fields.iter().any(|field| field.to_lowercase().contains(&keyword)) {
                    if let Some(id) = locate_id {
                        if row.try_get::<String,_>("id")? == *id { located_index = Some(matches.len()); }
                    }
                    matches.push(row);
                }
            }
            let total = matches.len() as i64;
            if locate_id.is_some() {
                page = located_index.ok_or_else(AssetError::not_found)? as i64 / page_size + 1;
            } else {
                page = page.min(((total + page_size - 1) / page_size).max(1));
            }
            let mut items = Vec::new();
            for row in matches.into_iter().skip(((page - 1) * page_size) as usize).take(page_size as usize) {
                items.push(device_json(&mut connection, &row).await?);
            }
            return Ok(json!({"items":items,"total":total,"page":page,"page_size":page_size}));
        }
        let mut count = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM asset_devices WHERE ");
        filter(&mut count, owner, node, &options);
        let total: i64 = count.build_query_scalar().fetch_one(&mut *connection).await?;
        if let Some(id) = options.locate_id.as_ref().filter(|id| !id.is_empty()) {
            let mut locate = QueryBuilder::<Sqlite>::new("SELECT created_at,id FROM asset_devices WHERE ");
            filter(&mut locate, owner, node, &options);
            locate.push(" AND id = ").push_bind(id.clone());
            let row = locate.build().fetch_optional(&mut *connection).await?.ok_or_else(AssetError::not_found)?;
            let created_at: String = row.try_get("created_at")?;
            let mut preceding = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM asset_devices WHERE ");
            filter(&mut preceding, owner, node, &options);
            preceding.push(" AND (created_at > ").push_bind(created_at.clone())
                .push(" OR (created_at = ").push_bind(created_at)
                .push(" AND id > ").push_bind(id.clone()).push("))");
            let rank: i64 = preceding.build_query_scalar().fetch_one(&mut *connection).await?;
            page = rank / page_size + 1;
        } else {
            page = page.min(((total + page_size - 1) / page_size).max(1));
        }
        let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM asset_devices WHERE ");
        filter(&mut query, owner, node, &options);
        query.push(" ORDER BY created_at DESC,id DESC LIMIT ").push_bind(page_size)
            .push(" OFFSET ").push_bind((page - 1) * page_size);
        let rows = query.build().fetch_all(&mut *connection).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows { items.push(device_json(&mut connection, &row).await?); }
        Ok(json!({"items":items,"total":total,"page":page,"page_size":page_size}))
    }

    pub(crate) async fn get_device(&self, owner: i32, node: &str, id: &str) -> AssetResult<Value> {
        let mut connection = self.pool.acquire().await?;
        let row = sqlx::query("SELECT * FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_optional(&mut *connection).await?.ok_or_else(AssetError::not_found)?;
        device_json(&mut connection, &row).await
    }

    pub(crate) async fn create_device(&self, actor: i32, owner: i32, node: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::device_payload(&body, true)?;
        let operation = Operation::new(actor, owner, "create_device", json!({"machine_id":node}), &body, &payload)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(response) = operation.replay(&mut transaction).await? {
            transaction.commit().await?;
            return Ok(response);
        }
        ensure_node(&mut transaction, owner, node).await?;
        validate_relations(&mut transaction, owner, node, "", &payload).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        sqlx::query("INSERT INTO asset_devices(id,owner_user_id,machine_id,gateway_group_id,device_type_id,name,model,overlay_ip,lan_ip,is_gateway_self,remarks,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(owner).bind(node).bind(payload["gateway_group_id"].as_str()).bind(payload["device_type_id"].as_str())
            .bind(payload["name"].as_str()).bind(payload["model"].as_str()).bind(payload["overlay_ip"].as_str())
            .bind(payload["lan_ip"].as_str()).bind(payload["is_gateway_self"].as_bool().unwrap_or(false))
            .bind(payload["remarks"].as_str()).bind(&timestamp).bind(&timestamp).execute(&mut *transaction).await?;
        let row = sqlx::query("SELECT * FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(&id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = device_json(&mut transaction, &row).await?;
        operation.finish(&mut transaction, "device", &id, &response).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn update_device(&self, owner: i32, node: &str, id: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::device_payload(&body, false)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let old = sqlx::query("SELECT gateway_group_id FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_optional(&mut *transaction).await?.ok_or_else(AssetError::not_found)?;
        let original_group: Option<String> = old.try_get("gateway_group_id")?;
        if original_group.as_deref() != payload["gateway_group_id"].as_str() {
            let mut error = AssetError::conflict("asset_ownership_fixed", "A device's network node and gateway group cannot be changed after creation.");
            error.field_errors.insert("gateway_group_id".into(), "The gateway group is fixed after creation.".into());
            return Err(error);
        }
        validate_relations(&mut transaction, owner, node, id, &payload).await?;
        sqlx::query("UPDATE asset_devices SET device_type_id = ?, name = ?, model = ?, overlay_ip = ?, lan_ip = ?, is_gateway_self = ?, remarks = ?, updated_at = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(payload["device_type_id"].as_str()).bind(payload["name"].as_str()).bind(payload["model"].as_str())
            .bind(payload["overlay_ip"].as_str()).bind(payload["lan_ip"].as_str()).bind(payload["is_gateway_self"].as_bool().unwrap_or(false))
            .bind(payload["remarks"].as_str()).bind(now()).bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        let row = sqlx::query("SELECT * FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = device_json(&mut transaction, &row).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn delete_device(&self, owner: i32, node: &str, id: &str) -> AssetResult<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_device(&mut transaction, owner, node, id).await?;
        // The cascading FK removes every service present at deletion time, not
        // only the service IDs displayed in a prior browser confirmation.
        sqlx::query("DELETE FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn preview_device_address(&self, owner: i32, node: &str, body: Value) -> AssetResult<Value> {
        validation::keys(&body, &["device_id", "gateway_group_id", "overlay_ip", "lan_ip"])?;
        let mut connection = self.pool.acquire().await?;
        ensure_node(&mut connection, owner, node).await?;
        let group = validation::optional_id(&body,"gateway_group_id")?;
        let id = validation::optional_id(&body,"device_id")?;
        if let Some(group_id) = &group { ensure_group(&mut connection, owner, node, group_id).await?; }
        if let Some(id) = &id { ensure_device(&mut connection, owner, node, id).await?; }
        let addresses = (|| -> AssetResult<_> {
            validation::canonical_ip(&validation::required_string(&body,"overlay_ip",128)?, "overlay_ip")?;
            validation::optional_string(&body,"lan_ip",128)?.map(|value| validation::canonical_ip(&value,"lan_ip")).transpose()
        })();
        let lan = match addresses {
            Ok(value) => value,
            Err(error) if error.status == 422 => return Ok(json!({"field_errors":error.field_errors,"warnings":[]})),
            Err(error) => return Err(error),
        };
        let payload = json!({"gateway_group_id":group,"lan_ip":lan,"is_gateway_self":false});
        if let Err(error) = validate_relations(&mut connection, owner, node, id.as_deref().unwrap_or(""), &payload).await {
            if !error.field_errors.is_empty() { return Ok(json!({"field_errors":error.field_errors,"warnings":[]})); }
            return Err(error);
        }
        let mut warnings = Vec::new();
        if let (Some(group), Some(lan)) = (payload["gateway_group_id"].as_str(), payload["lan_ip"].as_str()) {
            let subnets = group_subnets(&mut connection, owner, node, group).await?;
            if !validation::matches_subnets(lan, &subnets) { warnings.push(address_warning(id.as_deref(), None, lan)); }
        }
        Ok(json!({"field_errors":{},"warnings":warnings}))
    }
}
