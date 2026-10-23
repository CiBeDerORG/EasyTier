//! Pure contract tests: address examples are independent expected network values,
//! and state cases exercise the differences between parent and child acceptance.
use serde_json::{Value, json};

use super::{
    address_derivation::{derive_address, parse_rules},
    bindings::{binding_state, child_state, preview_version},
};

fn rules(values: &[&str]) -> Vec<Value> {
    parse_rules(
        &values
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn key_for<'a>(values: &'a [Value], real: &str) -> &'a str {
    values
        .iter()
        .find(|rule| rule["real_cidr"] == real)
        .unwrap()["rule_key"]
        .as_str()
        .unwrap()
}

fn snapshot(proxy_rules: &[&str]) -> Value {
    json!({
        "instance_id":"ba590155-c7f9-4a80-b721-90acfe42947b",
        "display_name":"Gateway", "source":"web",
        "role":if proxy_rules.is_empty() { "ordinary" } else { "gateway" },
        "config_ref":{"id":7,"create_time":"2026-10-23T00:00:00Z"},
        "config_fingerprint":"accepted-config", "ipv4":"10.90.0.1",
        "ipv4_source":"configured", "rules":rules(proxy_rules),
    })
}

fn assert_state(result: Value, name: &str, reason: Option<&str>) {
    assert_eq!(result["state"], name);
    assert_eq!(result["reason"].as_str(), reason);
}

#[test]
fn proxy_and_mapping_use_real_lan_and_mapped_overlay_addresses() {
    let values = rules(&["192.168.1.0/24", "192.168.2.0/24 -> 10.20.2.0/24"]);
    let proxy = derive_address(
        "gateway_rule",
        "10.90.0.1",
        &values,
        Some(key_for(&values, "192.168.1.0/24")),
        Some(20),
        None,
    )
    .unwrap();
    assert_eq!(proxy["lan_ip"], "192.168.1.20");
    assert_eq!(proxy["overlay_ip"], "192.168.1.20");
    let mapped = derive_address(
        "gateway_rule",
        "10.90.0.1",
        &values,
        Some(key_for(&values, "192.168.2.0/24")),
        Some(42),
        None,
    )
    .unwrap();
    assert_eq!(mapped["lan_ip"], "192.168.2.42");
    assert_eq!(mapped["overlay_ip"], "10.20.2.42");
    assert_eq!(mapped["warnings"], json!([]));
}

#[test]
fn non_24_offsets_carry_across_octets_and_reject_values_outside_the_subnet() {
    let values = rules(&["192.168.2.0/23->10.40.2.0/23"]);
    let key = key_for(&values, "192.168.2.0/23");
    let carried = derive_address(
        "gateway_rule",
        "10.90.0.1",
        &values,
        Some(key),
        Some(256),
        None,
    )
    .unwrap();
    assert_eq!(carried["lan_ip"], "192.168.3.0");
    assert_eq!(carried["overlay_ip"], "10.40.3.0");
    assert_eq!(carried["warnings"], json!([]));
    let end = derive_address(
        "gateway_rule",
        "10.90.0.1",
        &values,
        Some(key),
        Some(511),
        None,
    )
    .unwrap();
    assert_eq!(end["lan_ip"], "192.168.3.255");
    assert_eq!(end["overlay_ip"], "10.40.3.255");
    assert_eq!(end["warnings"][0]["code"], "reserved_subnet_address");
    let invalid = derive_address(
        "gateway_rule",
        "10.90.0.1",
        &values,
        Some(key),
        Some(512),
        None,
    )
    .unwrap_err();
    assert_eq!(invalid.status, 422);
    assert!(invalid.field_errors.contains_key("host_offset"));
}

#[test]
fn full_ipv4_range_does_not_wrap_and_reserved_addresses_are_warnings() {
    let values = rules(&["0.0.0.0/0"]);
    let key = key_for(&values, "0.0.0.0/0");
    for (offset, expected) in [(0, "0.0.0.0"), (u64::from(u32::MAX), "255.255.255.255")] {
        let result = derive_address(
            "gateway_rule",
            "10.90.0.1",
            &values,
            Some(key),
            Some(offset),
            None,
        )
        .unwrap();
        assert_eq!(result["lan_ip"], expected);
        assert_eq!(result["overlay_ip"], expected);
        assert_eq!(result["warnings"][0]["code"], "reserved_subnet_address");
    }
    assert_eq!(
        derive_address(
            "gateway_rule",
            "10.90.0.1",
            &values,
            Some(key),
            Some(u64::from(u32::MAX) + 1),
            None
        )
        .unwrap_err()
        .status,
        422
    );
}

#[test]
fn point_to_point_and_host_routes_allow_their_entire_host_ranges() {
    for (raw, real, mapped, offset) in [
        (
            "192.0.2.10/31->198.51.100.10/31",
            "192.0.2.10",
            "198.51.100.10",
            0,
        ),
        (
            "192.0.2.10/31->198.51.100.10/31",
            "192.0.2.11",
            "198.51.100.11",
            1,
        ),
        (
            "192.0.2.99/32->198.51.100.7/32",
            "192.0.2.99",
            "198.51.100.7",
            0,
        ),
    ] {
        let values = rules(&[raw]);
        let result = derive_address(
            "gateway_rule",
            "10.90.0.1",
            &values,
            values[0]["rule_key"].as_str(),
            Some(offset),
            None,
        )
        .unwrap();
        assert_eq!(result["lan_ip"], real);
        assert_eq!(result["overlay_ip"], mapped);
        assert_eq!(result["warnings"], json!([]));
    }
    let values = rules(&["192.0.2.99/32"]);
    assert_eq!(
        derive_address(
            "gateway_rule",
            "10.90.0.1",
            &values,
            values[0]["rule_key"].as_str(),
            Some(1),
            None
        )
        .unwrap_err()
        .status,
        422
    );
}

#[test]
fn subnet_rules_require_ipv4_network_addresses_and_matching_prefixes() {
    for raw in [
        "192.168.1.7/24",
        "192.168.1.0/33",
        "192.168.1.0/-1",
        "192.168.1.0",
        "2001:db8::/64",
        "192.168.1.0/24->10.1.0.0/16",
        "192.168.1.0/24->10.1.0.7/24",
        "192.168.1.0/24->10.1.0.0/24->10.2.0.0/24",
        "192.168.1.0/24->",
    ] {
        let error = parse_rules(&[raw.to_owned()]).unwrap_err();
        assert_eq!(error.status, 422, "{raw}");
        assert!(error.field_errors.contains_key("proxy_cidrs.0"), "{raw}");
    }
    let error = parse_rules(&["192.168.1.7/24".into()]).unwrap_err();
    assert!(error.field_errors["proxy_cidrs.0"].contains("192.168.1.0/24"));
}

#[test]
fn rule_identity_is_stable_across_reordering_whitespace_and_duplicates() {
    let values = rules(&["192.168.1.0/24", "192.168.2.0/24->10.20.2.0/24"]);
    let reordered = rules(&[
        " 192.168.2.0/24 -> 10.20.2.0/24 ",
        "192.168.1.0/24",
        "192.168.1.0/24",
    ]);
    assert_eq!(values, reordered);
    let changed = rules(&["192.168.2.0/24->10.30.2.0/24"]);
    assert_ne!(
        key_for(&values, "192.168.2.0/24"),
        key_for(&changed, "192.168.2.0/24")
    );
}

#[test]
fn gateway_self_keeps_instance_overlay_even_when_its_lan_uses_a_mapping_rule() {
    let values = rules(&["192.168.1.0/24->10.20.1.0/24"]);
    let without_lan =
        derive_address("gateway_self", "10.90.0.1", &values, None, None, None).unwrap();
    assert_eq!(without_lan["overlay_ip"], "10.90.0.1");
    assert!(without_lan["lan_ip"].is_null());
    let with_lan = derive_address(
        "gateway_self",
        "10.90.0.1",
        &values,
        values[0]["rule_key"].as_str(),
        Some(1),
        None,
    )
    .unwrap();
    assert_eq!(with_lan["overlay_ip"], "10.90.0.1");
    assert_eq!(with_lan["lan_ip"], "192.168.1.1");
    assert_eq!(
        derive_address(
            "gateway_self",
            "10.90.0.1",
            &values,
            values[0]["rule_key"].as_str(),
            None,
            None
        )
        .unwrap_err()
        .status,
        422
    );
    assert_eq!(
        derive_address("gateway_self", "10.90.0.1", &values, None, Some(1), None)
            .unwrap_err()
            .status,
        422
    );
}

#[test]
fn modes_reject_manual_derived_addresses_and_invalid_source_selections() {
    let values = rules(&["192.168.1.0/24"]);
    let key = values[0]["rule_key"].as_str();
    assert_eq!(
        derive_address(
            "gateway_rule",
            "10.90.0.1",
            &values,
            key,
            Some(1),
            Some("192.168.1.9")
        )
        .unwrap_err()
        .status,
        422
    );
    assert_eq!(
        derive_address(
            "gateway_self",
            "10.90.0.1",
            &values,
            None,
            None,
            Some("192.168.1.9")
        )
        .unwrap_err()
        .status,
        422
    );
    assert_eq!(
        derive_address("direct_instance", "10.90.0.1", &values, key, Some(1), None)
            .unwrap_err()
            .status,
        422
    );
    let missing = derive_address(
        "gateway_rule",
        "10.90.0.1",
        &values,
        Some("removed-key"),
        Some(1),
        None,
    )
    .unwrap_err();
    assert_eq!(
        (missing.status, missing.code.as_str()),
        (409, "binding_rule_missing")
    );
    for invalid in ["2001:db8::1", "10.90.0.1/24", "10.90.0.1:80", "nas.example"] {
        assert_eq!(
            derive_address("direct_instance", invalid, &[], None, None, None)
                .unwrap_err()
                .status,
            422
        );
        assert_eq!(
            derive_address(
                "direct_instance",
                "10.90.0.1",
                &[],
                None,
                None,
                Some(invalid)
            )
            .unwrap_err()
            .status,
            422
        );
    }
    let direct = derive_address(
        "direct_instance",
        "10.90.0.1",
        &[],
        None,
        None,
        Some(" 192.168.1.10 "),
    )
    .unwrap();
    assert_eq!(direct["overlay_ip"], "10.90.0.1");
    assert_eq!(direct["lan_ip"], "192.168.1.10");
}

#[test]
fn binding_states_distinguish_legacy_missing_temporary_failure_and_recreated_sources() {
    let stored = snapshot(&["192.168.1.0/24"]);
    assert_state(
        binding_state(None, None, "gateway", Some("read error")),
        "legacy_unbound",
        None,
    );
    assert_state(
        binding_state(Some(&stored), Some(&stored), "gateway", None),
        "current",
        None,
    );
    assert_state(
        binding_state(Some(&stored), None, "gateway", None),
        "missing",
        Some("instance_missing"),
    );
    assert_state(
        binding_state(Some(&stored), None, "gateway", Some("read error")),
        "unavailable",
        Some("instance_source_unavailable"),
    );
    let mut recreated = stored.clone();
    recreated["config_ref"]["id"] = json!(8);
    recreated["config_ref"]["create_time"] = json!("2026-10-23T00:01:00Z");
    recreated["ipv4"] = Value::Null;
    assert_state(
        binding_state(Some(&stored), Some(&recreated), "gateway", None),
        "replaced",
        Some("instance_replaced"),
    );
}

#[test]
fn binding_state_identifies_role_address_and_relevant_configuration_changes() {
    let stored = snapshot(&["192.168.1.0/24"]);
    let ordinary = snapshot(&[]);
    assert_state(
        binding_state(Some(&stored), Some(&ordinary), "gateway", None),
        "changed",
        Some("binding_role_mismatch"),
    );
    assert_state(
        binding_state(Some(&ordinary), Some(&stored), "ordinary", None),
        "changed",
        Some("binding_role_mismatch"),
    );
    let mut current = stored.clone();
    current["ipv4"] = json!("10.90.0.2");
    assert_state(
        binding_state(Some(&stored), Some(&current), "gateway", None),
        "changed",
        Some("instance_ipv4_changed"),
    );
    current = stored.clone();
    current["config_fingerprint"] = json!("new-config");
    assert_state(
        binding_state(Some(&stored), Some(&current), "gateway", None),
        "changed",
        Some("instance_config_changed"),
    );
    current = stored.clone();
    current["display_name"] = json!("Renamed network");
    assert_state(
        binding_state(Some(&stored), Some(&current), "gateway", None),
        "current",
        None,
    );
}

#[test]
fn incomplete_or_inconsistent_snapshots_cannot_be_treated_as_legacy_or_current() {
    let stored = snapshot(&["192.168.1.0/24"]);
    assert_state(
        binding_state(Some(&Value::Null), Some(&stored), "gateway", None),
        "unavailable",
        Some("binding_snapshot_incomplete"),
    );
    let mut damaged = stored.clone();
    damaged["rules"][0]["prefix_len"] = json!(25);
    assert_state(
        binding_state(Some(&damaged), Some(&stored), "gateway", None),
        "unavailable",
        Some("binding_snapshot_incomplete"),
    );
    let mut current = stored.clone();
    current["instance_id"] = json!("c1c6078a-c15a-42b1-b14b-7ec69f8b016a");
    assert_state(
        binding_state(Some(&stored), Some(&current), "gateway", None),
        "unavailable",
        Some("instance_identity_invalid"),
    );
    current = stored.clone();
    current["ipv4"] = Value::Null;
    current["ipv4_source"] = json!("missing");
    assert_state(
        binding_state(Some(&stored), Some(&current), "gateway", None),
        "unavailable",
        Some("instance_configuration_unavailable"),
    );
}

#[test]
fn children_keep_legacy_and_require_a_confirmed_parent_before_using_new_sources() {
    let stored = snapshot(&["192.168.1.0/24"]);
    let key = stored["rules"][0]["rule_key"].as_str();
    assert_state(
        child_state(None, None, None, "legacy_manual", None, Some("offline")),
        "legacy_unbound",
        None,
    );
    assert_state(
        child_state(
            Some(&stored),
            None,
            Some(&stored),
            "gateway_rule",
            key,
            None,
        ),
        "unavailable",
        Some("gateway_not_bound"),
    );
    assert_state(
        child_state(
            Some(&stored),
            Some(&stored),
            None,
            "gateway_rule",
            key,
            None,
        ),
        "missing",
        Some("instance_missing"),
    );
    let mut current = stored.clone();
    current["config_fingerprint"] = json!("new-configuration");
    assert_state(
        child_state(
            Some(&stored),
            Some(&stored),
            Some(&current),
            "gateway_rule",
            key,
            None,
        ),
        "changed",
        Some("instance_config_changed"),
    );
    assert_state(
        child_state(
            Some(&stored),
            Some(&current),
            Some(&current),
            "gateway_rule",
            key,
            None,
        ),
        "current",
        None,
    );
}

#[test]
fn confirming_a_recreated_parent_does_not_silently_confirm_its_children() {
    let stored = snapshot(&["192.168.1.0/24"]);
    let key = stored["rules"][0]["rule_key"].as_str();
    let mut recreated = stored.clone();
    recreated["config_ref"]["id"] = json!(8);
    recreated["config_ref"]["create_time"] = json!("2026-10-23T00:02:00Z");
    for mode in ["gateway_rule", "gateway_self"] {
        assert_state(
            child_state(
                Some(&stored),
                Some(&recreated),
                Some(&recreated),
                mode,
                key,
                None,
            ),
            "replaced",
            Some("instance_replaced"),
        );
        assert_state(
            child_state(
                Some(&recreated),
                Some(&recreated),
                Some(&recreated),
                mode,
                key,
                None,
            ),
            "current",
            None,
        );
    }
}

#[test]
fn unchanged_child_rules_survive_unrelated_rule_and_gateway_ip_changes() {
    let stored = snapshot(&["192.168.1.0/24"]);
    let key = stored["rules"][0]["rule_key"].as_str();
    let mut accepted = snapshot(&["192.168.1.0/24", "192.168.2.0/24->10.20.2.0/24"]);
    accepted["config_fingerprint"] = json!("new-rule-list");
    accepted["ipv4"] = json!("10.90.0.2");
    assert_state(
        child_state(
            Some(&stored),
            Some(&accepted),
            Some(&accepted),
            "gateway_rule",
            key,
            None,
        ),
        "current",
        None,
    );
    assert_state(
        child_state(
            Some(&stored),
            Some(&accepted),
            Some(&accepted),
            "gateway_self",
            key,
            None,
        ),
        "changed",
        Some("instance_ipv4_changed"),
    );
    let removed = snapshot(&["192.168.2.0/24"]);
    assert_state(
        child_state(
            Some(&stored),
            Some(&removed),
            Some(&removed),
            "gateway_rule",
            key,
            None,
        ),
        "changed",
        Some("binding_rule_missing"),
    );
}

#[test]
fn preview_versions_bind_scope_subject_and_selection_but_ignore_display_only_changes() {
    let current = snapshot(&["192.168.1.0/24"]);
    let scope = json!({"owner_user_id":1,"machine_id":"node-a"});
    let subject = json!({"resource_kind":"device","id":"device-a"});
    let selection = json!({"address_spec":{"mode":"gateway_rule","rule_key":current["rules"][0]["rule_key"],"host_offset":20}});
    let original = preview_version(&scope, &subject, &selection, &current);
    let mut renamed = current.clone();
    renamed["display_name"] = json!("Display-only rename");
    renamed["source"] = json!("user");
    renamed["online"] = json!(false);
    assert_eq!(
        original,
        preview_version(&scope, &subject, &selection, &renamed)
    );
    assert_ne!(
        original,
        preview_version(
            &json!({"owner_user_id":2,"machine_id":"node-a"}),
            &subject,
            &selection,
            &current
        )
    );
    assert_ne!(
        original,
        preview_version(
            &json!({"owner_user_id":1,"machine_id":"node-b"}),
            &subject,
            &selection,
            &current
        )
    );
    assert_ne!(
        original,
        preview_version(
            &scope,
            &json!({"resource_kind":"device","id":"device-b"}),
            &selection,
            &current
        )
    );
    let mut changed = selection.clone();
    changed["address_spec"]["host_offset"] = json!(21);
    assert_ne!(
        original,
        preview_version(&scope, &subject, &changed, &current)
    );
    let mut changed = current.clone();
    changed["config_ref"]["id"] = json!(8);
    assert_ne!(
        original,
        preview_version(&scope, &subject, &selection, &changed)
    );
    changed = current.clone();
    changed["ipv4"] = json!("10.90.0.2");
    assert_ne!(
        original,
        preview_version(&scope, &subject, &selection, &changed)
    );
    let reordered = serde_json::from_str::<Value>(
        r#"{"address_spec":{"host_offset":20,"rule_key":"placeholder","mode":"gateway_rule"}}"#,
    )
    .unwrap();
    let mut reordered = reordered;
    reordered["address_spec"]["rule_key"] = current["rules"][0]["rule_key"].clone();
    assert_eq!(
        original,
        preview_version(&scope, &subject, &reordered, &current)
    );
}
