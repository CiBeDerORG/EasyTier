use std::collections::BTreeMap;

use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

pub(crate) type AssetResult<T> = Result<T, AssetError>;

#[derive(Debug)]
pub(crate) struct AssetError {
    pub(crate) status: u16,
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) field_errors: BTreeMap<String, String>,
}

impl AssetError {
    pub(crate) fn new(status: u16, code: &str, message: &str) -> Self {
        Self { status, code: code.into(), message: message.into(), field_errors: BTreeMap::new() }
    }

    pub(crate) fn field(field: &str, message: &str) -> Self {
        let mut error = Self::new(422, "invalid_field", "Some fields are invalid.");
        error.field_errors.insert(field.into(), message.into());
        error
    }

    pub(crate) fn not_found() -> Self {
        Self::new(404, "asset_not_found", "The requested resource no longer exists in this context.")
    }

    pub(crate) fn conflict(code: &str, message: &str) -> Self {
        Self::new(409, code, message)
    }
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AssetError {}

impl From<sqlx::Error> for AssetError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref database) = error {
            if database.is_unique_violation() {
                return Self::conflict("asset_unique_conflict", "This address, gateway self device, or protocol and port is already in use.");
            }
            if database.is_foreign_key_violation() {
                return Self::conflict("asset_relation_conflict", "The parent resource changed or this resource is still in use.");
            }
        }
        tracing::error!(error = %error, "asset database operation failed");
        Self::new(500, "asset_database_error", "The resource could not be saved or loaded.")
    }
}

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

pub(crate) async fn ensure_owner(connection: &mut SqliteConnection, owner: i32) -> AssetResult<()> {
    let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ?)")
        .bind(owner).fetch_one(&mut *connection).await?;
    if exists == 0 { return Err(AssetError::not_found()); }
    Ok(())
}

pub(crate) async fn ensure_node(connection: &mut SqliteConnection, owner: i32, node: &str) -> AssetResult<()> {
    let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE user_id = ? AND machine_id = ?)")
        .bind(owner).bind(node).fetch_one(&mut *connection).await?;
    if exists == 0 { return Err(AssetError::not_found()); }
    Ok(())
}

pub(crate) async fn ensure_group(connection: &mut SqliteConnection, owner: i32, node: &str, id: &str) -> AssetResult<()> {
    let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_gateway_groups WHERE id = ? AND owner_user_id = ? AND machine_id = ?)")
        .bind(id).bind(owner).bind(node).fetch_one(&mut *connection).await?;
    if exists == 0 { return Err(AssetError::not_found()); }
    Ok(())
}

pub(crate) async fn ensure_type(connection: &mut SqliteConnection, owner: i32, id: &str) -> AssetResult<()> {
    let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_device_types WHERE id = ? AND owner_user_id = ?)")
        .bind(id).bind(owner).fetch_one(&mut *connection).await?;
    if exists == 0 { return Err(AssetError::not_found()); }
    Ok(())
}

pub(crate) async fn ensure_device(connection: &mut SqliteConnection, owner: i32, node: &str, id: &str) -> AssetResult<()> {
    let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?)")
        .bind(id).bind(owner).bind(node).fetch_one(&mut *connection).await?;
    if exists == 0 { return Err(AssetError::not_found()); }
    Ok(())
}

pub(crate) fn type_json(row: &SqliteRow) -> AssetResult<Value> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "owner_user_id": row.try_get::<i32, _>("owner_user_id")?,
        "name": row.try_get::<String, _>("name")?,
        "semantic_kind": row.try_get::<Option<String>, _>("semantic_kind")?,
        "created_at": row.try_get::<String, _>("created_at")?,
        "updated_at": row.try_get::<String, _>("updated_at")?,
    }))
}

pub(crate) async fn group_subnets(connection: &mut SqliteConnection, owner: i32, node: &str, id: &str) -> AssetResult<Vec<String>> {
    Ok(sqlx::query_scalar::<_, String>("SELECT cidr FROM asset_gateway_subnets WHERE group_id = ? AND owner_user_id = ? AND machine_id = ? ORDER BY cidr")
        .bind(id).bind(owner).bind(node).fetch_all(&mut *connection).await?)
}

pub(crate) fn address_warning(id: Option<&str>, name: Option<&str>, lan: &str) -> Value {
    let mut warning = json!({"code":"lan_outside_gateway_subnets", "field":"lan_ip", "message":format!("{lan} is outside the registered gateway subnets.")});
    if let Some(id) = id { warning["device_id"] = json!(id); }
    if let Some(name) = name { warning["device_name"] = json!(name); }
    warning
}

pub(crate) async fn group_warnings(connection: &mut SqliteConnection, owner: i32, node: &str, group_id: &str, subnets: &[String]) -> AssetResult<Vec<Value>> {
    let rows = sqlx::query("SELECT id, name, lan_ip FROM asset_devices WHERE owner_user_id = ? AND machine_id = ? AND gateway_group_id = ? AND lan_ip IS NOT NULL ORDER BY id")
        .bind(owner).bind(node).bind(group_id).fetch_all(&mut *connection).await?;
    let mut warnings = Vec::new();
    for row in rows {
        let lan: String = row.try_get("lan_ip")?;
        if !super::validation::matches_subnets(&lan, subnets) {
            warnings.push(address_warning(Some(&row.try_get::<String, _>("id")?), Some(&row.try_get::<String, _>("name")?), &lan));
        }
    }
    Ok(warnings)
}

pub(crate) async fn group_json(connection: &mut SqliteConnection, row: &SqliteRow, runtime: &super::instances::RuntimeObservations) -> AssetResult<Value> {
    let id: String = row.try_get("id")?;
    let owner: i32 = row.try_get("owner_user_id")?;
    let node: String = row.try_get("machine_id")?;
    let subnets = group_subnets(connection, owner, &node, &id).await?;
    let warnings = group_warnings(connection, owner, &node, &id, &subnets).await?;
    let binding = super::binding_ops::binding_info(connection, row, "gateway", runtime).await?;
    Ok(json!({"id":id, "owner_user_id":owner, "machine_id":node,
        "name":row.try_get::<String,_>("name")?, "remarks":row.try_get::<Option<String>,_>("remarks")?,
        "subnets":subnets, "warnings":warnings,"overlay_ip":binding["snapshot"]["ipv4"],"binding":binding,
        "created_at":row.try_get::<String,_>("created_at")?, "updated_at":row.try_get::<String,_>("updated_at")?}))
}

pub(crate) async fn device_json(connection: &mut SqliteConnection, row: &SqliteRow, runtime: &super::instances::RuntimeObservations) -> AssetResult<Value> {
    let id: String = row.try_get("id")?;
    let owner: i32 = row.try_get("owner_user_id")?;
    let node: String = row.try_get("machine_id")?;
    let name: String = row.try_get("name")?;
    let group: Option<String> = row.try_get("gateway_group_id")?;
    let lan: Option<String> = row.try_get("lan_ip")?;
    let semantic: Option<String> = sqlx::query_scalar("SELECT semantic_kind FROM asset_device_types WHERE id=? AND owner_user_id=?")
        .bind(row.try_get::<Option<String>,_>("device_type_id")?).bind(owner).fetch_optional(&mut *connection).await?.flatten();
    let discovery: Option<String> = row.try_get("discovery_json")?;
    let mode: String = row.try_get("address_mode")?;
    let instance: Option<String> = row.try_get("network_instance_id")?;
    let rule: Option<String> = row.try_get("rule_key")?;
    let host: Option<i64> = row.try_get("host_offset")?;
    let binding = super::binding_ops::binding_info(connection, row, "device", runtime).await?;
    let address_spec = match mode.as_str() {
        "direct_instance" => json!({"mode":mode,"instance_id":instance,"lan_ip":lan}),
        "gateway_rule" | "gateway_self" => json!({"mode":mode,"gateway_group_id":group,"rule_key":rule,"host_offset":host}),
        _ => Value::Null,
    };
    let mut warnings = Vec::new();
    if let (Some(group_id), Some(lan_ip)) = (group.as_ref(), lan.as_ref()) {
        let subnets = group_subnets(connection, owner, &node, group_id).await?;
        if !super::validation::matches_subnets(lan_ip, &subnets) {
            warnings.push(address_warning(Some(&id), Some(&name), lan_ip));
        }
    }
    Ok(json!({"id":id,"owner_user_id":owner,"machine_id":node,"name":name,
        "semantic_kind":semantic,"endpoint_revision":row.try_get::<i64,_>("endpoint_revision")?,
        "discovery":discovery.as_deref().and_then(|s|serde_json::from_str::<Value>(s).ok()),
        "display_name_manual":row.try_get::<bool,_>("display_name_manual")?,
        "gateway_group_id":group,"device_type_id":row.try_get::<Option<String>,_>("device_type_id")?,
        "model":row.try_get::<Option<String>,_>("model")?,"overlay_ip":row.try_get::<String,_>("overlay_ip")?,
        "lan_ip":lan,"is_gateway_self":row.try_get::<bool,_>("is_gateway_self")?,
        "remarks":row.try_get::<Option<String>,_>("remarks")?,"warnings":warnings,
        "address_mode":mode,"rule_key":rule,"host_offset":host,"network_instance_id":instance,"binding":binding,"address_spec":address_spec,
        "created_at":row.try_get::<String,_>("created_at")?,"updated_at":row.try_get::<String,_>("updated_at")?}))
}

pub(crate) async fn service_json(connection: &mut SqliteConnection, row: &SqliteRow) -> AssetResult<Value> {
    let id: String = row.try_get("id")?;
    let owner: i32 = row.try_get("owner_user_id")?;
    let node: String = row.try_get("machine_id")?;
    let publication = super::publications::response(connection, owner, &node, &id).await?;
    let protocols = sqlx::query_scalar::<_, String>("SELECT protocol FROM asset_service_bindings WHERE service_id = ? ORDER BY protocol")
        .bind(&id).fetch_all(&mut *connection).await?;
    Ok(json!({"id":id,"owner_user_id":row.try_get::<i32,_>("owner_user_id")?,
        "machine_id":row.try_get::<String,_>("machine_id")?,"device_id":row.try_get::<String,_>("device_id")?,
        "service_type":row.try_get::<String,_>("service_type")?,"connection_mode":row.try_get::<String,_>("connection_mode")?,"endpoint_revision":row.try_get::<i64,_>("endpoint_revision")?,
        "name":row.try_get::<String,_>("name")?,"protocols":protocols,"port":row.try_get::<i32,_>("port")?,
        "overlay_ip":row.try_get::<String,_>("overlay_ip")?,"lan_ip":row.try_get::<Option<String>,_>("lan_ip")?,
        "publication":publication,"show_in_navigation":row.try_get::<bool,_>("show_in_navigation")?,
        "remarks":row.try_get::<Option<String>,_>("remarks")?,"access_url":row.try_get::<Option<String>,_>("access_url")?,
        "icon":row.try_get::<Option<String>,_>("icon")?,"created_at":row.try_get::<String,_>("created_at")?,
        "updated_at":row.try_get::<String,_>("updated_at")?}))
}
