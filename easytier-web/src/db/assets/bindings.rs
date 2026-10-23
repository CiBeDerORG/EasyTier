//! Pure interpretation of accepted snapshots against the current persistent source.
//! These helpers never adopt a changed configuration or modify a saved asset address.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::address_derivation::{ipv4, parse_rules};

fn state(name: &str, reason: Option<&str>) -> Value {
    json!({"state":name,"reason":reason})
}

fn has_identity(snapshot: &Value) -> bool {
    snapshot["instance_id"].as_str().is_some_and(|value| !value.is_empty())
        && snapshot["config_ref"]["id"].as_i64().is_some_and(|value| value > 0)
        && snapshot["config_ref"]["create_time"].as_str().is_some_and(|value| !value.is_empty())
}

fn usable_snapshot(snapshot: &Value) -> bool {
    has_identity(snapshot)
        && matches!(snapshot["role"].as_str(), Some("gateway" | "ordinary"))
        && snapshot["config_fingerprint"].as_str().is_some_and(|value| !value.is_empty())
        && snapshot["ipv4"].as_str().is_some_and(|value| ipv4(value, "instance_id").is_ok())
        && matches!(snapshot["ipv4_source"].as_str(), Some("configured" | "allocated" | "runtime"))
        && usable_rules(snapshot)
}

fn usable_rules(snapshot: &Value) -> bool {
    let Some(rules) = snapshot["rules"].as_array() else { return false; };
    if (snapshot["role"] == "ordinary") != rules.is_empty() {
        return false;
    }
    let mut raw = Vec::with_capacity(rules.len());
    for rule in rules {
        let Some(real) = rule["real_cidr"].as_str() else { return false; };
        let value = match rule.get("mapped_cidr") {
            Some(Value::Null) => real.to_owned(),
            Some(Value::String(mapped)) => format!("{real}->{mapped}"),
            _ => return false,
        };
        raw.push(value);
    }
    // Persisted rule identity, prefix and normalized subnet content must agree. A damaged
    // snapshot cannot be treated as a valid legacy record or a trustworthy derivation source.
    parse_rules(&raw).is_ok_and(|parsed| parsed == *rules)
}

pub(super) fn binding_state(
    stored: Option<&Value>, current: Option<&Value>, role: &str, read_error: Option<&str>,
) -> Value {
    let Some(stored) = stored else { return state("legacy_unbound", None); };
    // None is reserved for a legitimate pre-migration record. Partial/corrupt bindings are not
    // allowed to fall back to legacy editing and thereby bypass binding restrictions.
    if !usable_snapshot(stored) {
        return state("unavailable", Some("binding_snapshot_incomplete"));
    }
    if stored["role"].as_str() != Some(role) {
        return state("unavailable", Some("binding_snapshot_role_invalid"));
    }
    if read_error.is_some() {
        return state("unavailable", Some("instance_source_unavailable"));
    }
    let Some(current) = current else { return state("missing", Some("instance_missing")); };
    if !has_identity(current) || stored["instance_id"] != current["instance_id"] {
        return state("unavailable", Some("instance_identity_invalid"));
    }
    if stored["config_ref"] != current["config_ref"] {
        return state("replaced", Some("instance_replaced"));
    }
    if !usable_snapshot(current) {
        return state("unavailable", Some("instance_configuration_unavailable"));
    }
    if current["role"].as_str() != Some(role) {
        return state("changed", Some("binding_role_mismatch"));
    }
    if stored["ipv4"] != current["ipv4"] {
        return state("changed", Some("instance_ipv4_changed"));
    }
    if stored["config_fingerprint"] != current["config_fingerprint"] {
        return state("changed", Some("instance_config_changed"));
    }
    state("current", None)
}

fn selected_rule<'a>(snapshot: &'a Value, key: &str) -> Option<&'a Value> {
    snapshot["rules"].as_array()?.iter().find(|rule| rule["rule_key"].as_str() == Some(key))
}

pub(super) fn child_state(
    stored: Option<&Value>, accepted_parent: Option<&Value>, current: Option<&Value>,
    mode: &str, rule_key: Option<&str>, read_error: Option<&str>,
) -> Value {
    if mode == "legacy_manual" {
        return state("legacy_unbound", None);
    }
    if !matches!(mode, "gateway_rule" | "gateway_self") {
        return state("unavailable", Some("binding_address_mode_invalid"));
    }
    let parent_state = binding_state(accepted_parent, current, "gateway", read_error);
    if parent_state["state"] != "current" {
        return if parent_state["state"] == "legacy_unbound" {
            state("unavailable", Some("gateway_not_bound"))
        } else {
            parent_state
        };
    }
    let Some(stored) = stored.filter(|stored| usable_snapshot(stored)) else {
        return state("unavailable", Some("binding_snapshot_incomplete"));
    };
    if stored["role"] != "gateway" {
        return state("unavailable", Some("binding_snapshot_role_invalid"));
    }
    let Some(current) = current else { return state("missing", Some("instance_missing")); };
    if stored["instance_id"] != current["instance_id"] {
        return state("unavailable", Some("instance_identity_invalid"));
    }
    // Even after the parent adopts a recreated same-ID instance, each child must explicitly
    // accept that generation. An unchanged rule key is not sufficient across generations.
    if stored["config_ref"] != current["config_ref"] {
        return state("replaced", Some("instance_replaced"));
    }
    if mode == "gateway_self" && stored["ipv4"] != current["ipv4"] {
        return state("changed", Some("instance_ipv4_changed"));
    }
    match rule_key {
        Some(key) => {
            let Some(rule) = selected_rule(current, key) else {
                return state("changed", Some("binding_rule_missing"));
            };
            if selected_rule(stored, key) != Some(rule) {
                return state("changed", Some("binding_rule_changed"));
            }
        }
        None if mode == "gateway_rule" => return state("unavailable", Some("binding_rule_missing")),
        None => {},
    }
    // Same-generation changes to unrelated rules or the gateway's own IPv4 do not rewrite or
    // invalidate an ordinary child's unchanged address rule after the gateway is confirmed.
    state("current", None)
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            let mut result = serde_json::Map::new();
            for key in keys {
                result.insert(key.clone(), canonical(&object[key.as_str()]));
            }
            Value::Object(result)
        }
        Value::Array(array) => Value::Array(array.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}

pub(super) fn preview_version(scope: &Value, subject: &Value, selection: &Value, snapshot: &Value) -> String {
    // Display names and source labels are not address inputs; renaming a network does not
    // invalidate a preview. Scope, subject, effective IP, generation and selected inputs do.
    let mut relevant = serde_json::Map::new();
    for key in ["instance_id", "role", "config_ref", "config_fingerprint", "ipv4", "ipv4_source", "rules"] {
        relevant.insert(key.into(), snapshot.get(key).cloned().unwrap_or(Value::Null));
    }
    let input = canonical(&json!({
        "scope":scope,"subject":subject,"selection":selection,"snapshot":relevant,
    }));
    format!("{:x}", Sha256::digest(input.to_string().as_bytes()))
}
