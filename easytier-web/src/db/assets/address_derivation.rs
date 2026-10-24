//! IPv4 address derivation for registered assets. These helpers never change a network.

use std::{collections::BTreeMap, net::Ipv4Addr};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::common::{AssetError, AssetResult};

fn cidr(value: &str, field: &str) -> AssetResult<(Ipv4Addr, u8)> {
    let (address, prefix) = value.trim().split_once('/')
        .ok_or_else(|| AssetError::field(field, "Enter an IPv4 CIDR subnet."))?;
    let address = ipv4(address, field)?;
    let prefix = prefix.parse::<u8>().ok().filter(|value| *value <= 32)
        .ok_or_else(|| AssetError::field(field, "Use an IPv4 prefix between 0 and 32."))?;
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
    let network = Ipv4Addr::from(u32::from(address) & mask);
    if network != address {
        return Err(AssetError::field(field, &format!("Use the network address {network}/{prefix}; host bits are set.")));
    }
    Ok((network, prefix))
}

pub(crate) fn ipv4(value: &str, field: &str) -> AssetResult<Ipv4Addr> {
    value.trim().parse::<Ipv4Addr>()
        .map_err(|_| AssetError::field(field, "Enter a plain IPv4 address without a port or prefix."))
}

/// Canonical, order-independent rule keys preserve identity when an instance's rules reorder.
pub(crate) fn parse_rules(raw: &[String]) -> AssetResult<Vec<Value>> {
    let mut rules = BTreeMap::new();
    for (index, value) in raw.iter().enumerate() {
        let field = format!("proxy_cidrs.{index}");
        let mut parts = value.trim().split("->");
        let (real, prefix) = cidr(parts.next().unwrap_or_default(), &field)?;
        let mapped = match parts.next() {
            Some(value) => {
                let (network, mapped_prefix) = cidr(value, &field)?;
                if mapped_prefix != prefix {
                    return Err(AssetError::field(&field, "Mapped and real subnets must have the same prefix length."));
                }
                Some(network)
            }
            None => None,
        };
        if parts.next().is_some() {
            return Err(AssetError::field(&field, "Use a subnet or real_subnet->mapped_subnet."));
        }
        let real_cidr = format!("{real}/{prefix}");
        let mapped_cidr = mapped.map(|network| format!("{network}/{prefix}"));
        let canonical = match &mapped_cidr {
            Some(mapped) => format!("{real_cidr}->{mapped}"),
            None => real_cidr.clone(),
        };
        let rule_key = format!("{:x}", Sha256::digest(canonical.as_bytes()));
        rules.insert(rule_key.clone(), json!({
            "rule_key": rule_key,
            "real_cidr": real_cidr,
            "mapped_cidr": mapped_cidr,
            "prefix_len": prefix,
        }));
    }
    Ok(rules.into_values().collect())
}

fn rule_addresses(rules: &[Value], rule_key: Option<&str>, host_offset: Option<u64>) -> AssetResult<(String, String, Vec<Value>)> {
    let key = rule_key.filter(|key| !key.is_empty())
        .ok_or_else(|| AssetError::field("rule_key", "Choose a subnet rule."))?;
    let rule = rules.iter().find(|rule| rule["rule_key"].as_str() == Some(key))
        .ok_or_else(|| AssetError::conflict("binding_rule_missing", "This subnet rule is no longer available. Refresh the gateway configuration."))?;
    let (real, prefix) = cidr(rule["real_cidr"].as_str().unwrap_or_default(), "rule_key")?;
    let mapped = match rule["mapped_cidr"].as_str() {
        Some(value) => {
            let (network, mapped_prefix) = cidr(value, "rule_key")?;
            if prefix != mapped_prefix {
                return Err(AssetError::field("rule_key", "Mapped and real subnets must have the same prefix length."));
            }
            network
        }
        None => real,
    };
    let max_offset = (1_u64 << (32 - prefix)) - 1;
    let offset = host_offset.filter(|offset| *offset <= max_offset)
        .ok_or_else(|| AssetError::field("host_offset", &format!("Enter an integer host offset between 0 and {max_offset}.")))?;
    // The range checks use u64 so a /0 network and a 32-bit host offset cannot overflow.
    let lan = u64::from(u32::from(real)) + offset;
    let overlay = u64::from(u32::from(mapped)) + offset;
    if lan > u64::from(u32::MAX) || overlay > u64::from(u32::MAX) {
        return Err(AssetError::field("host_offset", "This host offset is outside the subnet."));
    }
    let warnings = if prefix <= 30 && (offset == 0 || offset == max_offset) {
        vec![json!({
            "code": "reserved_subnet_address",
            "field": "host_offset",
            "message": "This host offset selects a network or broadcast address. Registration does not establish reachability.",
        })]
    } else {
        Vec::new()
    };
    Ok((Ipv4Addr::from(overlay as u32).to_string(), Ipv4Addr::from(lan as u32).to_string(), warnings))
}

pub(crate) fn derive_address(
    mode: &str,
    instance_ipv4: &str,
    rules: &[Value],
    rule_key: Option<&str>,
    host_offset: Option<u64>,
    lan_ip: Option<&str>,
) -> AssetResult<Value> {
    let instance = ipv4(instance_ipv4, "instance_id")?.to_string();
    let (overlay, lan, warnings) = match mode {
        "direct_instance" => {
            if rule_key.is_some() || host_offset.is_some() {
                return Err(AssetError::field("address_spec", "Direct devices do not use a gateway subnet rule."));
            }
            let lan = lan_ip.filter(|value| !value.trim().is_empty())
                .map(|value| ipv4(value, "lan_ip").map(|ip| ip.to_string())).transpose()?;
            (instance, lan, Vec::new())
        }
        "gateway_rule" => {
            if lan_ip.is_some() {
                return Err(AssetError::field("lan_ip", "The LAN address is derived from the selected gateway rule."));
            }
            let (overlay, lan, warnings) = rule_addresses(rules, rule_key, host_offset)?;
            (overlay, Some(lan), warnings)
        }
        "gateway_self" => {
            if lan_ip.is_some() {
                return Err(AssetError::field("lan_ip", "Choose a real subnet and host offset, or leave the gateway LAN address empty."));
            }
            if rule_key.is_none() && host_offset.is_none() {
                (instance, None, Vec::new())
            } else {
                let (_, lan, warnings) = rule_addresses(rules, rule_key, host_offset)?;
                (instance, Some(lan), warnings)
            }
        }
        _ => return Err(AssetError::field("address_mode", "Choose a supported device address mode.")),
    };
    Ok(json!({"overlay_ip": overlay, "lan_ip": lan, "warnings": warnings}))
}
