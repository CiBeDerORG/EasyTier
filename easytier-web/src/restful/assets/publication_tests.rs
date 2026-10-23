use super::*;

fn fixture(source: &str, runtime_source: ConfigSource) -> (Uuid, SavedInstance, NetworkMeta) {
    let id = Uuid::new_v4();
    let saved = SavedInstance {
        config: Some(NetworkConfig { instance_id: Some(id.to_string()), ..Default::default() }),
        source: source.to_owned(),
        disabled: false,
    };
    let meta = NetworkMeta { source: runtime_source as i32, ..Default::default() };
    (id, saved, meta)
}

#[test]
fn writable_user_and_web_publication_candidates_are_selectable() {
    for saved_source in ["user", "web"] {
        for runtime_source in [ConfigSource::User, ConfigSource::Web] {
            let (id, saved, meta) = fixture(saved_source, runtime_source);
            assert_eq!(unavailable_reason(id, Some(&saved), Some(&meta), false), None);
        }
    }
}

#[test]
fn publication_candidates_keep_central_ownership_guard() {
    let (id, saved, meta) = fixture("user", ConfigSource::User);
    assert_eq!(unavailable_reason(id, Some(&saved), Some(&meta), true).unwrap().0, "centrally_managed");
}

#[test]
fn publication_candidates_keep_read_only_guard() {
    for source in ["user", "web"] {
        let (id, saved, mut meta) = fixture(source, ConfigSource::User);
        // All permission bits include READ_ONLY without depending on its bit index.
        meta.config_permission = !0;
        assert_eq!(unavailable_reason(id, Some(&saved), Some(&meta), false).unwrap().0, "runtime_config_read_only");
    }
}

#[test]
fn publication_candidates_reject_unknown_sources() {
    let (id, mut saved, mut meta) = fixture("unknown", ConfigSource::User);
    assert_eq!(unavailable_reason(id, Some(&saved), Some(&meta), false).unwrap().0, "saved_config_not_web");
    saved.source = "user".into();
    meta.source = i32::MAX;
    assert_eq!(unavailable_reason(id, Some(&saved), Some(&meta), false).unwrap().0, "runtime_config_not_web");
}

#[test]
fn publication_candidates_require_saved_identity_and_runtime_metadata() {
    let (id, mut saved, meta) = fixture("user", ConfigSource::User);
    assert_eq!(unavailable_reason(id, None, Some(&meta), false).unwrap().0, "config_not_saved");
    assert_eq!(unavailable_reason(id, Some(&saved), None, false).unwrap().0, "runtime_metadata_missing");
    saved.disabled = true;
    assert_eq!(unavailable_reason(id, Some(&saved), Some(&meta), false).unwrap().0, "saved_config_stopped");
    saved.disabled = false;
    saved.config.as_mut().unwrap().instance_id = Some(Uuid::new_v4().to_string());
    assert_eq!(unavailable_reason(id, Some(&saved), Some(&meta), false).unwrap().0, "instance_id_mismatch");
    saved.config = None;
    assert_eq!(unavailable_reason(id, Some(&saved), Some(&meta), false).unwrap().0, "saved_config_invalid");
}
