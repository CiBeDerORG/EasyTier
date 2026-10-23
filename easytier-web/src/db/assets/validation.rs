use std::{collections::HashSet, net::{IpAddr, Ipv4Addr, Ipv6Addr}};

use serde_json::{Value, json};

use super::common::{AssetError, AssetResult};

pub(super) fn keys(body: &Value, allowed: &[&str]) -> AssetResult<()> {
    let Some(object) = body.as_object() else { return Err(AssetError::field("body", "Expected an object.")); };
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(AssetError::field(key, "This field cannot be submitted here."));
    }
    Ok(())
}

pub(super) fn required_string(body: &Value, field: &str, max: usize) -> AssetResult<String> {
    let value = body.get(field).and_then(Value::as_str)
        .ok_or_else(|| AssetError::field(field, "This field is required."))?.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(AssetError::field(field, &format!("Use 1–{max} characters without control characters.")));
    }
    Ok(value.to_owned())
}

pub(super) fn optional_string(body: &Value, field: &str, max: usize) -> AssetResult<Option<String>> {
    let value = match body.get(field) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(value)) => value.trim(),
        _ => return Err(AssetError::field(field, "Expected text or an empty value.")),
    };
    if value.is_empty() { return Ok(None); }
    let bad_control = value.chars().any(|character| character.is_control() && !(field == "remarks" && matches!(character, '\n' | '\r' | '\t')));
    if value.chars().count() > max || bad_control {
        return Err(AssetError::field(field, &format!("Use at most {max} characters without invalid control characters.")));
    }
    Ok(Some(value.to_owned()))
}

pub(super) fn optional_id(body: &Value, field: &str) -> AssetResult<Option<String>> {
    optional_string(body, field, 64)?.map(|value| {
        uuid::Uuid::parse_str(&value).map(|id| id.to_string())
            .map_err(|_| AssetError::field(field, "Expected a UUID."))
    }).transpose()
}

pub(super) fn canonical_ip(input: &str, field: &str) -> AssetResult<String> {
    let address = input.parse::<IpAddr>().map_err(|_| AssetError::field(field, "Enter a plain IPv4 or IPv6 address without a port, URL, or subnet prefix."))?;
    Ok(match address {
        IpAddr::V6(value) => value.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(value)),
        value => value,
    }.to_string())
}

pub(super) fn subnets(body: &Value) -> AssetResult<Vec<String>> {
    let entries = body.get("subnets").and_then(Value::as_array)
        .ok_or_else(|| AssetError::field("subnets", "At least one subnet is required."))?;
    if entries.is_empty() { return Err(AssetError::field("subnets", "At least one subnet is required.")); }
    let mut normalized = Vec::with_capacity(entries.len());
    let mut seen = HashSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let field = format!("subnets.{index}");
        let input = entry.as_str().ok_or_else(|| AssetError::field(&field, "Enter an IPv4 or IPv6 CIDR subnet."))?.trim();
        let (address, prefix) = input.split_once('/').ok_or_else(|| AssetError::field(&field, "Enter the prefix length, for example 192.168.1.0/24."))?;
        let address = address.parse::<IpAddr>().map_err(|_| AssetError::field(&field, "Enter an IPv4 or IPv6 CIDR subnet."))?;
        let prefix = prefix.parse::<u8>().map_err(|_| AssetError::field(&field, "Invalid subnet prefix length."))?;
        let network = match address {
            IpAddr::V4(address) if prefix <= 32 => {
                let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
                IpAddr::V4(Ipv4Addr::from(u32::from(address) & mask))
            }
            IpAddr::V6(address) if prefix <= 128 => {
                let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix) };
                IpAddr::V6(Ipv6Addr::from(u128::from(address) & mask))
            }
            _ => return Err(AssetError::field(&field, "Invalid subnet prefix length.")),
        };
        // Device IPs normalize mapped IPv6 addresses to IPv4. Apply the same
        // representation to subnets wholly inside the mapped address range.
        let (canonical_network, canonical_prefix) = match network {
            IpAddr::V6(value) if prefix >= 96 => match value.to_ipv4_mapped() {
                Some(address) => (IpAddr::V4(address), prefix - 96),
                None => (network, prefix),
            },
            _ => (network, prefix),
        };
        let canonical = format!("{canonical_network}/{canonical_prefix}");
        if network != address {
            return Err(AssetError::field(&field, &format!("Enter the network address {canonical}; this value has host bits set.")));
        }
        canonical.parse::<cidr::IpCidr>().map_err(|_| AssetError::field(&field, "Invalid subnet."))?;
        if !seen.insert(canonical.clone()) {
            return Err(AssetError::field(&field, "This subnet is already present in this gateway group."));
        }
        normalized.push(canonical);
    }
    normalized.sort();
    Ok(normalized)
}

pub(super) fn matches_subnets(address: &str, subnets: &[String]) -> bool {
    let Ok(address) = address.parse::<IpAddr>() else { return false; };
    subnets.iter().any(|subnet| subnet.parse::<cidr::IpCidr>().is_ok_and(|network| network.contains(&address)))
}

pub(super) fn group_payload(body: &Value, creating: bool) -> AssetResult<Value> {
    let mut allowed = vec!["name", "remarks", "subnets"];
    if creating { allowed.push("request_id"); }
    keys(body, &allowed)?;
    Ok(json!({"name":required_string(body,"name",128)?,
        "remarks":optional_string(body,"remarks",2000)?,"subnets":subnets(body)?}))
}

pub(super) fn type_payload(body: &Value, creating: bool) -> AssetResult<Value> {
    keys(body, if creating { &["request_id", "name"] } else { &["name"] })?;
    Ok(json!({"name":required_string(body,"name",128)?}))
}

pub(super) fn device_payload(body: &Value, creating: bool) -> AssetResult<Value> {
    let mut allowed = vec!["gateway_group_id", "device_type_id", "name", "model", "overlay_ip", "lan_ip", "is_gateway_self", "remarks"];
    if creating { allowed.push("request_id"); }
    keys(body, &allowed)?;
    let group = optional_id(body, "gateway_group_id")?;
    let self_device = match body.get("is_gateway_self") {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => return Err(AssetError::field("is_gateway_self", "Expected true or false.")),
    };
    if self_device && group.is_none() { return Err(AssetError::field("is_gateway_self", "A gateway self device must belong to a gateway group.")); }
    let overlay = canonical_ip(&required_string(body, "overlay_ip", 128)?, "overlay_ip")?;
    let lan = optional_string(body,"lan_ip",128)?.map(|value| canonical_ip(&value, "lan_ip")).transpose()?;
    Ok(json!({"gateway_group_id":group,"device_type_id":optional_id(body,"device_type_id")?,
        "name":required_string(body,"name",128)?,"model":optional_string(body,"model",128)?,
        "overlay_ip":overlay,"lan_ip":lan,"is_gateway_self":self_device,
        "remarks":optional_string(body,"remarks",2000)?}))
}

pub(super) fn service_payload(body: &Value, creating: bool) -> AssetResult<Value> {
    let mut allowed = vec!["name", "protocols", "port", "remarks", "access_url", "icon"];
    if creating { allowed.push("request_id"); }
    keys(body, &allowed)?;
    let input_protocols = body.get("protocols").and_then(Value::as_array)
        .ok_or_else(|| AssetError::field("protocols", "Select TCP, UDP, or both."))?;
    if input_protocols.is_empty() || input_protocols.len() > 2 { return Err(AssetError::field("protocols", "Select TCP, UDP, or both.")); }
    let mut protocols = Vec::new();
    for input in input_protocols {
        let protocol = input.as_str().filter(|value| matches!(*value, "tcp" | "udp"))
            .ok_or_else(|| AssetError::field("protocols", "Only TCP and UDP are supported."))?;
        if protocols.contains(&protocol) { return Err(AssetError::field("protocols", "Do not select the same protocol twice.")); }
        protocols.push(protocol);
    }
    protocols.sort_unstable();
    let port = body.get("port").and_then(Value::as_u64).filter(|port| (1..=65535).contains(port))
        .ok_or_else(|| AssetError::field("port", "Enter an integer port between 1 and 65535."))?;
    let access_url = optional_string(body,"access_url",2048)?;
    if let Some(link) = &access_url {
        let lower = link.to_ascii_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return Err(AssetError::field("access_url", "Enter a complete HTTP or HTTPS URL."));
        }
        let parsed = url::Url::parse(link).map_err(|_| AssetError::field("access_url", "Enter a complete HTTP or HTTPS URL."))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() || !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(AssetError::field("access_url", "Only HTTP/HTTPS URLs without embedded credentials are supported."));
        }
    }
    let icon = optional_string(body,"icon",32)?;
    if let Some(icon) = &icon {
        if !["server","globe","database","mobile","desktop","home","cloud","cog","play","link","shield","wifi"].contains(&icon.as_str()) {
            return Err(AssetError::field("icon", "Choose an available built-in icon."));
        }
    }
    Ok(json!({"name":required_string(body,"name",128)?,"protocols":protocols,"port":port,
        "remarks":optional_string(body,"remarks",2000)?,"access_url":access_url,"icon":icon}))
}
