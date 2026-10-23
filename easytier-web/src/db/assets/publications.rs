//! Service-owned port forwards. All helpers run inside the caller's asset
//! transaction so a service and its desired public mappings commit together.
use std::net::IpAddr;

use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use super::{AssetsStore, common::{AssetError, AssetResult, now}, validation};

#[path = "publication_adoption.rs"]
mod adoption;

/// Constructed only from a scoped, current runtime observation by the HTTP layer.
#[derive(Clone, Debug)]
pub(crate) struct PublicationRuntimeProof {
    pub(crate) owner_user_id: i32,
    pub(crate) machine_id: String,
    pub(crate) instance_id: String,
}

struct Publication {
    machine_id: String,
    instance_id: String,
    config_record_id: i64,
    config_create_time: String,
    input: Value,
    rules: Vec<Value>,
}

impl Publication {
    fn from_row(row: &SqliteRow) -> AssetResult<Self> {
        let machine_id: String = row.try_get("public_machine_id")?;
        let instance_id: String = row.try_get("instance_id")?;
        let rules = serde_json::from_str::<Vec<Value>>(&row.try_get::<String, _>("rules_json")?)
            .map_err(|_| invalid_saved_publication())?;
        let input = json!({"machine_id":machine_id,"instance_id":instance_id,
            "bind_ip":row.try_get::<String,_>("bind_ip")?,
            "bind_port":row.try_get::<i64,_>("bind_port")?,
            "dst_port":row.try_get::<i64,_>("dst_port")?});
        let mut expected = input.clone();
        expected["dst_ip"] = json!(row.try_get::<String,_>("dst_ip")?);
        let mut protocols = std::collections::BTreeSet::new();
        if rules.is_empty() || rules.len() > 2 || rules.iter().any(|rule| {
            let Some(proto) = rule["proto"].as_str().map(str::to_ascii_lowercase) else { return true; };
            if !matches!(proto.as_str(), "tcp" | "udp") || !protocols.insert(proto) { return true; }
            address(rule, "bind_ip").is_none() || address(rule, "bind_ip") != address(&expected, "bind_ip")
                || address(rule, "dst_ip").is_none() || address(rule, "dst_ip") != address(&expected, "dst_ip")
                || ["bind_port", "dst_port"].iter().any(|key| {
                    rule[*key] != expected[*key] || !rule[*key].as_u64().is_some_and(|port| (1..=65535).contains(&port))
                })
        }) { return Err(invalid_saved_publication()); }
        Ok(Self {
            input,
            machine_id, instance_id, rules,
            config_record_id: row.try_get("config_record_id")?,
            config_create_time: row.try_get("config_create_time")?,
        })
    }
}

struct StoredConfig {
    id: i64,
    created: String,
    source: String,
    disabled: bool,
    value: Value,
}

fn publication_conflict(code: &str, field: &str, message: &str) -> AssetError {
    let mut error = AssetError::conflict(code, message);
    error.field_errors.insert(format!("publication.{field}"), message.to_owned());
    error
}

fn invalid_saved_publication() -> AssetError {
    publication_conflict("publication_data_invalid", "instance_id", "The saved publication is incomplete. Review its network configuration before changing it.")
}

fn invalid_config() -> AssetError {
    publication_conflict("publication_config_invalid", "instance_id", "The saved network configuration cannot be safely updated. Open and save it in network settings first.")
}

pub(super) fn input(body: &Value) -> AssetResult<Value> {
    if body.is_null() { return Ok(Value::Null); }
    let normalized: AssetResult<Value> = (|| {
        validation::keys(body, &["machine_id", "instance_id", "bind_ip", "bind_port", "dst_port"])?;
        let port = |field: &str| -> AssetResult<u64> {
            body.get(field).and_then(Value::as_u64).filter(|port| (1..=65535).contains(port))
                .ok_or_else(|| AssetError::field(field, "Enter an integer port between 1 and 65535."))
        };
        let bind_ip = validation::canonical_ip(&validation::required_string(body, "bind_ip", 128)?, "bind_ip")?;
        let address: IpAddr = bind_ip.parse().map_err(|_| AssetError::field("bind_ip", "Enter a valid binding IP."))?;
        if address.is_multicast() {
            return Err(AssetError::field("bind_ip", "A multicast address cannot be used as a public listener."));
        }
        Ok(json!({"machine_id":validation::required_id(body,"machine_id")?,
            "instance_id":validation::required_id(body,"instance_id")?,"bind_ip":bind_ip,
            "bind_port":port("bind_port")?,"dst_port":port("dst_port")?}))
    })();
    normalized.map_err(|mut error: AssetError| {
        error.field_errors = error.field_errors.into_iter()
            .map(|(field, message)| (format!("publication.{field}"), message)).collect();
        error
    })
}

async fn existing(connection: &mut SqliteConnection, owner: i32, node: &str, service: &str) -> AssetResult<Option<Publication>> {
    sqlx::query("SELECT * FROM asset_service_publications WHERE service_id = ? AND owner_user_id = ? AND machine_id = ?")
        .bind(service).bind(owner).bind(node).fetch_optional(&mut *connection).await?
        .as_ref().map(Publication::from_row).transpose()
}

async fn active_owner(connection: &mut SqliteConnection, owner: i32) -> AssetResult<()> {
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND deleted_at IS NULL)")
        .bind(owner).fetch_one(&mut *connection).await?;
    if !active { return Err(AssetError::new(403, "account_deleted", "The network configuration owner is inactive.")); }
    Ok(())
}

async fn load_config(connection: &mut SqliteConnection, owner: i32, machine: &str, instance: &str) -> AssetResult<Option<StoredConfig>> {
    let Some(row) = sqlx::query("SELECT id,create_time,source,disabled,network_config FROM user_running_network_configs WHERE user_id = ? AND device_id = ? AND network_instance_id = ?")
        .bind(owner).bind(machine).bind(instance).fetch_optional(&mut *connection).await? else { return Ok(None); };
    let value: Value = serde_json::from_str(&row.try_get::<String, _>("network_config")?).map_err(|_| invalid_config())?;
    if !value.is_object() { return Err(invalid_config()); }
    Ok(Some(StoredConfig { id: row.try_get("id")?, created: row.try_get("create_time")?,
        source: row.try_get("source")?, disabled: row.try_get("disabled")?, value }))
}

async fn mutable_config(connection: &mut SqliteConnection, owner: i32, machine: &str, instance: &str, config: &StoredConfig) -> AssetResult<()> {
    active_owner(connection, owner).await?;
    let config_instance = config.value["instance_id"].as_str().and_then(|id| uuid::Uuid::parse_str(id).ok());
    if config_instance.is_none() || config_instance != uuid::Uuid::parse_str(instance).ok() {
        return Err(publication_conflict("publication_config_identity_changed", "instance_id", "The saved configuration has a different instance ID. Review and save the network configuration before binding a service."));
    }
    if config.source != "web" {
        return Err(publication_conflict("publication_config_owned_elsewhere", "instance_id", "This network is no longer managed by Web. Review the existing mapping in its configuration."));
    }
    let central: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM networks n JOIN network_members m ON n.user_id = m.user_id AND n.id = m.network_id \
         WHERE m.user_id = ? AND m.device_id = ? AND (n.id = ? OR n.network_name = ?))",
    ).bind(owner).bind(machine).bind(instance).bind(config.value["network_name"].as_str().unwrap_or_default())
        .fetch_one(&mut *connection).await?;
    if central {
        return Err(publication_conflict("publication_central_network", "instance_id", "This network is managed by the central network controller and cannot be changed by a service publication."));
    }
    Ok(())
}

fn forwards(value: &Value) -> AssetResult<Vec<Value>> {
    if !value.is_object() { return Err(invalid_config()); }
    match value.get("port_forwards") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(rules)) => Ok(rules.clone()),
        _ => Err(invalid_config()),
    }
}

fn address(value: &Value, field: &str) -> Option<IpAddr> {
    let text = value.get(field)?.as_str()?;
    let text = text.strip_prefix('[').and_then(|text| text.strip_suffix(']')).unwrap_or(text);
    match text.parse::<IpAddr>().ok()? {
        IpAddr::V6(value) => Some(value.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(value))),
        value => Some(value),
    }
}

fn same_rule(left: &Value, right: &Value) -> bool {
    left["proto"].as_str().map(str::to_ascii_lowercase) == right["proto"].as_str().map(str::to_ascii_lowercase)
        && ["bind_port", "dst_port"].iter().all(|key| left[*key] == right[*key])
        && address(left, "bind_ip").is_some() && address(left, "bind_ip") == address(right, "bind_ip")
        && address(left, "dst_ip").is_some() && address(left, "dst_ip") == address(right, "dst_ip")
}

fn listeners_overlap(left: &Value, right: &Value) -> bool {
    if left["proto"].as_str().map(str::to_ascii_lowercase) != right["proto"].as_str().map(str::to_ascii_lowercase)
        || left["bind_port"] != right["bind_port"] { return false; }
    let (Some(left), Some(right)) = (address(left, "bind_ip"), address(right, "bind_ip")) else { return true; };
    // An IPv6 wildcard can also reserve the IPv4 listener on dual-stack hosts.
    left == right || left.is_unspecified() || right.is_unspecified()
}

fn wire_ip(text: &str) -> String {
    // Older NetworkConfig::gen_config versions combine these fields with
    // ':port'. Brackets preserve compatibility; asset responses use plain IPs.
    if text.parse::<IpAddr>().is_ok_and(|ip| ip.is_ipv6()) { format!("[{text}]") } else { text.to_owned() }
}

fn desired_rules(input: &Value, protocols: &Value, dst_ip: &str) -> AssetResult<Vec<Value>> {
    let dst: IpAddr = dst_ip.parse().map_err(|_| AssetError::field("publication.dst_ip", "The device needs a valid network IP before its service can be public."))?;
    let dst = match dst {
        IpAddr::V6(address) => address.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(address)),
        address => address,
    };
    if dst.is_unspecified() || dst.is_multicast() || dst.is_loopback() {
        return Err(AssetError::field("publication.dst_ip", "The device needs a usable network IP, not a loopback or wildcard address, before its service can be public."));
    }
    Ok(protocols.as_array().into_iter().flatten().map(|protocol| json!({
        "proto":protocol,"bind_ip":wire_ip(input["bind_ip"].as_str().unwrap_or_default()),
        "bind_port":input["bind_port"],"dst_ip":wire_ip(dst_ip),"dst_port":input["dst_port"]
    })).collect())
}

async fn save_config(connection: &mut SqliteConnection, owner: i32, machine: &str, instance: &str, config: &StoredConfig, rules: Vec<Value>) -> AssetResult<()> {
    mutable_config(connection, owner, machine, instance, config).await?;
    let mut value = config.value.clone();
    value["port_forwards"] = json!(rules);
    let updated = sqlx::query("UPDATE user_running_network_configs SET network_config = ?, update_time = ? WHERE id = ? AND user_id = ? AND device_id = ? AND network_instance_id = ? AND create_time = ? AND source = 'web'")
        .bind(value.to_string()).bind(chrono::Local::now().fixed_offset()).bind(config.id)
        .bind(owner).bind(machine).bind(instance).bind(&config.created).execute(&mut *connection).await?;
    if updated.rows_affected() != 1 {
        return Err(publication_conflict("publication_config_changed", "instance_id", "The network configuration changed. Refresh and retry."));
    }
    sqlx::query("DELETE FROM managed_config_revisions WHERE user_id = ? AND device_id = ?")
        .bind(owner).bind(machine).execute(&mut *connection).await?;
    Ok(())
}

async fn remove_owned(connection: &mut SqliteConnection, owner: i32, publication: &Publication) -> AssetResult<()> {
    let Some(config) = load_config(connection, owner, &publication.machine_id, &publication.instance_id).await? else { return Ok(()); };
    if config.id != publication.config_record_id || config.created != publication.config_create_time {
        // A deleted/recreated network is not the configuration this service owned.
        return Ok(());
    }
    let mut rules = forwards(&config.value)?;
    let mut changed = false;
    for owned in &publication.rules {
        if let Some(index) = rules.iter().position(|rule| same_rule(rule, owned)) {
            rules.remove(index);
            changed = true;
        } else if rules.iter().any(|rule| listeners_overlap(rule, owned)) {
            return Err(publication_conflict("publication_mapping_changed", "bind_port", "The public mapping was changed outside this service. Review that network's port mappings before saving."));
        }
    }
    if changed { save_config(connection, owner, &publication.machine_id, &publication.instance_id, &config, rules).await?; }
    Ok(())
}

async fn available_listeners(connection: &mut SqliteConnection, owner: i32, machine: &str, service: &str, desired: &[Value]) -> AssetResult<()> {
    let configurations: Vec<String> = sqlx::query_scalar("SELECT network_config FROM user_running_network_configs WHERE user_id = ? AND device_id = ?")
        .bind(owner).bind(machine).fetch_all(&mut *connection).await?;
    for raw in configurations {
        let value: Value = serde_json::from_str(&raw).map_err(|_| invalid_config())?;
        if forwards(&value)?.iter().any(|rule| desired.iter().any(|new| listeners_overlap(rule, new))) {
            return Err(publication_conflict("publication_port_in_use", "bind_port", "This protocol and binding port are already used by another mapping on the public server."));
        }
    }
    // Keep another service's reservation even if its network was removed or a
    // manual edit temporarily removed the corresponding desired rule.
    let reservations: Vec<String> = sqlx::query_scalar("SELECT rules_json FROM asset_service_publications WHERE owner_user_id = ? AND public_machine_id = ? AND service_id <> ?")
        .bind(owner).bind(machine).bind(service).fetch_all(&mut *connection).await?;
    for raw in reservations {
        let rules: Vec<Value> = serde_json::from_str(&raw).map_err(|_| invalid_saved_publication())?;
        if rules.iter().any(|rule| desired.iter().any(|new| listeners_overlap(rule, new))) {
            return Err(publication_conflict("publication_port_in_use", "bind_port", "This protocol and binding port are reserved by another public service."));
        }
    }
    Ok(())
}

pub(super) async fn sync_service(connection: &mut SqliteConnection, owner: i32, node: &str, device: &str, service: &str, payload: &Value, proof: Option<&PublicationRuntimeProof>) -> AssetResult<()> {
    let old = existing(connection, owner, node, service).await?;
    let requested = payload.get("publication");
    let desired = requested.cloned().or_else(|| old.as_ref().map(|old| old.input.clone())).unwrap_or(Value::Null);
    if desired.is_null() {
        if let Some(old) = old { remove_owned(connection, owner, &old).await?; }
        sqlx::query("DELETE FROM asset_service_publications WHERE service_id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(service).bind(owner).bind(node).execute(&mut *connection).await?;
        return Ok(());
    }
    let dst_ip: String = sqlx::query_scalar("SELECT overlay_ip FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
        .bind(device).bind(owner).bind(node).fetch_one(&mut *connection).await?;
    let rules = desired_rules(&desired, &payload["protocols"], &dst_ip)?;
    let machine = desired["machine_id"].as_str().ok_or_else(invalid_saved_publication)?;
    let instance = desired["instance_id"].as_str().ok_or_else(invalid_saved_publication)?;
    let mut owned_intact = false;
    if let Some(old) = &old {
        if old.input == desired {
            if let Some(config) = load_config(connection, owner, machine, instance).await? {
                let config_instance = config.value["instance_id"].as_str().and_then(|id| uuid::Uuid::parse_str(id).ok());
                if config.id == old.config_record_id && config.created == old.config_create_time
                    && config_instance.is_some() && config_instance == uuid::Uuid::parse_str(instance).ok() {
                    let current = forwards(&config.value)?;
                    owned_intact = old.rules.iter().all(|owned| current.iter().any(|rule| same_rule(rule, owned)));
                }
            }
            // Editing a name/remark remains possible while the public node is
            // offline. Recreating an externally removed mapping needs proof.
            if owned_intact && old.rules.len() == rules.len()
                && old.rules.iter().all(|old| rules.iter().any(|rule| same_rule(old, rule))) {
                return Ok(());
            }
        }
    }
    // Older clients omit publication entirely. Their protocol changes follow
    // an intact existing binding, just like inherited device IP changes; they
    // cannot create, retarget or restore an externally removed public mapping.
    let requires_runtime = requested.is_some() || !owned_intact;
    if requires_runtime && !proof.is_some_and(|proof| proof.owner_user_id == owner && proof.machine_id == machine && proof.instance_id == instance) {
        return Err(publication_conflict("publication_runtime_required", "instance_id", "Choose a running, writable User or Web network on the selected public server and retry."));
    }
    let server_name: Option<String> = sqlx::query_scalar(
        "SELECT COALESCE(NULLIF(d.alias,''),NULLIF(d.hostname,''),d.machine_id) FROM devices d JOIN node_asset_metadata m \
         ON d.user_id = m.owner_user_id AND d.machine_id = m.machine_id WHERE d.user_id = ? AND d.machine_id = ? AND m.is_public_server = 1",
    ).bind(owner).bind(machine).fetch_optional(&mut *connection).await?;
    let server_name = server_name.ok_or_else(|| publication_conflict("publication_server_unavailable", "machine_id", "Choose a public server owned by this account."))?;
    let mut config = load_config(connection, owner, machine, instance).await?
        .ok_or_else(|| publication_conflict("publication_network_unavailable", "instance_id", "This running network needs a saved configuration before it can host a service."))?;
    if requires_runtime && config.disabled {
        return Err(publication_conflict("publication_network_disabled", "instance_id", "This network is disabled. Start it before binding a service."));
    }
    adoption::ensure_web_config(connection, owner, machine, instance, &mut config, proof).await?;
    if let Some(old) = &old { remove_owned(connection, owner, old).await?; }
    available_listeners(connection, owner, machine, service, &rules).await?;
    // Removing the old mapping may have changed this very configuration.
    let config = load_config(connection, owner, machine, instance).await?.ok_or_else(invalid_config)?;
    let mut combined = forwards(&config.value)?;
    combined.extend(rules.iter().cloned());
    save_config(connection, owner, machine, instance, &config, combined).await?;
    let network_name = config.value["network_name"].as_str().filter(|name| !name.is_empty()).unwrap_or(instance);
    let timestamp = now();
    sqlx::query(
        "INSERT INTO asset_service_publications(service_id,owner_user_id,machine_id,device_id,public_machine_id,instance_id,config_record_id,config_create_time,bind_ip,bind_port,dst_ip,dst_port,server_name,network_name,rules_json,created_at,updated_at) \
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(service_id) DO UPDATE SET public_machine_id=excluded.public_machine_id,instance_id=excluded.instance_id,config_record_id=excluded.config_record_id,config_create_time=excluded.config_create_time,bind_ip=excluded.bind_ip,bind_port=excluded.bind_port,dst_ip=excluded.dst_ip,dst_port=excluded.dst_port,server_name=excluded.server_name,network_name=excluded.network_name,rules_json=excluded.rules_json,updated_at=excluded.updated_at",
    ).bind(service).bind(owner).bind(node).bind(device).bind(machine).bind(instance).bind(config.id).bind(&config.created)
        .bind(desired["bind_ip"].as_str()).bind(desired["bind_port"].as_i64()).bind(&dst_ip).bind(desired["dst_port"].as_i64())
        .bind(server_name).bind(network_name).bind(json!(rules).to_string()).bind(&timestamp).bind(&timestamp)
        .execute(&mut *connection).await?;
    Ok(())
}

pub(super) async fn remove_service(connection: &mut SqliteConnection, owner: i32, node: &str, service: &str) -> AssetResult<()> {
    if let Some(publication) = existing(connection, owner, node, service).await? {
        remove_owned(connection, owner, &publication).await?;
        sqlx::query("DELETE FROM asset_service_publications WHERE service_id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(service).bind(owner).bind(node).execute(&mut *connection).await?;
    }
    Ok(())
}

pub(super) async fn remove_device(connection: &mut SqliteConnection, owner: i32, node: &str, device: &str) -> AssetResult<()> {
    let services: Vec<String> = sqlx::query_scalar("SELECT service_id FROM asset_service_publications WHERE owner_user_id = ? AND machine_id = ? AND device_id = ?")
        .bind(owner).bind(node).bind(device).fetch_all(&mut *connection).await?;
    for service in services { remove_service(connection, owner, node, &service).await?; }
    Ok(())
}

/// Address changes preserve each existing binding and update only its target IP.
pub(super) async fn sync_device_address(connection: &mut SqliteConnection, owner: i32, node: &str, device: &str) -> AssetResult<()> {
    let dst_ip: String = sqlx::query_scalar("SELECT overlay_ip FROM asset_devices WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
        .bind(device).bind(owner).bind(node).fetch_one(&mut *connection).await?;
    let rows = sqlx::query("SELECT * FROM asset_service_publications WHERE owner_user_id = ? AND machine_id = ? AND device_id = ? AND dst_ip <> ?")
        .bind(owner).bind(node).bind(device).bind(&dst_ip).fetch_all(&mut *connection).await?;
    for row in rows {
        let service: String = row.try_get("service_id")?;
        let old = Publication::from_row(&row)?;
        let protocols: Vec<String> = sqlx::query_scalar("SELECT protocol FROM asset_service_bindings WHERE service_id = ? ORDER BY protocol")
            .bind(&service).fetch_all(&mut *connection).await?;
        let rules = desired_rules(&old.input, &json!(protocols), &dst_ip)?;
        let config = load_config(connection, owner, &old.machine_id, &old.instance_id).await?
            .ok_or_else(|| publication_conflict("publication_network_unavailable", "instance_id", "Unpublish this device's services or restore their public network before changing its IP."))?;
        if config.id != old.config_record_id || config.created != old.config_create_time {
            return Err(publication_conflict("publication_network_replaced", "instance_id", "The public network was replaced. Rebind or unpublish this device's services before changing its IP."));
        }
        let current = forwards(&config.value)?;
        if !old.rules.iter().all(|owned| current.iter().any(|rule| same_rule(rule, owned))) {
            return Err(publication_conflict("publication_mapping_changed", "instance_id", "A public mapping was removed or changed outside this service. Rebind or unpublish the service before changing the device IP."));
        }
        remove_owned(connection, owner, &old).await?;
        available_listeners(connection, owner, &old.machine_id, &service, &rules).await?;
        let config = load_config(connection, owner, &old.machine_id, &old.instance_id).await?.ok_or_else(invalid_config)?;
        let mut combined = forwards(&config.value)?;
        combined.extend(rules.iter().cloned());
        save_config(connection, owner, &old.machine_id, &old.instance_id, &config, combined).await?;
        sqlx::query("UPDATE asset_service_publications SET dst_ip = ?, rules_json = ?, updated_at = ? WHERE service_id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(&dst_ip).bind(json!(rules).to_string()).bind(now()).bind(&service).bind(owner).bind(node).execute(&mut *connection).await?;
    }
    Ok(())
}

pub(super) async fn response(connection: &mut SqliteConnection, owner: i32, node: &str, service: &str) -> AssetResult<Value> {
    let Some(row) = sqlx::query("SELECT * FROM asset_service_publications WHERE service_id = ? AND owner_user_id = ? AND machine_id = ?")
        .bind(service).bind(owner).bind(node).fetch_optional(&mut *connection).await? else { return Ok(Value::Null); };
    let publication = Publication::from_row(&row)?;
    let mut result = publication.input;
    result["server_name"] = json!(row.try_get::<String,_>("server_name")?);
    result["network_name"] = json!(row.try_get::<String,_>("network_name")?);
    result["dst_ip"] = json!(row.try_get::<String,_>("dst_ip")?);
    result["protocols"] = json!(publication.rules.iter().filter_map(|rule| rule["proto"].as_str()).collect::<Vec<_>>());
    Ok(result)
}

impl AssetsStore {
    pub(crate) async fn publication_targets_for_node(&self, owner: i32, node: &str) -> AssetResult<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT DISTINCT public_machine_id FROM asset_service_publications WHERE owner_user_id = ? AND machine_id = ? ORDER BY public_machine_id")
            .bind(owner).bind(node).fetch_all(&self.pool).await?)
    }
}
