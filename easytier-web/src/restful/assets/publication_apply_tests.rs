use super::*;

#[test]
fn persisted_retry_keeps_selected_target_even_when_saved_rules_unchanged() {
    let selected=(Uuid::new_v4(),Uuid::new_v4());
    let plan=ApplyPlan{before:BTreeMap::from([(selected,None)]),since:12,selected:Some(selected)};
    let restored=ApplyPlan::decode(&plan.encode()).unwrap();
    assert_eq!(restored.selected,Some(selected));
    assert!(restored.before.contains_key(&selected));
}

fn rule(proto: &str, bind: u32, destination: u32) -> PortForwardConfig {
    PortForwardConfig { proto: proto.into(), bind_ip: "0.0.0.0".into(), bind_port: bind,
        dst_ip: "10.10.10.3".into(), dst_port: destination }
}

#[test]
fn publication_apply_adds_tcp_udp_and_keeps_live_manual_rules() {
    let manual = rule("tcp", 7010, 7010);
    let desired = vec![rule("tcp", 80, 80), rule("udp", 80, 80)];
    let merged = merge_rules(&[], &desired, std::slice::from_ref(&manual)).unwrap();
    assert_eq!(merged.len(), 3);
    assert!(merged.contains(&manual));
    assert!(desired.iter().all(|rule| merged.contains(rule)));
}

#[test]
fn publication_apply_unpublish_removes_only_owned_mapping() {
    let manual = rule("tcp", 7010, 7010);
    let owned = rule("tcp", 80, 80);
    let before = vec![manual.clone(), owned.clone()];
    assert_eq!(merge_rules(&before, std::slice::from_ref(&manual), &before).unwrap(), vec![manual]);
}

#[test]
fn publication_apply_retry_and_changed_target_are_idempotent() {
    let old = rule("tcp", 80, 80);
    let new = rule("tcp", 80, 8080);
    let desired = vec![new.clone()];
    assert_eq!(merge_rules(std::slice::from_ref(&old), &desired, std::slice::from_ref(&old)).unwrap(), desired);
    assert_eq!(merge_rules(&[old], &desired, &desired).unwrap(), desired);
}

#[test]
fn publication_apply_does_not_overwrite_a_manual_listener_conflict() {
    assert_eq!(merge_rules(&[], &[rule("tcp", 80, 80)], &[rule("tcp", 80, 9999)]), Err("runtime_port_conflict"));
}

#[test]
fn publication_apply_readback_compares_normalized_multisets() {
    let mut equivalent = rule("TCP", 80, 80);
    equivalent.dst_ip = "[::ffff:10.10.10.3]".into();
    let original = rule("tcp", 80, 80);
    assert!(same_rules(std::slice::from_ref(&original), std::slice::from_ref(&equivalent)));
    assert!(!same_rules(&[original.clone(), original], &[equivalent]));
}
