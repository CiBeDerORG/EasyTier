//! These tests use the real migration chain and SQLite connection settings.
//! They exercise the constraints shared by UI requests and concurrent callers.
use easytier::common::config::{ConfigSource, NetworkConfig};
use easytier_core::management::remote_client::Storage;
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

const DIRECT_INSTANCE: &str = "10000000-0000-0000-0000-000000000001";

async fn seed_config(db: &Db, owner: i32, node: Uuid, instance: Uuid, ipv4: &str, rules: &[&str]) {
    db.insert_or_update_user_network_config(
        (owner, node),
        instance,
        NetworkConfig {
            instance_id: Some(instance.into()),
            virtual_ipv4: Some(ipv4.into()),
            dhcp: Some(false),
            proxy_cidrs: rules.iter().map(|rule| (*rule).into()).collect(),
            ..Default::default()
        },
        ConfigSource::Web,
    )
    .await
    .unwrap();
}

async fn prepared_body(store: &AssetsStore, owner: i32, node: &str, mut body: Value) -> Value {
    let preview = store
        .preview_instance_address(
            owner,
            node,
            json!({
                "subject":{"resource_kind":"device","id":null}, "address_spec":body["address_spec"]
            }),
        )
        .await
        .unwrap();
    assert!(preview["preview_version"].is_string(), "{preview}");
    body["preview_version"] = preview["preview_version"].clone();
    body
}

async fn register(db: &Db, owner: i32, node: Uuid) {
    db.upsert_device_heartbeat(DeviceHeartbeatRecord {
        user_id: owner,
        machine_id: node,
        hostname: "asset-test-node".into(),
        easytier_version: "test".into(),
        device_os: "{}".into(),
        client_url: "tcp://192.0.2.1:1001".into(),
    })
    .await
    .unwrap();
}

impl Fixture {
    async fn new() -> Self {
        Self::with_db(Db::memory_db().await).await
    }

    async fn with_db(db: Db) -> Self {
        let owner = db.auto_create_user("asset-owner").await.unwrap().id;
        let other = db.auto_create_user("asset-other").await.unwrap().id;
        let node = Uuid::new_v4();
        let second_node = Uuid::new_v4();
        register(&db, owner, node).await;
        register(&db, other, node).await;
        register(&db, owner, second_node).await;
        for (account, machine) in [(owner, node), (other, node), (owner, second_node)] {
            seed_config(
                &db,
                account,
                machine,
                DIRECT_INSTANCE.parse().unwrap(),
                "10.0.0.2",
                &[],
            )
            .await;
        }
        let store = AssetsStore::new(db.clone());
        Self {
            db,
            store,
            owner,
            other,
            node,
            second_node,
        }
    }

    async fn group(&self, name: &str) -> Value {
        let instance = Uuid::new_v4();
        seed_config(
            &self.db,
            self.owner,
            self.node,
            instance,
            "10.0.0.1",
            &["192.168.1.0/24"],
        )
        .await;
        let preview = self.store.preview_binding(self.owner, &self.node.to_string(), json!({
            "subject":{"resource_kind":"gateway","id":null}, "operation":"attach", "instance_id":instance
        })).await.unwrap();
        self.store
            .create_group(
                self.owner,
                self.owner,
                &self.node.to_string(),
                json!({
                    "request_id":Uuid::new_v4(), "name":name, "binding":{"instance_id":instance},
                    "preview_version":preview["preview_version"]
                }),
            )
            .await
            .unwrap()
    }

    async fn device(&self, body: Value) -> Value {
        let body = prepared_body(&self.store, self.owner, &self.node.to_string(), body).await;
        self.store
            .create_device(self.owner, self.owner, &self.node.to_string(), body)
            .await
            .unwrap()
    }

    // Pre-upgrade rows are deliberately seeded through SQL. The v2 POST must not
    // accept the old manual address format just to keep legacy fixtures working.
    async fn legacy_group(&self, name: &str, subnets: &[&str]) -> Value {
        let group = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO asset_gateway_groups(id,owner_user_id,machine_id,name,created_at,updated_at) VALUES(?,?,?,?,?,?)")
            .bind(&group).bind(self.owner).bind(self.node.to_string()).bind(name)
            .bind(super::common::now()).bind(super::common::now()).execute(&self.db.inner()).await.unwrap();
        self.store
            .update_group(
                self.owner,
                &self.node.to_string(),
                &group,
                json!({"name":name,"subnets":subnets}),
            )
            .await
            .unwrap()
    }

    async fn legacy_device(&self, group: Option<&str>, overlay: &str, lan: Option<&str>) -> Value {
        let device = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO asset_devices(id,owner_user_id,machine_id,gateway_group_id,name,overlay_ip,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&device).bind(self.owner).bind(self.node.to_string()).bind(group).bind("Legacy")
            .bind(overlay).bind(super::common::now()).bind(super::common::now()).execute(&self.db.inner()).await.unwrap();
        self.store
            .update_device(
                self.owner,
                &self.node.to_string(),
                &device,
                json!({"lan_ip":lan}),
            )
            .await
            .unwrap()
    }
}

fn device_body() -> Value {
    json!({"request_id":Uuid::new_v4(), "name":"NAS", "address_spec":{"mode":"direct_instance","instance_id":DIRECT_INSTANCE,"lan_ip":null}})
}

fn child_body(group: &Value, is_self: bool) -> Value {
    let mut body = device_body();
    body["address_spec"] = if is_self {
        json!({"mode":"gateway_self","gateway_group_id":group["id"],"rule_key":null,"host_offset":null})
    } else {
        json!({"mode":"gateway_rule","gateway_group_id":group["id"],"rule_key":group["binding"]["snapshot"]["rules"][0]["rule_key"],"host_offset":10})
    };
    body
}

fn service_body(protocols: &[&str], port: u16) -> Value {
    json!({"request_id":Uuid::new_v4(), "name":"Service", "protocols":protocols, "port":port})
}

fn update_body(record: &Value, fields: &[&str]) -> Value {
    let mut body = serde_json::Map::new();
    for field in fields {
        body.insert((*field).into(), record[*field].clone());
    }
    Value::Object(body)
}

fn device_update(record: &Value) -> Value {
    update_body(
        record,
        &[
            "gateway_group_id",
            "device_type_id",
            "name",
            "model",
            "is_gateway_self",
            "remarks",
        ],
    )
}

fn id(record: &Value) -> &str {
    record["id"].as_str().unwrap()
}

#[tokio::test]
async fn migrated_foreign_keys_and_repository_queries_isolate_owner_and_node() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.group("LAN").await;
    let body = child_body(&group, false);
    let mut body = prepared_body(&f.store, f.owner, &node, body).await;
    let device = f
        .store
        .create_device(f.owner, f.owner, &node, body.clone())
        .await
        .unwrap();

    assert_eq!(
        f.store
            .get_device(f.other, &node, id(&device))
            .await
            .unwrap_err()
            .status,
        404
    );
    assert_eq!(
        f.store
            .delete_device(f.owner, &f.second_node.to_string(), id(&device))
            .await
            .unwrap_err()
            .status,
        404
    );
    assert_eq!(
        f.store
            .create_device(f.other, f.other, &node, body.clone())
            .await
            .unwrap_err()
            .status,
        404
    );
    // Use a new operation here so this checks parent isolation rather than
    // the separate prohibition on reusing a request ID in a different node.
    body["request_id"] = json!(Uuid::new_v4());
    assert_eq!(
        f.store
            .create_device(f.owner, f.owner, &f.second_node.to_string(), body)
            .await
            .unwrap_err()
            .status,
        404
    );
    assert!(
        f.store.list_groups(f.other, &node).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // The database itself must reject a cross-owner group even if a future
    // code path bypasses the repository's parent checks.
    let mut other_body = device_body();
    other_body["name"] = json!("Other account");
    let other_body = prepared_body(&f.store, f.other, &node, other_body).await;
    let other_device = f
        .store
        .create_device(f.other, f.other, &node, other_body)
        .await
        .unwrap();
    let error = sqlx::query("UPDATE asset_devices SET gateway_group_id = ? WHERE id = ?")
        .bind(id(&group))
        .bind(id(&other_device))
        .execute(&f.db.inner())
        .await
        .unwrap_err();
    assert!(
        error
            .as_database_error()
            .unwrap()
            .is_foreign_key_violation()
    );

    let mut first_connection = f.db.inner().acquire().await.unwrap();
    let mut second_connection = f.db.inner().acquire().await.unwrap();
    for connection in [&mut first_connection, &mut second_connection] {
        let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&mut **connection)
            .await
            .unwrap();
        assert_eq!(enabled, 1);
    }
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut *first_connection)
        .await
        .unwrap();
    assert!(violations.is_empty());
}

#[tokio::test]
async fn cidr_host_bits_are_rejected_and_mapped_addresses_share_one_representation() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let preview = f
        .store
        .preview_subnets(f.owner, &node, json!({"subnets":["192.168.1.7/24"]}))
        .await
        .unwrap();
    assert!(
        preview["field_errors"]["subnets.0"]
            .as_str()
            .unwrap()
            .contains("192.168.1.0/24")
    );
    let bad = f
        .store
        .create_group(
            f.owner,
            f.owner,
            &node,
            json!({
                "request_id":Uuid::new_v4(), "name":"Bad", "subnets":["192.168.1.7/24"]
            }),
        )
        .await
        .unwrap_err();
    assert_eq!(bad.status, 422);
    assert!(
        f.store.list_groups(f.owner, &node).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let duplicate = f
        .store
        .preview_subnets(
            f.owner,
            &node,
            json!({
                "subnets":["::ffff:192.168.1.0/120", "192.168.1.0/24"]
            }),
        )
        .await
        .unwrap();
    assert!(duplicate["field_errors"].get("subnets.1").is_some());

    let group = f
        .legacy_group(
            "Dual stack",
            &[
                "::ffff:192.168.1.0/120",
                "192.168.0.0/16",
                "2001:0db8:0001::/64",
            ],
        )
        .await;
    assert_eq!(
        group["subnets"],
        json!(["192.168.0.0/16", "192.168.1.0/24", "2001:db8:1::/64"])
    );
    let device = f
        .legacy_device(Some(id(&group)), "2001:db8::2", Some("::ffff:192.168.1.10"))
        .await;
    assert_eq!(device["lan_ip"], "192.168.1.10");
    assert!(device["warnings"].as_array().unwrap().is_empty());

    let mut changed = device_update(&device);
    changed["lan_ip"] = json!("2001:db8:2::10");
    let outside = f
        .store
        .update_device(f.owner, &node, id(&device), changed)
        .await
        .unwrap();
    assert_eq!(
        outside["warnings"][0]["code"],
        "lan_outside_gateway_subnets"
    );
    let preview = f
        .store
        .preview_subnets(
            f.owner,
            &node,
            json!({
                "gateway_group_id":id(&group), "subnets":["2001:db8:2::/64"]
            }),
        )
        .await
        .unwrap();
    assert!(preview["warnings"].as_array().unwrap().is_empty());
    f.store
        .update_group(
            f.owner,
            &node,
            id(&group),
            json!({
                "name":"Dual stack", "subnets":["2001:db8:2::/64"]
            }),
        )
        .await
        .unwrap();
    assert!(
        f.store
            .get_device(f.owner, &node, id(&device))
            .await
            .unwrap()["warnings"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn lan_uniqueness_uses_canonical_ips_and_is_limited_to_one_owner_node() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let mut first = device_body();
    first["address_spec"]["lan_ip"] = json!("192.168.1.10");
    f.device(first.clone()).await;
    let mut duplicate = first;
    duplicate["request_id"] = json!(Uuid::new_v4());
    duplicate["preview_version"] = json!("any-version");
    // Preview itself checks duplicate addresses; neither preview nor a stale POST may create one.
    let preview = f.store.preview_instance_address(f.owner, &node, json!({"subject":{"resource_kind":"device","id":null},"address_spec":duplicate["address_spec"]})).await.unwrap_err();
    assert_eq!(
        (preview.status, preview.code.as_str()),
        (409, "lan_ip_in_use")
    );
    assert_eq!(
        f.store
            .create_device(f.owner, f.owner, &node, duplicate.clone())
            .await
            .unwrap_err()
            .code,
        "lan_ip_in_use"
    );
    for (owner, node) in [
        (f.owner, f.second_node.to_string()),
        (f.other, node.clone()),
    ] {
        let mut scoped = duplicate.clone();
        scoped["request_id"] = json!(Uuid::new_v4());
        let body = prepared_body(&f.store, owner, &node, scoped).await;
        f.store
            .create_device(owner, owner, &node, body)
            .await
            .unwrap();
    }
    // Missing LAN addresses and repeated instance/overlay addresses are allowed.
    f.device(device_body()).await;
    f.device(device_body()).await;
    let saved = f
        .legacy_device(
            None,
            "2001:db8::2",
            Some("2001:0DB8:0001:0000:0000:0000:0000:0010"),
        )
        .await;
    assert_eq!(saved["lan_ip"], "2001:db8:1::10");
    let other = f.legacy_device(None, "2001:db8::3", None).await;
    assert_eq!(
        f.store
            .update_device(
                f.owner,
                &node,
                id(&other),
                json!({"lan_ip":"2001:db8:1::10"})
            )
            .await
            .unwrap_err()
            .status,
        409
    );
}

#[tokio::test]
async fn gateway_self_and_fixed_placement_survive_edits() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.group("Gateway").await;
    let body = child_body(&group, true);
    let mut body = prepared_body(&f.store, f.owner, &node, body).await;
    let device = f
        .store
        .create_device(f.owner, f.owner, &node, body.clone())
        .await
        .unwrap();
    body["request_id"] = json!(Uuid::new_v4());
    assert_eq!(
        f.store
            .create_device(f.owner, f.owner, &node, body.clone())
            .await
            .unwrap_err()
            .code,
        "gateway_self_in_use"
    );

    let mut moved = device_update(&device);
    moved["gateway_group_id"] = Value::Null;
    moved["is_gateway_self"] = json!(false);
    assert_eq!(
        f.store
            .update_device(f.owner, &node, id(&device), moved)
            .await
            .unwrap_err()
            .code,
        "asset_ownership_fixed"
    );
    let mut edit_a = device_update(&device);
    edit_a["name"] = json!("First save");
    f.store
        .update_device(f.owner, &node, id(&device), edit_a)
        .await
        .unwrap();
    let mut edit_b = device_update(&device);
    edit_b["name"] = json!("Last save");
    let saved = f
        .store
        .update_device(f.owner, &node, id(&device), edit_b)
        .await
        .unwrap();
    assert_eq!(saved["name"], "Last save");
    assert_eq!(
        f.store
            .update_device(
                f.owner,
                &node,
                id(&device),
                json!({"is_gateway_self":false})
            )
            .await
            .unwrap_err()
            .code,
        "asset_ownership_fixed"
    );
    f.store
        .delete_device(f.owner, &node, id(&device))
        .await
        .unwrap();
    f.device(body).await;
}

#[tokio::test]
async fn protocol_port_conflicts_are_per_device_and_updates_preserve_previous_bindings() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let first = f.device(device_body()).await;
    let other = f.device(device_body()).await;
    let tcp = f
        .store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&first),
            service_body(&["tcp"], 443),
        )
        .await
        .unwrap();
    f.store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&first),
            service_body(&["udp"], 443),
        )
        .await
        .unwrap();
    f.store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&other),
            service_body(&["tcp", "udp"], 443),
        )
        .await
        .unwrap();
    let error = f
        .store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&first),
            service_body(&["tcp", "udp"], 443),
        )
        .await
        .unwrap_err();
    assert_eq!(
        (error.status, error.code.as_str()),
        (409, "service_port_in_use")
    );

    let mut update = update_body(
        &tcp,
        &["name", "protocols", "port", "remarks", "access_url", "icon"],
    );
    update["protocols"] = json!(["tcp", "udp"]);
    assert_eq!(
        f.store
            .update_service(f.owner, &node, id(&tcp), update.clone())
            .await
            .unwrap_err()
            .status,
        409
    );
    let unchanged = f
        .store
        .list_services(f.owner, &node, id(&first))
        .await
        .unwrap();
    let row = unchanged["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == tcp["id"])
        .unwrap();
    assert_eq!(row["protocols"], json!(["tcp"]));
    assert_eq!(row["port"], 443);
    update["port"] = json!(8443);
    let changed = f
        .store
        .update_service(f.owner, &node, id(&tcp), update)
        .await
        .unwrap();
    assert_eq!(changed["protocols"], json!(["tcp", "udp"]));
    f.store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&first),
            service_body(&["tcp"], 443),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn multi_protocol_failure_rolls_back_service_bindings_and_idempotency_record() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let device = f.device(device_body()).await;
    sqlx::query("CREATE TRIGGER reject_udp_binding BEFORE INSERT ON asset_service_bindings WHEN NEW.protocol = 'udp' BEGIN SELECT RAISE(ABORT, 'injected failure'); END")
        .execute(&f.db.inner()).await.unwrap();
    let body = service_body(&["tcp", "udp"], 53);
    assert!(
        f.store
            .create_service(f.owner, f.owner, &node, id(&device), body.clone())
            .await
            .is_err()
    );
    let services: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_services")
        .fetch_one(&f.db.inner())
        .await
        .unwrap();
    let bindings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_service_bindings")
        .fetch_one(&f.db.inner())
        .await
        .unwrap();
    let operations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM asset_operations WHERE operation = 'create_service'",
    )
    .fetch_one(&f.db.inner())
    .await
    .unwrap();
    assert_eq!((services, bindings, operations), (0, 0, 0));
    sqlx::query("DROP TRIGGER reject_udp_binding")
        .execute(&f.db.inner())
        .await
        .unwrap();
    let service = f
        .store
        .create_service(f.owner, f.owner, &node, id(&device), body)
        .await
        .unwrap();
    assert_eq!(service["protocols"], json!(["tcp", "udp"]));
}

#[tokio::test]
async fn deleting_device_cascades_current_services_but_nodes_and_used_parents_are_restricted() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.group("LAN").await;
    let kind = f
        .store
        .create_type(
            f.owner,
            f.owner,
            json!({"request_id":Uuid::new_v4(), "name":"Router"}),
        )
        .await
        .unwrap();
    let mut body = child_body(&group, false);
    body["device_type_id"] = kind["id"].clone();
    let device = f.device(body).await;
    for port in [80, 443] {
        f.store
            .create_service(
                f.owner,
                f.owner,
                &node,
                id(&device),
                service_body(&["tcp", "udp"], port),
            )
            .await
            .unwrap();
    }
    assert_eq!(
        f.store
            .delete_group(f.owner, &node, id(&group))
            .await
            .unwrap_err()
            .status,
        409
    );
    assert_eq!(
        f.store
            .delete_type(f.owner, id(&kind))
            .await
            .unwrap_err()
            .status,
        409
    );
    assert!(
        f.db.delete_device_with_optional_block((f.owner, f.node), true)
            .await
            .is_err()
    );
    assert!(f.db.get_device((f.owner, f.node)).await.unwrap().is_some());
    let blocked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM blocked_devices WHERE user_id = ? AND machine_id = ?",
    )
    .bind(f.owner)
    .bind(&node)
    .fetch_one(&f.db.inner())
    .await
    .unwrap();
    assert_eq!(blocked, 0);

    // A service added after the delete dialog was shown must also be removed.
    f.store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&device),
            service_body(&["tcp"], 8080),
        )
        .await
        .unwrap();
    f.store
        .delete_device(f.owner, &node, id(&device))
        .await
        .unwrap();
    let services: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_services")
        .fetch_one(&f.db.inner())
        .await
        .unwrap();
    let bindings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_service_bindings")
        .fetch_one(&f.db.inner())
        .await
        .unwrap();
    assert_eq!((services, bindings), (0, 0));
    assert!(f.db.node_has_assets(f.owner, f.node).await.unwrap());
    f.store
        .delete_group(f.owner, &node, id(&group))
        .await
        .unwrap();
    f.store.delete_type(f.owner, id(&kind)).await.unwrap();
    sqlx::query(
        "INSERT INTO node_asset_metadata(owner_user_id,machine_id,is_public_server) VALUES(?,?,1)",
    )
    .bind(f.owner)
    .bind(&node)
    .execute(&f.db.inner())
    .await
    .unwrap();
    assert!(
        f.db.delete_device_with_optional_block((f.owner, f.node), false)
            .await
            .unwrap()
    );
    let metadata: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_asset_metadata")
        .fetch_one(&f.db.inner())
        .await
        .unwrap();
    assert_eq!(metadata, 0);
    assert!(f.db.get_device((f.other, f.node)).await.unwrap().is_some());
}

#[tokio::test]
async fn simultaneous_retries_create_once_and_changed_scope_or_payload_cannot_reuse_key() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let mut body = device_body();
    body["name"] = json!(" NAS ");
    body = prepared_body(&f.store, f.owner, &node, body).await;
    let (first, retry) = tokio::join!(
        f.store.create_device(f.owner, f.owner, &node, body.clone()),
        f.store.create_device(f.owner, f.owner, &node, body.clone())
    );
    let first = first.unwrap();
    assert_eq!(first, retry.unwrap());
    let mut equivalent = body.clone();
    equivalent["name"] = json!("NAS");

    assert_eq!(
        first,
        f.store
            .create_device(f.owner, f.owner, &node, equivalent)
            .await
            .unwrap()
    );
    assert_eq!(
        f.store
            .list_devices(f.owner, &node, DeviceListQuery::default())
            .await
            .unwrap()["total"],
        1
    );
    let mut changed = body.clone();
    changed["name"] = json!("Phone");
    assert_eq!(
        f.store
            .create_device(f.owner, f.owner, &node, changed)
            .await
            .unwrap_err()
            .status,
        409
    );
    assert_eq!(
        f.store
            .create_device(f.owner, f.owner, &f.second_node.to_string(), body.clone())
            .await
            .unwrap_err()
            .status,
        409
    );
    f.store
        .delete_device(f.owner, &node, id(&first))
        .await
        .unwrap();
    assert_eq!(
        f.store
            .create_device(f.owner, f.owner, &node, body)
            .await
            .unwrap_err()
            .status,
        410
    );
}

#[tokio::test]
async fn creation_replay_survives_database_reopen_and_does_not_resurrect_deleted_resources() {
    let path = std::env::temp_dir().join(format!("easytier-assets-replay-{}.db", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let db = Db::new(&database_url).await.unwrap();
    let owner = db
        .auto_create_user("persistent-assets-owner")
        .await
        .unwrap()
        .id;
    let node_id = Uuid::new_v4();
    let node = node_id.to_string();
    register(&db, owner, node_id).await;
    seed_config(
        &db,
        owner,
        node_id,
        DIRECT_INSTANCE.parse().unwrap(),
        "10.0.0.2",
        &[],
    )
    .await;
    let store = AssetsStore::new(db.clone());
    let body = prepared_body(&store, owner, &node, device_body()).await;
    let original = store
        .create_device(owner, owner, &node, body.clone())
        .await
        .unwrap();
    drop(store);
    db.inner().close().await;
    drop(db);

    let reopened = Db::new(&database_url).await.unwrap();
    let store = AssetsStore::new(reopened.clone());
    assert_eq!(
        original,
        store
            .create_device(owner, owner, &node, body.clone())
            .await
            .unwrap()
    );
    store
        .delete_device(owner, &node, id(&original))
        .await
        .unwrap();
    reopened
        .delete_device_with_optional_block((owner, node_id), false)
        .await
        .unwrap();
    assert_eq!(
        store
            .create_device(owner, owner, &node, body)
            .await
            .unwrap_err()
            .status,
        410
    );
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
    assert!(
        f.store.list_types(f.owner).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let initial = f.store.initialize_types(f.owner).await.unwrap();
    assert_eq!(initial["items"].as_array().unwrap().len(), 6);
    for item in initial["items"].as_array().unwrap() {
        f.store.delete_type(f.owner, id(item)).await.unwrap();
    }
    assert!(
        f.store.initialize_types(f.owner).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.store.initialize_types(f.other).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    let first = f
        .store
        .create_type(
            f.owner,
            f.owner,
            json!({"request_id":Uuid::new_v4(), "name":"Custom"}),
        )
        .await
        .unwrap();
    let second = f
        .store
        .create_type(
            f.owner,
            f.owner,
            json!({"request_id":Uuid::new_v4(), "name":"Custom"}),
        )
        .await
        .unwrap();
    assert_ne!(first["id"], second["id"]);
    assert_eq!(
        f.store
            .update_type(f.other, id(&first), json!({"name":"Wrong owner"}))
            .await
            .unwrap_err()
            .status,
        404
    );
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
    let page = f
        .store
        .list_devices(
            f.owner,
            &node,
            DeviceListQuery {
                keyword: Some("étage 100%".into()),
                page_size: Some(1),
                locate_id: Some(id(&first).into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (page["total"].as_i64(), page["page"].as_i64()),
        (Some(2), Some(2))
    );
    assert_eq!(page["items"][0]["id"], first["id"]);
    let percent = f
        .store
        .list_devices(
            f.owner,
            &node,
            DeviceListQuery {
                keyword: Some("%".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(percent["total"], 2);
    assert_eq!(percent["items"][0]["id"], second["id"]);
    assert_eq!(
        f.store
            .list_devices(
                f.owner,
                &node,
                DeviceListQuery {
                    keyword: Some("missing".into()),
                    locate_id: Some(id(&first).into()),
                    ..Default::default()
                }
            )
            .await
            .unwrap_err()
            .status,
        404
    );
}

#[tokio::test]
async fn service_links_and_icons_cannot_introduce_active_content() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let device = f.device(device_body()).await;
    for link in [
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "//example.test/app",
        "https://user:secret@example.test/",
    ] {
        let mut body = service_body(&["tcp"], 443);
        body["access_url"] = json!(link);
        assert_eq!(
            f.store
                .create_service(f.owner, f.owner, &node, id(&device), body)
                .await
                .unwrap_err()
                .status,
            422
        );
    }
    let mut body = service_body(&["tcp"], 443);
    body["icon"] = json!("<svg onload=alert(1)>");
    assert_eq!(
        f.store
            .create_service(f.owner, f.owner, &node, id(&device), body)
            .await
            .unwrap_err()
            .status,
        422
    );
    let mut safe = service_body(&["tcp"], 443);
    safe["access_url"] = json!("https://nas.example.test/app?view=files#home");
    safe["icon"] = json!("database");
    let service = f
        .store
        .create_service(f.owner, f.owner, &node, id(&device), safe)
        .await
        .unwrap();
    assert_eq!(
        service["access_url"],
        "https://nas.example.test/app?view=files#home"
    );
}

async fn refresh_body(f: &Fixture, kind: &str, resource: &Value, operation: &str) -> Value {
    let instance = resource["binding"]["instance_id"].clone();
    let preview = f.store.preview_binding(f.owner, &f.node.to_string(), json!({
        "subject":{"resource_kind":kind,"id":id(resource)},"operation":operation,"instance_id":instance
    })).await.unwrap();
    assert_eq!(preview["can_save"], true, "{preview}");
    json!({"request_id":Uuid::new_v4(),"operation":operation,"instance_id":instance,"preview_version":preview["preview_version"]})
}

#[tokio::test]
async fn catalog_uses_persistent_scope_core_dhcp_defaults_and_effective_address_policy() {
    use super::{RuntimeObservation, RuntimeObservations};
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let dynamic = Uuid::new_v4();
    let config = NetworkConfig {
        instance_id: Some(dynamic.into()),
        dhcp: None,
        dhcp_ipv4_cidr: Some("10.42.0.0/24".into()),
        virtual_ipv4: Some("10.99.0.10".into()),
        network_secret: Some("must-not-leak".into()),
        ..Default::default()
    };
    f.db.insert_or_update_user_network_config(
        (f.owner, f.node),
        dynamic,
        config.clone(),
        ConfigSource::Web,
    )
    .await
    .unwrap();
    let candidates = f
        .store
        .list_instance_candidates(f.owner, &node, "ordinary", None, None)
        .await
        .unwrap();
    let candidate = candidates["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["instance_id"] == dynamic.to_string())
        .unwrap();
    assert_eq!(candidate["selectable"], false);
    assert!(candidate["ipv4"].is_null()); // Core ignores leftover static IPv4 under implicit DHCP.
    assert!(!candidates.to_string().contains("must-not-leak"));
    let mut runtime = RuntimeObservations::new();
    runtime.insert(
        dynamic.to_string(),
        RuntimeObservation {
            ipv4: Some("10.42.0.12/24".into()),
            online: Some(false),
        },
    );
    runtime.insert(
        Uuid::new_v4().to_string(),
        RuntimeObservation {
            ipv4: Some("10.42.0.13".into()),
            online: Some(true),
        },
    );
    let store = f.store.clone().with_runtime_observations(runtime);
    let candidates = store
        .list_instance_candidates(f.owner, &node, "ordinary", None, None)
        .await
        .unwrap();
    assert_eq!(candidates["items"].as_array().unwrap().len(), 2); // runtime-only is not a persistent candidate
    let old = candidates["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["instance_id"] == dynamic.to_string())
        .unwrap()
        .clone();
    assert_eq!(
        (old["ipv4"].as_str(), old["ipv4_source"].as_str()),
        (Some("10.42.0.12"), Some("runtime"))
    );
    assert_eq!(
        (old["online"].as_bool(), old["selectable"].as_bool()),
        (Some(false), Some(true))
    );
    sqlx::query("UPDATE user_running_network_configs SET disabled=1 WHERE user_id=? AND device_id=? AND network_instance_id=?")
        .bind(f.owner).bind(&node).bind(dynamic.to_string()).execute(&f.db.inner()).await.unwrap();
    let before = store
        .list_instance_candidates(f.owner, &node, "ordinary", None, None)
        .await
        .unwrap();
    let disabled = before["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["instance_id"] == dynamic.to_string())
        .unwrap();
    assert_eq!(
        (
            disabled["disabled"].as_bool(),
            disabled["selectable"].as_bool()
        ),
        (Some(true), Some(true))
    );
    assert_eq!(old["config_fingerprint"], disabled["config_fingerprint"]);
    let mut changed = config;
    changed.dhcp_ipv4_cidr = Some("10.43.0.0/24".into());
    f.db.insert_or_update_user_network_config(
        (f.owner, f.node),
        dynamic,
        changed,
        ConfigSource::Web,
    )
    .await
    .unwrap();
    let after = store
        .list_instance_candidates(f.owner, &node, "ordinary", None, None)
        .await
        .unwrap();
    let changed = after["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["instance_id"] == dynamic.to_string())
        .unwrap();
    assert_ne!(old["config_fingerprint"], changed["config_fingerprint"]);
    assert_eq!(
        f.store
            .list_instance_candidates(f.other, &node, "ordinary", None, None)
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    sqlx::query("UPDATE user_running_network_configs SET network_config='broken-secret-value' WHERE user_id=? AND device_id=? AND network_instance_id=?")
        .bind(f.owner).bind(&node).bind(dynamic.to_string()).execute(&f.db.inner()).await.unwrap();
    let corrupt = store
        .list_instance_candidates(f.owner, &node, "gateway", None, None)
        .await
        .unwrap();
    assert_eq!(corrupt["items"][0]["role"], "unknown");
    assert_eq!(corrupt["items"][0]["selectable"], false);
    assert!(!corrupt.to_string().contains("broken-secret-value"));
}

#[tokio::test]
async fn stale_preview_rolls_back_creation_and_gateway_occupancy_is_node_scoped() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let body = prepared_body(&f.store, f.owner, &node, device_body()).await;
    seed_config(
        &f.db,
        f.owner,
        f.node,
        DIRECT_INSTANCE.parse().unwrap(),
        "10.0.0.9",
        &[],
    )
    .await;
    let error = f
        .store
        .create_device(f.owner, f.owner, &node, body)
        .await
        .unwrap_err();
    assert_eq!(error.code, "instance_config_changed");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_devices")
        .fetch_one(&f.db.inner())
        .await
        .unwrap();
    let operations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM asset_operations WHERE operation='create_device'")
            .fetch_one(&f.db.inner())
            .await
            .unwrap();
    assert_eq!((count, operations), (0, 0));
    let group = f.group("Gateway").await;
    let instance = group["binding"]["instance_id"].as_str().unwrap();
    let preview_body = json!({"subject":{"resource_kind":"gateway","id":null},"operation":"attach","instance_id":instance});
    assert_eq!(
        f.store
            .preview_binding(f.owner, &node, preview_body.clone())
            .await
            .unwrap_err()
            .code,
        "instance_already_bound"
    );
    let candidates = f
        .store
        .list_instance_candidates(f.owner, &node, "gateway", None, None)
        .await
        .unwrap();
    assert_eq!(candidates["items"][0]["bound_gateway"]["id"], group["id"]);
    seed_config(
        &f.db,
        f.owner,
        f.second_node,
        instance.parse().unwrap(),
        "10.0.0.1",
        &["192.168.1.0/24"],
    )
    .await;
    let preview = f
        .store
        .preview_binding(f.owner, &f.second_node.to_string(), preview_body)
        .await
        .unwrap();
    let second = f.store.create_group(f.owner,f.owner,&f.second_node.to_string(),json!({"request_id":Uuid::new_v4(),"name":"Gateway","binding":{"instance_id":instance},"preview_version":preview["preview_version"]})).await.unwrap();
    assert_ne!(second["id"], group["id"]);
}

#[tokio::test]
async fn missing_replaced_and_role_changed_instances_keep_assets_services_and_manual_confirmations()
{
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.group("Gateway").await;
    let child = f.device(child_body(&group, false)).await;
    let self_device = f.device(child_body(&group, true)).await;
    let service = f
        .store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&child),
            service_body(&["tcp"], 80),
        )
        .await
        .unwrap();
    let instance: Uuid = group["binding"]["instance_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    sqlx::query("DELETE FROM user_running_network_configs WHERE user_id=? AND device_id=? AND network_instance_id=?")
        .bind(f.owner).bind(&node).bind(instance.to_string()).execute(&f.db.inner()).await.unwrap();
    assert_eq!(
        f.store
            .get_device(f.owner, &node, id(&child))
            .await
            .unwrap()["binding"]["state"],
        "missing"
    );
    assert_eq!(
        f.store
            .update_device(f.owner, &node, id(&child), json!({"name":"Still editable"}))
            .await
            .unwrap()["overlay_ip"],
        child["overlay_ip"]
    );
    assert_eq!(
        f.store
            .list_services(f.owner, &node, id(&child))
            .await
            .unwrap()["items"][0]["id"],
        service["id"]
    );
    seed_config(
        &f.db,
        f.owner,
        f.node,
        instance,
        "10.0.0.1",
        &["192.168.1.0/24"],
    )
    .await;
    assert_eq!(
        f.store
            .get_device(f.owner, &node, id(&child))
            .await
            .unwrap()["binding"]["state"],
        "replaced"
    );
    let body = refresh_body(&f, "gateway", &group, "refresh").await;
    let refreshed = f
        .store
        .apply_binding(f.owner, f.owner, &node, "gateway", id(&group), body.clone())
        .await
        .unwrap();
    assert_eq!(refreshed["binding"]["state"], "current");
    assert_eq!(
        f.store
            .get_device(f.owner, &node, id(&child))
            .await
            .unwrap()["binding"]["state"],
        "replaced"
    );
    for device in [&child, &self_device] {
        let preview = f.store.preview_instance_address(f.owner,&node,json!({"subject":{"resource_kind":"device","id":id(device)},"address_spec":device["address_spec"]})).await.unwrap();
        let applied = f.store.apply_address(f.owner,f.owner,&node,id(device),json!({"request_id":Uuid::new_v4(),"address_spec":device["address_spec"],"preview_version":preview["preview_version"]})).await.unwrap();
        assert_eq!(applied["binding"]["state"], "current");
        assert_eq!(applied["overlay_ip"], device["overlay_ip"]);
    }
    seed_config(&f.db, f.owner, f.node, instance, "10.0.0.1", &[]).await;
    let changed = f
        .store
        .get_device(f.owner, &node, id(&child))
        .await
        .unwrap();
    assert_eq!(changed["binding"]["state"], "changed");
    assert_eq!(changed["binding"]["reason"], "binding_role_mismatch");
    // A timed-out operation replays its original result even after the source changes.
    assert_eq!(
        f.store
            .apply_binding(f.owner, f.owner, &node, "gateway", id(&group), body)
            .await
            .unwrap(),
        refreshed
    );
    assert_eq!(
        f.store
            .list_services(f.owner, &node, id(&child))
            .await
            .unwrap()["items"][0]["id"],
        service["id"]
    );
}

#[tokio::test]
async fn legacy_attachment_is_explicit_preserves_children_and_rejects_silent_ipv6_conversion() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.legacy_group("Old gateway", &["192.168.1.0/24"]).await;
    let child = f
        .legacy_device(Some(id(&group)), "2001:db8::12", Some("192.168.1.12"))
        .await;
    let service = f
        .store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&child),
            service_body(&["tcp"], 443),
        )
        .await
        .unwrap();
    let instance = Uuid::new_v4();
    seed_config(
        &f.db,
        f.owner,
        f.node,
        instance,
        "10.0.0.1",
        &["192.168.1.0/24 -> 10.20.1.0/24"],
    )
    .await;
    let preview = f.store.preview_binding(f.owner,&node,json!({"subject":{"resource_kind":"gateway","id":id(&group)},"operation":"attach","instance_id":instance})).await.unwrap();
    assert_eq!(preview["affected_device_ids"], json!([id(&child)]));
    let attached = f.store.apply_binding(f.owner,f.owner,&node,"gateway",id(&group),json!({"request_id":Uuid::new_v4(),"operation":"attach","instance_id":instance,"preview_version":preview["preview_version"]})).await.unwrap();
    assert_eq!(attached["id"], group["id"]);
    let unchanged = f
        .store
        .get_device(f.owner, &node, id(&child))
        .await
        .unwrap();
    assert_eq!(unchanged["binding"]["state"], "legacy_unbound");
    assert_eq!(unchanged["overlay_ip"], "2001:db8::12");
    let spec = json!({"mode":"gateway_rule","gateway_group_id":id(&group),"rule_key":attached["binding"]["snapshot"]["rules"][0]["rule_key"],"host_offset":12});
    let preview = f
        .store
        .preview_instance_address(
            f.owner,
            &node,
            json!({"subject":{"resource_kind":"device","id":id(&child)},"address_spec":spec}),
        )
        .await
        .unwrap();
    let applied = f.store.apply_address(f.owner,f.owner,&node,id(&child),json!({"request_id":Uuid::new_v4(),"address_spec":spec,"preview_version":preview["preview_version"]})).await.unwrap();
    assert_eq!(
        (applied["overlay_ip"].as_str(), applied["lan_ip"].as_str()),
        (Some("10.20.1.12"), Some("192.168.1.12"))
    );
    assert_eq!(
        f.store
            .list_services(f.owner, &node, id(&child))
            .await
            .unwrap()["items"][0]["id"],
        service["id"]
    );
    let direct = f
        .legacy_device(None, "2001:db8::2", Some("2001:db8:1::2"))
        .await;
    let mut attach = json!({"subject":{"resource_kind":"device","id":id(&direct)},"operation":"attach","instance_id":DIRECT_INSTANCE});
    let rejected = f
        .store
        .preview_binding(f.owner, &node, attach.clone())
        .await
        .unwrap();
    assert_eq!(rejected["can_save"], false);
    assert!(rejected["field_errors"].get("lan_ip").is_some());
    attach["lan_ip"] = Value::Null;
    let preview = f
        .store
        .preview_binding(f.owner, &node, attach)
        .await
        .unwrap();
    let body = json!({"request_id":Uuid::new_v4(),"operation":"attach","instance_id":DIRECT_INSTANCE,"lan_ip":null,"preview_version":preview["preview_version"]});
    let saved = f
        .store
        .apply_binding(f.owner, f.owner, &node, "device", id(&direct), body.clone())
        .await
        .unwrap();
    assert!(saved["lan_ip"].is_null());
    let mut changed = body;
    changed.as_object_mut().unwrap().remove("lan_ip");
    assert_eq!(
        f.store
            .apply_binding(f.owner, f.owner, &node, "device", id(&direct), changed)
            .await
            .unwrap_err()
            .code,
        "request_payload_changed"
    );
}

#[tokio::test]
async fn damaged_snapshot_retains_metadata_but_cannot_be_reinterpreted_as_manual_or_block_parent_review()
 {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.group("Gateway").await;
    let device = f.device(child_body(&group, false)).await;
    sqlx::query("UPDATE asset_devices SET binding_snapshot_json=NULL WHERE id=?")
        .bind(id(&device))
        .execute(&f.db.inner())
        .await
        .unwrap();
    let read = f
        .store
        .get_device(f.owner, &node, id(&device))
        .await
        .unwrap();
    assert_eq!(read["binding"]["state"], "unavailable");
    assert_eq!(read["binding"]["can_edit_address"], false);
    assert_eq!(
        f.store
            .update_device(f.owner, &node, id(&device), json!({"name":"Retained"}))
            .await
            .unwrap()["name"],
        "Retained"
    );
    assert_eq!(
        f.store
            .update_device(
                f.owner,
                &node,
                id(&device),
                json!({"overlay_ip":"10.99.0.1"})
            )
            .await
            .unwrap_err()
            .status,
        422
    );
    let preview = f.store.preview_binding(f.owner,&node,json!({"subject":{"resource_kind":"gateway","id":id(&group)},"operation":"refresh","instance_id":group["binding"]["instance_id"]})).await.unwrap();
    assert_eq!(preview["can_save"], true);
    assert_eq!(preview["affected_device_ids"], json!([id(&device)]));
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM asset_operations WHERE operation='apply_binding'")
            .fetch_one(&f.db.inner())
            .await
            .unwrap();
    assert_eq!(rows, 0); // Reads/previews did not write reconciliation operations.
}

#[tokio::test]
async fn failed_binding_write_rolls_back_snapshot_addresses_and_retry_record() {
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let device = f.legacy_device(None, "10.9.0.2", None).await;
    let preview = f.store.preview_binding(f.owner,&node,json!({"subject":{"resource_kind":"device","id":id(&device)},"operation":"attach","instance_id":DIRECT_INSTANCE})).await.unwrap();
    let body = json!({"request_id":Uuid::new_v4(),"operation":"attach","instance_id":DIRECT_INSTANCE,"preview_version":preview["preview_version"]});
    sqlx::query("CREATE TRIGGER reject_bound_address BEFORE UPDATE OF address_mode ON asset_devices BEGIN SELECT RAISE(ABORT,'injected failure'); END").execute(&f.db.inner()).await.unwrap();
    assert!(
        f.store
            .apply_binding(f.owner, f.owner, &node, "device", id(&device), body.clone())
            .await
            .is_err()
    );
    let saved = f
        .store
        .get_device(f.owner, &node, id(&device))
        .await
        .unwrap();
    assert_eq!(saved["binding"]["state"], "legacy_unbound");
    assert_eq!(saved["overlay_ip"], "10.9.0.2");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM asset_operations WHERE operation='apply_binding'")
            .fetch_one(&f.db.inner())
            .await
            .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER reject_bound_address")
        .execute(&f.db.inner())
        .await
        .unwrap();
    assert_eq!(
        f.store
            .apply_binding(f.owner, f.owner, &node, "device", id(&device), body)
            .await
            .unwrap()["overlay_ip"],
        "10.0.0.2"
    );
}

#[tokio::test]
async fn binding_migration_up_and_down_preserve_legacy_ipv6_ids_services_and_foreign_keys() {
    use sea_orm_migration::MigratorTrait;
    let f = Fixture::new().await;
    let node = f.node.to_string();
    let group = f.legacy_group("Legacy gateway", &["2001:db8::/64"]).await;
    let device = f
        .legacy_device(Some(id(&group)), "2001:db8::12", Some("2001:db8::34"))
        .await;
    let service = f
        .store
        .create_service(
            f.owner,
            f.owner,
            &node,
            id(&device),
            service_body(&["tcp", "udp"], 53),
        )
        .await
        .unwrap();
    let orm = sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(f.db.inner());
    crate::migrator::Migrator::down(&orm, Some(1))
        .await
        .unwrap();
    let before: (String, String, Option<String>) =
        sqlx::query_as("SELECT id,overlay_ip,lan_ip FROM asset_devices WHERE id=?")
            .bind(id(&device))
            .fetch_one(&f.db.inner())
            .await
            .unwrap();
    assert_eq!(
        before,
        (
            id(&device).to_owned(),
            "2001:db8::12".into(),
            Some("2001:db8::34".into())
        )
    );
    crate::migrator::Migrator::up(&orm, None).await.unwrap();
    crate::migrator::Migrator::up(&orm, None).await.unwrap();
    let after = f
        .store
        .get_device(f.owner, &node, id(&device))
        .await
        .unwrap();
    assert_eq!(after["binding"]["state"], "legacy_unbound");
    assert_eq!(after["overlay_ip"], device["overlay_ip"]);
    assert_eq!(after["lan_ip"], device["lan_ip"]);
    assert_eq!(after["address_mode"], "legacy_manual");
    assert!(after["network_instance_id"].is_null());
    assert_eq!(
        f.store
            .list_services(f.owner, &node, id(&device))
            .await
            .unwrap()["items"][0]["id"],
        service["id"]
    );
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&f.db.inner())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn central_allocation_is_scoped_and_static_configuration_takes_precedence() {
    let f = Fixture::new().await;
    let instance = Uuid::new_v4();
    for (owner, node, address) in [
        (f.owner, f.node, "10.42.0.12"),
        (f.other, f.node, "10.42.0.13"),
        (f.owner, f.second_node, "10.42.0.14"),
    ] {
        sqlx::query("INSERT INTO networks(user_id,id,display_name,network_name,network_secret,networking_method,create_time,update_time) VALUES(?,?,'Allocated','central','secret','Standalone','now','now') ON CONFLICT(user_id,id) DO NOTHING")
            .bind(owner).bind(instance.to_string()).execute(&f.db.inner()).await.unwrap();
        sqlx::query("INSERT INTO network_members(user_id,network_id,id,device_id,allocated_ipv4,create_time,update_time) VALUES(?,?,?,?,?,'now','now')")
            .bind(owner).bind(instance.to_string()).bind(Uuid::new_v4().to_string()).bind(node.to_string()).bind(address).execute(&f.db.inner()).await.unwrap();
        f.db.insert_or_update_user_network_config(
            (owner, node),
            instance,
            NetworkConfig {
                instance_id: Some(instance.into()),
                dhcp: Some(true),
                ..Default::default()
            },
            ConfigSource::Web,
        )
        .await
        .unwrap();
        let candidates = f
            .store
            .list_instance_candidates(owner, &node.to_string(), "ordinary", None, None)
            .await
            .unwrap();
        let candidate = candidates["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["instance_id"] == instance.to_string())
            .unwrap();
        assert_eq!(candidate["ipv4"], address);
        assert_eq!(candidate["ipv4_source"], "allocated");
    }
    seed_config(&f.db, f.owner, f.node, instance, "10.42.0.99", &[]).await;
    let candidates = f
        .store
        .list_instance_candidates(f.owner, &f.node.to_string(), "ordinary", None, None)
        .await
        .unwrap();
    let candidate = candidates["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["instance_id"] == instance.to_string())
        .unwrap();
    assert_eq!(candidate["ipv4"], "10.42.0.99");
    assert_eq!(candidate["ipv4_source"], "configured");
}

#[tokio::test]
async fn binding_and_address_retries_survive_restart_source_changes_and_deleted_resources() {
    let path = std::env::temp_dir().join(format!("easytier-binding-replay-{}.db", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let f = Fixture::with_db(Db::new(&database_url).await.unwrap()).await;
    let node = f.node.to_string();
    let legacy = f.legacy_device(None, "10.9.0.2", None).await;
    let preview = f.store.preview_binding(f.owner,&node,json!({"subject":{"resource_kind":"device","id":id(&legacy)},"operation":"attach","instance_id":DIRECT_INSTANCE})).await.unwrap();
    let attach_body = json!({"request_id":Uuid::new_v4(),"operation":"attach","instance_id":DIRECT_INSTANCE,"preview_version":preview["preview_version"]});
    let attached = f
        .store
        .apply_binding(
            f.owner,
            f.owner,
            &node,
            "device",
            id(&legacy),
            attach_body.clone(),
        )
        .await
        .unwrap();
    seed_config(
        &f.db,
        f.owner,
        f.node,
        DIRECT_INSTANCE.parse().unwrap(),
        "10.0.0.20",
        &[],
    )
    .await;
    let refresh_body = refresh_body(&f, "device", &attached, "refresh").await;
    let refreshed = f
        .store
        .apply_binding(
            f.owner,
            f.owner,
            &node,
            "device",
            id(&attached),
            refresh_body.clone(),
        )
        .await
        .unwrap();
    let group = f.group("Gateway").await;
    let child = f.device(child_body(&group, false)).await;
    let mut spec = child["address_spec"].clone();
    spec["host_offset"] = json!(20);
    let preview = f
        .store
        .preview_instance_address(
            f.owner,
            &node,
            json!({"subject":{"resource_kind":"device","id":id(&child)},"address_spec":spec}),
        )
        .await
        .unwrap();
    let address_body = json!({"request_id":Uuid::new_v4(),"address_spec":spec,"preview_version":preview["preview_version"]});
    let addressed = f
        .store
        .apply_address(f.owner, f.owner, &node, id(&child), address_body.clone())
        .await
        .unwrap();
    f.db.inner().close().await;
    let reopened = Db::new(&database_url).await.unwrap();
    let store = AssetsStore::new(reopened.clone());
    sqlx::query("DELETE FROM user_running_network_configs WHERE user_id=? AND device_id=?")
        .bind(f.owner)
        .bind(&node)
        .execute(&reopened.inner())
        .await
        .unwrap();
    assert_eq!(
        store
            .apply_binding(f.owner, f.owner, &node, "device", id(&legacy), attach_body)
            .await
            .unwrap(),
        attached
    );
    assert_eq!(
        store
            .apply_binding(
                f.owner,
                f.owner,
                &node,
                "device",
                id(&attached),
                refresh_body
            )
            .await
            .unwrap(),
        refreshed
    );
    assert_eq!(
        store
            .apply_address(f.owner, f.owner, &node, id(&child), address_body.clone())
            .await
            .unwrap(),
        addressed
    );
    assert_eq!(
        store
            .get_device(f.owner, &node, id(&attached))
            .await
            .unwrap()["overlay_ip"],
        "10.0.0.20"
    );
    assert_eq!(
        store.get_device(f.owner, &node, id(&child)).await.unwrap()["lan_ip"],
        "192.168.1.20"
    );
    store
        .delete_device(f.owner, &node, id(&child))
        .await
        .unwrap();
    assert_eq!(
        store
            .apply_address(f.owner, f.owner, &node, id(&child), address_body)
            .await
            .unwrap_err()
            .status,
        410
    );
    drop(store);
    reopened.inner().close().await;
    drop(reopened);
    std::fs::remove_file(&path).unwrap();
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}
