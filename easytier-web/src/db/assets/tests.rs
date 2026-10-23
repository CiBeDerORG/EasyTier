//! These tests use the real migration chain and SQLite connection settings.
//! They exercise the constraints shared by UI requests and concurrent callers.
use serde_json::{Value, json};
use uuid::Uuid;

use super::{AssetsStore, DeviceListQuery};
use crate::db::{Db, DeviceHeartbeatRecord};

struct Fixture {
    db: Db,
    store: AssetsStore,
    owner: i32,
    other: i32,
    node: Uuid,
    second_node: Uuid,
}

async fn register(db: &Db, owner: i32, node: Uuid) {
    db.upsert_device_heartbeat(DeviceHeartbeatRecord {
        user_id: owner,
        machine_id: node,
        hostname: "asset-test-node".into(),
        easytier_version: "test".into(),
        device_os: "{}".into(),
        client_url: "tcp://192.0.2.1:1001".into(),
    }).await.unwrap();
}

impl Fixture {
    async fn new() -> Self {
        let db = Db::memory_db().await;
        let owner = db.auto_create_user("asset-owner").await.unwrap().id;
        let other = db.auto_create_user("asset-other").await.unwrap().id;
        let node = Uuid::new_v4();
        let second_node = Uuid::new_v4();
        register(&db, owner, node).await;
        register(&db, other, node).await;
        register(&db, owner, second_node).await;
        let store = AssetsStore::new(db.clone());
        Self { db, store, owner, other, node, second_node }
    }

    async fn group(&self, name: &str) -> Value {
        self.store.create_group(self.owner, self.owner, &self.node.to_string(), json!({
            "request_id":Uuid::new_v4(), "name":name, "subnets":["192.168.1.0/24"]
        })).await.unwrap()
    }

    async fn device(&self, body: Value) -> Value {
        self.store.create_device(self.owner, self.owner, &self.node.to_string(), body).await.unwrap()
    }
}

fn device_body() -> Value {
    json!({"request_id":Uuid::new_v4(), "name":"NAS", "overlay_ip":"10.0.0.2"})
}

fn service_body(protocols: &[&str], port: u16) -> Value {
    json!({"request_id":Uuid::new_v4(), "name":"Service", "protocols":protocols, "port":port})
}

fn update_body(record: &Value, fields: &[&str]) -> Value {
    let mut body = serde_json::Map::new();
    for field in fields { body.insert((*field).into(), record[*field].clone()); }
    Value::Object(body)
}

fn device_update(record: &Value) -> Value {
    update_body(record, &["gateway_group_id", "device_type_id", "name", "model", "overlay_ip", "lan_ip", "is_gateway_self", "remarks"])
}

fn id(record: &Value) -> &str { record["id"].as_str().unwrap() }

#[tokio::test]
async fn migrated_foreign_keys_and_repository_queries_isolate_owner_and_node() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.group("LAN").await;
    let mut body = device_body();
    body["gateway_group_id"] = group["id"].clone();
    let device = f.device(body.clone()).await;

    assert_eq!(f.store.get_device(f.other, &node, id(&device)).await.unwrap_err().status, 404);
    assert_eq!(f.store.delete_device(f.owner, &f.second_node.to_string(), id(&device)).await.unwrap_err().status, 404);
    assert_eq!(f.store.create_device(f.other, f.other, &node, body.clone()).await.unwrap_err().status, 404);
    // Use a new operation here so this checks parent isolation rather than
    // the separate prohibition on reusing a request ID in a different node.
    body["request_id"] = json!(Uuid::new_v4());
    assert_eq!(f.store.create_device(f.owner, f.owner, &f.second_node.to_string(), body).await.unwrap_err().status, 404);
    assert!(f.store.list_groups(f.other, &node).await.unwrap()["items"].as_array().unwrap().is_empty());

    // The database itself must reject a cross-owner group even if a future
    // code path bypasses the repository's parent checks.
    let mut other_body = device_body();
    other_body["name"] = json!("Other account");
    let other_device = f.store.create_device(f.other, f.other, &node, other_body).await.unwrap();
    let error = sqlx::query("UPDATE asset_devices SET gateway_group_id = ? WHERE id = ?")
        .bind(id(&group)).bind(id(&other_device)).execute(&f.db.inner()).await.unwrap_err();
    assert!(error.as_database_error().unwrap().is_foreign_key_violation());

    let mut first_connection = f.db.inner().acquire().await.unwrap();
    let mut second_connection = f.db.inner().acquire().await.unwrap();
    for connection in [&mut first_connection, &mut second_connection] {
        let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&mut **connection).await.unwrap();
        assert_eq!(enabled, 1);
    }
    let violations = sqlx::query("PRAGMA foreign_key_check").fetch_all(&mut *first_connection).await.unwrap();
    assert!(violations.is_empty());
}

#[tokio::test]
async fn cidr_host_bits_are_rejected_and_mapped_addresses_share_one_representation() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let preview = f.store.preview_subnets(f.owner, &node, json!({"subnets":["192.168.1.7/24"]})).await.unwrap();
    assert!(preview["field_errors"]["subnets.0"].as_str().unwrap().contains("192.168.1.0/24"));
    let bad = f.store.create_group(f.owner, f.owner, &node, json!({
        "request_id":Uuid::new_v4(), "name":"Bad", "subnets":["192.168.1.7/24"]
    })).await.unwrap_err();
    assert_eq!(bad.status, 422);
    assert!(f.store.list_groups(f.owner, &node).await.unwrap()["items"].as_array().unwrap().is_empty());
    let duplicate = f.store.preview_subnets(f.owner, &node, json!({
        "subnets":["::ffff:192.168.1.0/120", "192.168.1.0/24"]
    })).await.unwrap();
    assert!(duplicate["field_errors"].get("subnets.1").is_some());

    let group = f.store.create_group(f.owner, f.owner, &node, json!({
        "request_id":Uuid::new_v4(), "name":"Dual stack", "subnets":[
            "::ffff:192.168.1.0/120", "192.168.0.0/16", "2001:0db8:0001::/64"
        ]
    })).await.unwrap();
    assert_eq!(group["subnets"], json!(["192.168.0.0/16", "192.168.1.0/24", "2001:db8:1::/64"]));
    let mut body = device_body();
    body["gateway_group_id"] = group["id"].clone();
    body["lan_ip"] = json!("::ffff:192.168.1.10");
    let device = f.device(body).await;
    assert_eq!(device["lan_ip"], "192.168.1.10");
    assert!(device["warnings"].as_array().unwrap().is_empty());

    let mut changed = device_update(&device);
    changed["lan_ip"] = json!("2001:db8:2::10");
    let outside = f.store.update_device(f.owner, &node, id(&device), changed).await.unwrap();
    assert_eq!(outside["warnings"][0]["code"], "lan_outside_gateway_subnets");
    let preview = f.store.preview_subnets(f.owner, &node, json!({
        "gateway_group_id":id(&group), "subnets":["2001:db8:2::/64"]
    })).await.unwrap();
    assert!(preview["warnings"].as_array().unwrap().is_empty());
    f.store.update_group(f.owner, &node, id(&group), json!({
        "name":"Dual stack", "subnets":["2001:db8:2::/64"]
    })).await.unwrap();
    assert!(f.store.get_device(f.owner, &node, id(&device)).await.unwrap()["warnings"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn lan_uniqueness_uses_canonical_ips_and_is_limited_to_one_owner_node() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let mut first = device_body();
    first["lan_ip"] = json!("::ffff:192.168.1.10");
    f.device(first).await;
    let mut duplicate = device_body();
    duplicate["lan_ip"] = json!("192.168.1.10");
    let error = f.store.create_device(f.owner, f.owner, &node, duplicate.clone()).await.unwrap_err();
    assert_eq!((error.status, error.code.as_str()), (409, "lan_ip_in_use"));
    f.store.create_device(f.owner, f.owner, &f.second_node.to_string(), duplicate.clone()).await.unwrap();
    f.store.create_device(f.other, f.other, &node, duplicate).await.unwrap();

    // Missing LAN addresses and repeated overlay addresses are intentionally allowed.
    f.device(device_body()).await;
    f.device(device_body()).await;
    let mut ipv6 = device_body();
    ipv6["lan_ip"] = json!("2001:0DB8:0001:0000:0000:0000:0000:0010");
    let saved = f.device(ipv6).await;
    assert_eq!(saved["lan_ip"], "2001:db8:1::10");
    let mut equivalent = device_body();
    equivalent["lan_ip"] = json!("2001:db8:1::10");
    assert_eq!(f.store.create_device(f.owner, f.owner, &node, equivalent).await.unwrap_err().status, 409);
}

#[tokio::test]
async fn gateway_self_and_fixed_placement_survive_edits() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.group("Gateway").await;
    let mut body = device_body();
    body["gateway_group_id"] = group["id"].clone();
    body["is_gateway_self"] = json!(true);
    let device = f.device(body.clone()).await;
    body["request_id"] = json!(Uuid::new_v4());
    assert_eq!(f.store.create_device(f.owner, f.owner, &node, body.clone()).await.unwrap_err().code, "gateway_self_in_use");

    let mut moved = device_update(&device);
    moved["gateway_group_id"] = Value::Null;
    moved["is_gateway_self"] = json!(false);
    assert_eq!(f.store.update_device(f.owner, &node, id(&device), moved).await.unwrap_err().code, "asset_ownership_fixed");
    let mut edit_a = device_update(&device);
    edit_a["name"] = json!("First save");
    f.store.update_device(f.owner, &node, id(&device), edit_a).await.unwrap();
    let mut edit_b = device_update(&device);
    edit_b["name"] = json!("Last save");
    edit_b["is_gateway_self"] = json!(false);
    let saved = f.store.update_device(f.owner, &node, id(&device), edit_b).await.unwrap();
    assert_eq!(saved["name"], "Last save");
    f.device(body).await;
}

#[tokio::test]
async fn protocol_port_conflicts_are_per_device_and_updates_preserve_previous_bindings() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let first = f.device(device_body()).await;
    let other = f.device(device_body()).await;
    let tcp = f.store.create_service(f.owner, f.owner, &node, id(&first), service_body(&["tcp"], 443)).await.unwrap();
    f.store.create_service(f.owner, f.owner, &node, id(&first), service_body(&["udp"], 443)).await.unwrap();
    f.store.create_service(f.owner, f.owner, &node, id(&other), service_body(&["tcp", "udp"], 443)).await.unwrap();
    let error = f.store.create_service(f.owner, f.owner, &node, id(&first), service_body(&["tcp", "udp"], 443)).await.unwrap_err();
    assert_eq!((error.status, error.code.as_str()), (409, "service_port_in_use"));

    let mut update = update_body(&tcp, &["name", "protocols", "port", "remarks", "access_url", "icon"]);
    update["protocols"] = json!(["tcp", "udp"]);
    assert_eq!(f.store.update_service(f.owner, &node, id(&tcp), update.clone()).await.unwrap_err().status, 409);
    let unchanged = f.store.list_services(f.owner, &node, id(&first)).await.unwrap();
    let row = unchanged["items"].as_array().unwrap().iter().find(|row| row["id"] == tcp["id"]).unwrap();
    assert_eq!(row["protocols"], json!(["tcp"]));
    assert_eq!(row["port"], 443);
    update["port"] = json!(8443);
    let changed = f.store.update_service(f.owner, &node, id(&tcp), update).await.unwrap();
    assert_eq!(changed["protocols"], json!(["tcp", "udp"]));
    f.store.create_service(f.owner, f.owner, &node, id(&first), service_body(&["tcp"], 443)).await.unwrap();
}

#[tokio::test]
async fn multi_protocol_failure_rolls_back_service_bindings_and_idempotency_record() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let device = f.device(device_body()).await;
    sqlx::query("CREATE TRIGGER reject_udp_binding BEFORE INSERT ON asset_service_bindings WHEN NEW.protocol = 'udp' BEGIN SELECT RAISE(ABORT, 'injected failure'); END")
        .execute(&f.db.inner()).await.unwrap();
    let body = service_body(&["tcp", "udp"], 53);
    assert!(f.store.create_service(f.owner, f.owner, &node, id(&device), body.clone()).await.is_err());
    let services: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_services").fetch_one(&f.db.inner()).await.unwrap();
    let bindings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_service_bindings").fetch_one(&f.db.inner()).await.unwrap();
    let operations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_operations WHERE operation = 'create_service'").fetch_one(&f.db.inner()).await.unwrap();
    assert_eq!((services, bindings, operations), (0, 0, 0));
    sqlx::query("DROP TRIGGER reject_udp_binding").execute(&f.db.inner()).await.unwrap();
    let service = f.store.create_service(f.owner, f.owner, &node, id(&device), body).await.unwrap();
    assert_eq!(service["protocols"], json!(["tcp", "udp"]));
}

#[tokio::test]
async fn deleting_device_cascades_current_services_but_nodes_and_used_parents_are_restricted() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.group("LAN").await;
    let kind = f.store.create_type(f.owner, f.owner, json!({"request_id":Uuid::new_v4(), "name":"Router"})).await.unwrap();
    let mut body = device_body();
    body["gateway_group_id"] = group["id"].clone();
    body["device_type_id"] = kind["id"].clone();
    let device = f.device(body).await;
    for port in [80, 443] {
        f.store.create_service(f.owner, f.owner, &node, id(&device), service_body(&["tcp", "udp"], port)).await.unwrap();
    }
    assert_eq!(f.store.delete_group(f.owner, &node, id(&group)).await.unwrap_err().status, 409);
    assert_eq!(f.store.delete_type(f.owner, id(&kind)).await.unwrap_err().status, 409);
    assert!(f.db.delete_device_with_optional_block((f.owner, f.node), true).await.is_err());
    assert!(f.db.get_device((f.owner, f.node)).await.unwrap().is_some());
    let blocked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocked_devices WHERE user_id = ? AND machine_id = ?")
        .bind(f.owner).bind(&node).fetch_one(&f.db.inner()).await.unwrap();
    assert_eq!(blocked, 0);

    // A service added after the delete dialog was shown must also be removed.
    f.store.create_service(f.owner, f.owner, &node, id(&device), service_body(&["tcp"], 8080)).await.unwrap();
    f.store.delete_device(f.owner, &node, id(&device)).await.unwrap();
    let services: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_services").fetch_one(&f.db.inner()).await.unwrap();
    let bindings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_service_bindings").fetch_one(&f.db.inner()).await.unwrap();
    assert_eq!((services, bindings), (0, 0));
    assert!(f.db.node_has_assets(f.owner, f.node).await.unwrap());
    f.store.delete_group(f.owner, &node, id(&group)).await.unwrap();
    f.store.delete_type(f.owner, id(&kind)).await.unwrap();
    sqlx::query("INSERT INTO node_asset_metadata(owner_user_id,machine_id,is_public_server) VALUES(?,?,1)")
        .bind(f.owner).bind(&node).execute(&f.db.inner()).await.unwrap();
    assert!(f.db.delete_device_with_optional_block((f.owner, f.node), false).await.unwrap());
    let metadata: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_asset_metadata").fetch_one(&f.db.inner()).await.unwrap();
    assert_eq!(metadata, 0);
    assert!(f.db.get_device((f.other, f.node)).await.unwrap().is_some());
}

#[tokio::test]
async fn simultaneous_retries_create_once_and_changed_scope_or_payload_cannot_reuse_key() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let mut body = device_body();
    body["name"] = json!(" NAS ");
    body["overlay_ip"] = json!("2001:0DB8:0000::2");
    let (first, retry) = tokio::join!(
        f.store.create_device(f.owner, f.owner, &node, body.clone()),
        f.store.create_device(f.owner, f.owner, &node, body.clone())
    );
    let first = first.unwrap();
    assert_eq!(first, retry.unwrap());
    let mut equivalent = body.clone();
    equivalent["name"] = json!("NAS");
    equivalent["overlay_ip"] = json!("2001:db8::2");
    assert_eq!(first, f.store.create_device(f.owner, f.owner, &node, equivalent).await.unwrap());
    assert_eq!(f.store.list_devices(f.owner, &node, DeviceListQuery::default()).await.unwrap()["total"], 1);
    let mut changed = body.clone();
    changed["name"] = json!("Phone");
    assert_eq!(f.store.create_device(f.owner, f.owner, &node, changed).await.unwrap_err().status, 409);
    assert_eq!(f.store.create_device(f.owner, f.owner, &f.second_node.to_string(), body.clone()).await.unwrap_err().status, 409);
    f.store.delete_device(f.owner, &node, id(&first)).await.unwrap();
    assert_eq!(f.store.create_device(f.owner, f.owner, &node, body).await.unwrap_err().status, 410);
}

#[tokio::test]
async fn creation_replay_survives_database_reopen_and_does_not_resurrect_deleted_resources() {
    let path = std::env::temp_dir().join(format!("easytier-assets-replay-{}.db", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let db = Db::new(&database_url).await.unwrap();
    let owner = db.auto_create_user("persistent-assets-owner").await.unwrap().id;
    let node_id = Uuid::new_v4();
    let node = node_id.to_string();
    register(&db, owner, node_id).await;
    let body = device_body();
    let store = AssetsStore::new(db.clone());
    let original = store.create_device(owner, owner, &node, body.clone()).await.unwrap();
    drop(store);
    db.inner().close().await;
    drop(db);

    let reopened = Db::new(&database_url).await.unwrap();
    let store = AssetsStore::new(reopened.clone());
    assert_eq!(original, store.create_device(owner, owner, &node, body.clone()).await.unwrap());
    store.delete_device(owner, &node, id(&original)).await.unwrap();
    reopened.delete_device_with_optional_block((owner, node_id), false).await.unwrap();
    assert_eq!(store.create_device(owner, owner, &node, body).await.unwrap_err().status, 410);
    drop(store);
    reopened.inner().close().await;
    drop(reopened);
    std::fs::remove_file(&path).unwrap();
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

#[tokio::test]
async fn type_initialization_is_explicit_owner_scoped_and_does_not_revive_deleted_seeds() {
    let f = Fixture::new().await;
    assert!(f.store.list_types(f.owner).await.unwrap()["items"].as_array().unwrap().is_empty());
    let initial = f.store.initialize_types(f.owner).await.unwrap();
    assert_eq!(initial["items"].as_array().unwrap().len(), 6);
    for item in initial["items"].as_array().unwrap() {
        f.store.delete_type(f.owner, id(item)).await.unwrap();
    }
    assert!(f.store.initialize_types(f.owner).await.unwrap()["items"].as_array().unwrap().is_empty());
    assert_eq!(f.store.initialize_types(f.other).await.unwrap()["items"].as_array().unwrap().len(), 6);
    let first = f.store.create_type(f.owner, f.owner, json!({"request_id":Uuid::new_v4(), "name":"Custom"})).await.unwrap();
    let second = f.store.create_type(f.owner, f.owner, json!({"request_id":Uuid::new_v4(), "name":"Custom"})).await.unwrap();
    assert_ne!(first["id"], second["id"]);
    assert_eq!(f.store.update_type(f.other, id(&first), json!({"name":"Wrong owner"})).await.unwrap_err().status, 404);
}

#[tokio::test]
async fn search_is_literal_case_insensitive_and_location_uses_the_filtered_page() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let mut body = device_body();
    body["name"] = json!("Étage 100% NAS");
    let first = f.device(body.clone()).await;
    body["request_id"] = json!(Uuid::new_v4());
    let second = f.device(body).await;
    f.device(device_body()).await;
    let page = f.store.list_devices(f.owner, &node, DeviceListQuery {
        keyword: Some("étage 100%".into()), page_size: Some(1), locate_id: Some(id(&first).into()),
        ..Default::default()
    }).await.unwrap();
    assert_eq!((page["total"].as_i64(), page["page"].as_i64()), (Some(2), Some(2)));
    assert_eq!(page["items"][0]["id"], first["id"]);
    let percent = f.store.list_devices(f.owner, &node, DeviceListQuery {
        keyword: Some("%".into()), ..Default::default()
    }).await.unwrap();
    assert_eq!(percent["total"], 2);
    assert_eq!(percent["items"][0]["id"], second["id"]);
    assert_eq!(f.store.list_devices(f.owner, &node, DeviceListQuery {
        keyword: Some("missing".into()), locate_id: Some(id(&first).into()), ..Default::default()
    }).await.unwrap_err().status, 404);
}

#[tokio::test]
async fn service_links_and_icons_cannot_introduce_active_content() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let device = f.device(device_body()).await;
    for link in ["javascript:alert(1)", "data:text/html,<script>alert(1)</script>", "//example.test/app", "https://user:secret@example.test/"] {
        let mut body = service_body(&["tcp"], 443);
        body["access_url"] = json!(link);
        assert_eq!(f.store.create_service(f.owner, f.owner, &node, id(&device), body).await.unwrap_err().status, 422);
    }
    let mut body = service_body(&["tcp"], 443);
    body["icon"] = json!("<svg onload=alert(1)>");
    assert_eq!(f.store.create_service(f.owner, f.owner, &node, id(&device), body).await.unwrap_err().status, 422);
    let mut safe = service_body(&["tcp"], 443);
    safe["access_url"] = json!("https://nas.example.test/app?view=files#home");
    safe["icon"] = json!("database");
    let service = f.store.create_service(f.owner, f.owner, &node, id(&device), safe).await.unwrap();
    assert_eq!(service["access_url"], "https://nas.example.test/app?view=files#home");
}
