//! Read-only, tenant-scoped catalog of persistent network configurations.
//!
//! Runtime observations are supplied by the request layer before entering a write transaction.
//! They cannot create a bindable instance without a corresponding persistent configuration.

use std::collections::BTreeMap;

use cidr::Ipv4Cidr;
use easytier::common::config::NetworkConfig;
use easytier_core::config::DhcpConfig;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use super::{address_derivation::{ipv4, parse_rules}, common::{AssetError, AssetResult, ensure_node}};

#[derive(Clone, Debug, Default)]
pub(crate) struct RuntimeObservation {
    pub ipv4: Option<String>,
    pub online: Option<bool>,
}

pub(crate) type RuntimeObservations = BTreeMap<String, RuntimeObservation>;

const CATALOG_SQL: &str = "SELECT c.id, c.network_instance_id, c.network_config, c.source, c.disabled, c.create_time, \
    m.allocated_ipv4, d.hostname AS node_hostname, g.id AS gateway_id, g.name AS gateway_name \
    FROM user_running_network_configs c \
    LEFT JOIN network_members m ON m.user_id = c.user_id AND m.device_id = c.device_id AND m.network_id = c.network_instance_id \
    LEFT JOIN devices d ON d.user_id = c.user_id AND d.machine_id = c.device_id \
    LEFT JOIN asset_gateway_groups g ON g.owner_user_id = c.user_id AND g.machine_id = c.device_id AND g.network_instance_id = c.network_instance_id \
    WHERE c.user_id = ? AND c.device_id = ?";

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn candidate(row: &SqliteRow, exclude_group_id: Option<&str>, runtime: &RuntimeObservations) -> AssetResult<Value> {
    let instance_id: String = row.try_get("network_instance_id")?;
    let source: String = row.try_get("source")?;
    let observation = runtime.get(&instance_id);
    let config_text: String = row.try_get("network_config")?;
    // Never return the configuration or its parse error: it may contain credentials.
    let config = serde_json::from_str::<NetworkConfig>(&config_text);
    // Instance labels identify the host; network names can be shared by many nodes.
    let node_hostname: Option<String> = row.try_get("node_hostname")?;
    let mut display_name = nonempty(node_hostname.as_deref()).unwrap_or(&instance_id).to_owned();
    let mut role = "unknown";
    let mut rules = Vec::new();
    let mut selected_ip: Option<String> = None;
    let mut ip_source = "missing";
    let mut reason: Option<String> = None;
    let mut ip_policy = json!({"source":"unknown"});

    if let Ok(config) = config {
        if let Some(hostname) = nonempty(config.hostname.as_deref()) {
            display_name = hostname.to_owned();
        }
        match parse_rules(&config.proxy_cidrs) {
            Ok(parsed) => {
                role = if parsed.is_empty() { "ordinary" } else { "gateway" };
                rules = parsed;
            }
            Err(_) => reason = Some("The stored subnet rules are invalid or use unsupported IPv6 rules.".into()),
        }

        // Core enables DHCP by default when a DHCP subnet is configured, and ignores a
        // leftover virtual_ipv4 whenever DHCP is enabled. Match both decisions here.
        let dhcp = config.dhcp.unwrap_or(config.dhcp_ipv4_cidr.is_some());
        let configured = if dhcp { None } else { nonempty(config.virtual_ipv4.as_deref()) };
        let allocated: Option<String> = row.try_get("allocated_ipv4")?;
        let address = configured.map(|address| ("configured", address))
            .or_else(|| nonempty(allocated.as_deref()).map(|address| ("allocated", address)));
        if let Some((address_source, address)) = address {
            ip_policy = json!({"source":address_source,"ipv4":address});
            match ipv4(address, "instance_id") {
                Ok(address) => {
                    let address = address.to_string();
                    ip_policy = json!({"source":address_source,"ipv4":address});
                    selected_ip = Some(address);
                    ip_source = address_source;
                }
                Err(_) => {
                    role = "unknown";
                    reason.get_or_insert_with(|| format!("The {address_source} IPv4 address is invalid. Correct the instance configuration first."));
                }
            }
            if address_source == "configured" {
                let prefix = config.network_length.unwrap_or(24);
                ip_policy["prefix_len"] = json!(prefix);
                if !(0..=32).contains(&prefix) {
                    role = "unknown";
                    reason.get_or_insert_with(|| "The configured IPv4 prefix must be between 0 and 32.".into());
                }
            }
        } else {
            // The runtime value is compared separately, so loss of an observation cannot alter
            // the persistent configuration fingerprint or masquerade as a new address.
            ip_policy = json!({"source":"runtime"});
            if let Some(address) = observation.and_then(|observation| observation.ipv4.as_deref()) {
                let address = address.split_once('/').map_or(address, |(address, _)| address);
                match ipv4(address, "instance_id") {
                    Ok(address) => {
                        selected_ip = Some(address.to_string());
                        ip_source = "runtime";
                    }
                    Err(_) => {
                        reason.get_or_insert_with(|| "The observed instance IPv4 address is invalid.".into());
                    }
                }
            }
        }
        if selected_ip.is_none() {
            reason.get_or_insert_with(|| "No IPv4 address is currently available from configuration, central allocation, or runtime observation.".into());
        }
        ip_policy["dhcp"] = json!(dhcp);
        if dhcp {
            let dhcp_subnet = match config.dhcp_ipv4_cidr.as_deref() {
                Some(value) => match DhcpConfig::<Ipv4Cidr>::from_cidr(value) {
                    Ok(config) => config.cidr().map(|subnet| subnet.to_string()),
                    Err(_) => {
                        role = "unknown";
                        reason.get_or_insert_with(|| "The configured DHCP IPv4 subnet is invalid or has no usable host addresses.".into());
                        None
                    }
                },
                None => None,
            };
            // A changed DHCP pool changes the IPv4 configuration even when the latest
            // runtime observation still contains an address allocated from the old pool.
            ip_policy["dhcp_ipv4_cidr"] = json!(dhcp_subnet);
        }
    } else {
        reason = Some("The persistent instance configuration could not be parsed.".into());
    }

    let fingerprint_input = json!({"role":role,"ipv4_policy":ip_policy,"rules":rules});
    let fingerprint = format!("{:x}", Sha256::digest(fingerprint_input.to_string().as_bytes()));
    let gateway_id: Option<String> = row.try_get("gateway_id")?;
    let bound_gateway = match gateway_id.as_deref().filter(|id| Some(*id) != exclude_group_id) {
        Some(id) => Some(json!({"id":id,"name":row.try_get::<String, _>("gateway_name")?})),
        None => None,
    };
    let selectable = reason.is_none() && (role != "gateway" || bound_gateway.is_none());
    if !selectable && reason.is_none() {
        reason = Some("This instance is already bound to another gateway in this node.".into());
    }

    Ok(json!({
        "instance_id":instance_id, "display_name":display_name, "source":source, "role":role,
        "config_ref":{"id":row.try_get::<i32,_>("id")?,"create_time":row.try_get::<String,_>("create_time")?},
        "config_fingerprint":fingerprint, "ipv4":selected_ip, "ipv4_source":ip_source, "rules":rules,
        "online":observation.and_then(|observation| observation.online),
        "disabled":row.try_get::<bool,_>("disabled")?, "selectable":selectable,
        "unavailable_reason":reason, "bound_gateway":bound_gateway,
    }))
}

pub(super) async fn list_candidates(
    connection: &mut SqliteConnection, owner: i32, node: &str, role: &str,
    exclude_group_id: Option<&str>, runtime: &RuntimeObservations,
) -> AssetResult<Vec<Value>> {
    if !matches!(role, "gateway" | "ordinary") {
        return Err(AssetError::field("role", "Choose gateway or ordinary."));
    }
    ensure_node(connection, owner, node).await?;
    let rows = sqlx::query(&format!("{CATALOG_SQL} ORDER BY c.id"))
        .bind(owner).bind(node).fetch_all(&mut *connection).await?;
    let mut items = Vec::new();
    for row in rows {
        let item = candidate(&row, exclude_group_id, runtime)?;
        // Invalid configurations remain visible with their explanation, never as direct devices.
        if item["role"].as_str() == Some(role) || item["role"].as_str() == Some("unknown") {
            items.push(item);
        }
    }
    Ok(items)
}

pub(super) async fn get_candidate(
    connection: &mut SqliteConnection, owner: i32, node: &str, instance_id: &str,
    exclude_group_id: Option<&str>, runtime: &RuntimeObservations,
) -> AssetResult<Option<Value>> {
    ensure_node(connection, owner, node).await?;
    let row = sqlx::query(&format!("{CATALOG_SQL} AND c.network_instance_id = ?"))
        .bind(owner).bind(node).bind(instance_id).fetch_optional(&mut *connection).await?;
    row.map(|row| candidate(&row, exclude_group_id, runtime)).transpose()
}

/// Explicit allow-list: no future candidate field can silently enter a persisted snapshot.
pub(super) fn snapshot(candidate: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in ["instance_id", "display_name", "source", "role", "config_ref", "config_fingerprint", "ipv4", "ipv4_source", "rules"] {
        result.insert(key.into(), candidate.get(key).cloned().unwrap_or(Value::Null));
    }
    Value::Object(result)
}
