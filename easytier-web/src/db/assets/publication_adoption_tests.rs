use super::*;
use sqlx::Connection;

const MACHINE: &str = "22222222-2222-4222-8222-222222222222";
const INSTANCE: &str = "33333333-3333-4333-8333-333333333333";

async fn fixture() -> (SqliteConnection, StoredConfig, PublicationRuntimeProof) {
    let mut connection = SqliteConnection::connect("sqlite::memory:").await.unwrap();
    for statement in [
        "CREATE TABLE users(id INTEGER PRIMARY KEY, deleted_at TEXT)",
        "CREATE TABLE user_running_network_configs(id INTEGER PRIMARY KEY, user_id INTEGER, device_id TEXT, network_instance_id TEXT, create_time TEXT, update_time TEXT, source TEXT, disabled INTEGER, network_config TEXT)",
        "CREATE TABLE networks(id TEXT, user_id INTEGER, network_name TEXT)",
        "CREATE TABLE network_members(user_id INTEGER, device_id TEXT, network_id TEXT)",
        "CREATE TABLE managed_config_revisions(user_id INTEGER, device_id TEXT)",
        "CREATE TABLE asset_service_publications(owner_user_id INTEGER, public_machine_id TEXT, service_id TEXT, rules_json TEXT)",
        "INSERT INTO users(id) VALUES(1)",
    ] {
        sqlx::query(statement).execute(&mut connection).await.unwrap();
    }
    let value = json!({
        "instance_id": INSTANCE, "network_name": "YUN", "hostname": "public-node",
        "network_secret": "test-only-secret", "listeners": ["tcp://0.0.0.0:11010"],
        "port_forwards": [{"proto":"tcp", "bind_ip":"0.0.0.0", "bind_port":2222,
                           "dst_ip":"10.10.10.3", "dst_port":22}]
    });
    sqlx::query("INSERT INTO user_running_network_configs VALUES(1,1,?,?,'generation-1','before','user',0,?)")
        .bind(MACHINE).bind(INSTANCE).bind(value.to_string())
        .execute(&mut connection).await.unwrap();
    sqlx::query("INSERT INTO managed_config_revisions VALUES(1,?)")
        .bind(MACHINE).execute(&mut connection).await.unwrap();
    let config = load_config(&mut connection, 1, MACHINE, INSTANCE).await.unwrap().unwrap();
    let proof = PublicationRuntimeProof { owner_user_id: 1, machine_id: MACHINE.into(), instance_id: INSTANCE.into() };
    (connection, config, proof)
}

#[tokio::test]
async fn publication_adopts_user_config_and_preserves_manual_mapping_on_unpublish() {
    let (mut connection, mut config, proof) = fixture().await;
    let before = config.value.clone();
    ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&proof)).await.unwrap();
    let stored = load_config(&mut connection, 1, MACHINE, INSTANCE).await.unwrap().unwrap();
    assert_eq!(stored.source, "web");
    assert_eq!(stored.value, before);
    assert_eq!(stored.created, config.created);
    assert!(!stored.disabled);
    // Once adopted, a retry uses the existing Web ownership rather than
    // requiring another adoption or duplicating configuration records.
    ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, None).await.unwrap();
    let owned = json!({"proto":"tcp", "bind_ip":"0.0.0.0", "bind_port":80,
                      "dst_ip":"10.10.10.3", "dst_port":80});
    let mut rules = forwards(&config.value).unwrap();
    rules.push(owned.clone());
    save_config(&mut connection, 1, MACHINE, INSTANCE, &config, rules).await.unwrap();
    let publication = Publication {
        machine_id: MACHINE.into(), instance_id: INSTANCE.into(),
        config_record_id: config.id, config_create_time: config.created.clone(),
        input: Value::Null, rules: vec![owned],
    };
    remove_owned(&mut connection, 1, &publication).await.unwrap();
    let stored = load_config(&mut connection, 1, MACHINE, INSTANCE).await.unwrap().unwrap();
    assert_eq!(stored.value, before);
    assert_eq!(stored.source, "web");
    let revisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM managed_config_revisions")
        .fetch_one(&mut connection).await.unwrap();
    assert_eq!(revisions, 0);
}

#[tokio::test]
async fn publication_adoption_requires_exact_scoped_runtime_proof() {
    let (mut connection, mut config, proof) = fixture().await;
    assert!(ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, None).await.is_err());
    for incorrect in [
        PublicationRuntimeProof { owner_user_id: 2, ..proof.clone() },
        PublicationRuntimeProof { machine_id: uuid::Uuid::new_v4().to_string(), ..proof.clone() },
        PublicationRuntimeProof { instance_id: uuid::Uuid::new_v4().to_string(), ..proof.clone() },
    ] {
        assert!(ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&incorrect)).await.is_err());
    }
    assert_eq!(config.source, "user");
    assert_eq!(load_config(&mut connection, 1, MACHINE, INSTANCE).await.unwrap().unwrap().source, "user");
}

#[tokio::test]
async fn publication_adoption_rejects_central_ownership() {
    let (mut connection, mut config, proof) = fixture().await;
    sqlx::query("INSERT INTO networks VALUES(?,1,'YUN')")
        .bind(INSTANCE).execute(&mut connection).await.unwrap();
    sqlx::query("INSERT INTO network_members VALUES(1,?,?)")
        .bind(MACHINE).bind(INSTANCE).execute(&mut connection).await.unwrap();
    assert!(ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&proof)).await.is_err());
    assert_eq!(load_config(&mut connection, 1, MACHINE, INSTANCE).await.unwrap().unwrap().source, "user");
}

#[tokio::test]
async fn publication_adoption_rejects_disabled_unknown_and_mismatched_configs() {
    let (mut connection, mut config, proof) = fixture().await;
    config.disabled = true;
    assert!(ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&proof)).await.is_err());
    config.disabled = false;
    config.source = "unknown".into();
    assert!(ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&proof)).await.is_err());
    config.source = "user".into();
    config.value["instance_id"] = json!(uuid::Uuid::new_v4().to_string());
    assert!(ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&proof)).await.is_err());
    assert_eq!(load_config(&mut connection, 1, MACHINE, INSTANCE).await.unwrap().unwrap().source, "user");
}

#[tokio::test]
async fn publication_adoption_rejects_inactive_owner_and_replaced_row() {
    let (mut connection, mut config, proof) = fixture().await;
    config.created = "replaced-generation".into();
    assert!(ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&proof)).await.is_err());
    config.created = "generation-1".into();
    sqlx::query("UPDATE users SET deleted_at = 'deleted' WHERE id = 1")
        .execute(&mut connection).await.unwrap();
    assert!(ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&proof)).await.is_err());
    assert_eq!(load_config(&mut connection, 1, MACHINE, INSTANCE).await.unwrap().unwrap().source, "user");
}

#[tokio::test]
async fn publication_port_conflict_rolls_back_adoption() {
    let (mut connection, mut config, proof) = fixture().await;
    let before = config.value.clone();
    sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
    ensure_web_config(&mut connection, 1, MACHINE, INSTANCE, &mut config, Some(&proof)).await.unwrap();
    let conflict = vec![before["port_forwards"][0].clone()];
    assert!(available_listeners(&mut connection, 1, MACHINE, "new-service", &conflict).await.is_err());
    sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
    let stored = load_config(&mut connection, 1, MACHINE, INSTANCE).await.unwrap().unwrap();
    assert_eq!(stored.source, "user");
    assert_eq!(stored.value, before);
    let revisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM managed_config_revisions")
        .fetch_one(&mut connection).await.unwrap();
    assert_eq!(revisions, 1);
}
