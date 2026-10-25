use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Row, Sqlite, SqliteConnection};

use super::{AssetsStore, binding_ops, common::{AssetError, AssetResult, address_warning, device_json, ensure_device, ensure_group, ensure_node, ensure_type, group_subnets, now}, idempotency::Operation, publications, validation};

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

pub(crate) async fn validate_relations(connection: &mut SqliteConnection, owner: i32, node: &str, excluded_id: &str, payload: &Value) -> AssetResult<()> {
    if let Some(id) = payload["gateway_group_id"].as_str() { ensure_group(connection, owner, node, id).await?; }
    if let Some(id) = payload["device_type_id"].as_str() { ensure_type(connection, owner, id).await?; }
    if let Some(lan) = payload["lan_ip"].as_str() {
        let duplicate: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices WHERE owner_user_id = ? AND machine_id = ? AND gateway_group_id IS ? AND lan_ip = ? AND id <> ?)")
            .bind(owner).bind(node).bind(payload["gateway_group_id"].as_str()).bind(lan).bind(excluded_id).fetch_one(&mut *connection).await?;
        if duplicate != 0 {
            let mut error = AssetError::conflict("lan_ip_in_use", "This LAN IP is already assigned to another device in this gateway group.");
            error.field_errors.insert("lan_ip".into(), "The LAN IP must be unique within this gateway group (or the ungrouped node scope).".into());
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

// Group first, then the most recently updated devices within each group.
// Ungrouped devices come last; name + id stabilize pagination when names collide.
const DEVICE_ORDER: &str = " ORDER BY (gateway_group_id IS NULL), COALESCE((SELECT g.name FROM asset_gateway_groups g WHERE g.id=asset_devices.gateway_group_id), '') COLLATE NOCASE, gateway_group_id, updated_at DESC, id DESC";

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
            candidates.push(DEVICE_ORDER);
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
                items.push(device_json(&mut connection, &row, &self.runtime_observations).await?);
            }
            return Ok(json!({"items":items,"total":total,"page":page,"page_size":page_size}));
        }
        let mut count = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM asset_devices WHERE ");
        filter(&mut count, owner, node, &options);
        let total: i64 = count.build_query_scalar().fetch_one(&mut *connection).await?;
        if let Some(id) = options.locate_id.as_ref().filter(|id| !id.is_empty()) {
            // Use the same group/update ordering for locate and listing,
            // otherwise a device deep-link may open the wrong page.
            let mut locate = QueryBuilder::<Sqlite>::new(
                "SELECT rn FROM (SELECT id, ROW_NUMBER() OVER (");
            locate.push(DEVICE_ORDER);
            locate.push(") AS rn FROM asset_devices WHERE ");
            filter(&mut locate, owner, node, &options);
            locate.push(") ranked WHERE id = ").push_bind(id.clone());
            let rank: i64 = locate.build_query_scalar().fetch_optional(&mut *connection).await?
                .ok_or_else(AssetError::not_found)?;
            page = (rank - 1) / page_size + 1;
        } else {
            page = page.min(((total + page_size - 1) / page_size).max(1));
        }
        let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM asset_devices WHERE ");
        filter(&mut query, owner, node, &options);
        query.push(DEVICE_ORDER).push(" LIMIT ").push_bind(page_size)
            .push(" OFFSET ").push_bind((page - 1) * page_size);
        let rows = query.build().fetch_all(&mut *connection).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows { items.push(device_json(&mut connection, &row, &self.runtime_observations).await?); }
        Ok(json!({"items":items,"total":total,"page":page,"page_size":page_size}))
    }

    pub(crate) async fn get_device(&self, owner: i32, node: &str, id: &str) -> AssetResult<Value> {
        let mut connection = self.pool.acquire().await?;
        let row = sqlx::query("SELECT * FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_optional(&mut *connection).await?.ok_or_else(AssetError::not_found)?;
        device_json(&mut connection, &row, &self.runtime_observations).await
    }

    pub(crate) async fn create_device(&self, actor: i32, owner: i32, node: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::device_create_payload(&body)?;
        let operation = Operation::new(actor, owner, "create_device", json!({"machine_id":node}), &body, &payload)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(response) = operation.replay(&mut transaction).await? {
            transaction.commit().await?;
            return Ok(response);
        }
        ensure_node(&mut transaction, owner, node).await?;
        let prepared = binding_ops::prepare_address(&mut transaction, owner, node,
            &json!({"resource_kind":"device","id":null}), &payload["address_spec"], &self.runtime_observations, false).await?;
        binding_ops::assert_version(&payload, &prepared.preview)?;
        let relations = json!({"gateway_group_id":prepared.group_id,"device_type_id":payload["device_type_id"],
            "lan_ip":prepared.addresses["lan_ip"],"is_gateway_self":prepared.is_self});
        validate_relations(&mut transaction, owner, node, "", &relations).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        sqlx::query("INSERT INTO asset_devices(id,owner_user_id,machine_id,gateway_group_id,device_type_id,name,model,overlay_ip,lan_ip,is_gateway_self,remarks,created_at,updated_at,address_mode,rule_key,host_offset) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(owner).bind(node).bind(prepared.group_id.as_deref()).bind(payload["device_type_id"].as_str())
            .bind(payload["name"].as_str()).bind(payload["model"].as_str()).bind(prepared.addresses["overlay_ip"].as_str())
            .bind(prepared.addresses["lan_ip"].as_str()).bind(prepared.is_self)
            .bind(payload["remarks"].as_str()).bind(&timestamp).bind(&timestamp).bind(prepared.spec["mode"].as_str())
            .bind(prepared.spec["rule_key"].as_str()).bind(prepared.spec["host_offset"].as_i64()).execute(&mut *transaction).await?;
        binding_ops::save_snapshot(&mut transaction, owner, node, "device", &id, &prepared.snapshot).await?;
        let row = sqlx::query("SELECT * FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(&id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = device_json(&mut transaction, &row, &self.runtime_observations).await?;
        operation.finish(&mut transaction, "device", &id, &response).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn update_device(&self, owner: i32, node: &str, id: &str, body: Value) -> AssetResult<Value> {
        validation::keys(&body, &["name", "model", "remarks", "device_type_id", "overlay_ip", "lan_ip", "gateway_group_id", "is_gateway_self"])?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let old = sqlx::query("SELECT * FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_optional(&mut *transaction).await?.ok_or_else(AssetError::not_found)?;
        let original_group: Option<String> = old.try_get("gateway_group_id")?;
        if body.get("gateway_group_id").is_some() && original_group != validation::optional_id(&body, "gateway_group_id")? {
            let mut error = AssetError::conflict("asset_ownership_fixed", "A device's network node and gateway group cannot be changed after creation.");
            error.field_errors.insert("gateway_group_id".into(), "The gateway group is fixed after creation.".into());
            return Err(error);
        }
        let original_self: bool = old.try_get("is_gateway_self")?;
        let mode: String = old.try_get("address_mode")?;
        let legacy = mode == "legacy_manual" && binding_ops::stored_snapshot(&old).is_ok_and(|snapshot| snapshot.is_none());
        if !legacy {
            if body.get("overlay_ip").is_some() || body.get("lan_ip").is_some() {
                return Err(AssetError::field("address_spec", "Use the address action to review and change derived addresses."));
            }
            if body.get("is_gateway_self").is_some_and(|value| value.as_bool() != Some(original_self)) {
                return Err(AssetError::conflict("asset_ownership_fixed", "The self-device role cannot be changed after binding."));
            }
        }
        let mut merged = json!({"name":old.try_get::<String,_>("name")?,"model":old.try_get::<Option<String>,_>("model")?,
            "remarks":old.try_get::<Option<String>,_>("remarks")?,"device_type_id":old.try_get::<Option<String>,_>("device_type_id")?,
            "gateway_group_id":original_group,"is_gateway_self":original_self,"overlay_ip":old.try_get::<String,_>("overlay_ip")?,
            "lan_ip":old.try_get::<Option<String>,_>("lan_ip")?});
        for (key, value) in body.as_object().into_iter().flatten() { merged[key] = value.clone(); }
        let payload = validation::device_payload(&merged, false)?;
        validate_relations(&mut transaction, owner, node, id, &payload).await?;
        sqlx::query("UPDATE asset_devices SET device_type_id = ?, name = ?, display_name_manual = CASE WHEN ? THEN 1 ELSE display_name_manual END, model = ?, overlay_ip = ?, lan_ip = ?, is_gateway_self = ?, remarks = ?, updated_at = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(payload["device_type_id"].as_str()).bind(payload["name"].as_str()).bind(body.get("name").is_some()).bind(payload["model"].as_str())
            .bind(payload["overlay_ip"].as_str()).bind(payload["lan_ip"].as_str()).bind(payload["is_gateway_self"].as_bool().unwrap_or(false))
            .bind(payload["remarks"].as_str()).bind(now()).bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        publications::sync_device_address(&mut transaction, owner, node, id).await?;
        let row = sqlx::query("SELECT * FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = device_json(&mut transaction, &row, &self.runtime_observations).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn delete_device(&self, owner: i32, node: &str, id: &str) -> AssetResult<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_device(&mut transaction, owner, node, id).await?;
        publications::remove_device(&mut transaction, owner, node, id).await?;
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
