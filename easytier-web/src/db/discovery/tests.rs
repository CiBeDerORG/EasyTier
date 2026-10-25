use super::*;
use crate::db::{DeviceHeartbeatRecord, assets::AssetsStore, automation::AutomationStore};
use easytier::common::config::{ConfigSource, NetworkConfig};
use easytier_core::management::remote_client::Storage;

pub(crate) struct Fixture {
    pub(crate) db: Db,
    assets: AssetsStore,
    store: Store,
    pub(crate) g: Gateway,
    pub(crate) runtime: std::sync::Arc<Runtime>,
}
impl Fixture {
    pub(crate) async fn new() -> Self {
        easytier::utils::init_crypto_provider();
        let db = Db::memory_db().await;
        let owner = db.auto_create_user("ods-owner").await.unwrap().id;
        let node = Uuid::new_v4();
        db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id: owner,
            machine_id: node,
            hostname: "gateway".into(),
            easytier_version: "test".into(),
            device_os: "{}".into(),
            client_url: "tcp://192.0.2.1:1001".into(),
        })
        .await
        .unwrap();
        let assets = AssetsStore::new(db.clone());
        let instance = Uuid::new_v4();
        db.insert_or_update_user_network_config(
            (owner, node),
            instance,
            NetworkConfig {
                instance_id: Some(instance.into()),
                virtual_ipv4: Some("10.0.0.1".into()),
                dhcp: Some(false),
                proxy_cidrs: vec!["192.168.1.0/24->10.60.1.0/24".into()],
                ..Default::default()
            },
            ConfigSource::Web,
        )
        .await
        .unwrap();
        let p=assets.preview_binding(owner,&node.to_string(),json!({"subject":{"resource_kind":"gateway","id":null},"operation":"attach","instance_id":instance})).await.unwrap();
        let group=assets.create_group(owner,owner,&node.to_string(),json!({"request_id":Uuid::new_v4(),"name":"Gateway","binding":{"instance_id":instance},"preview_version":p["preview_version"]})).await.unwrap();
        let t = assets
            .create_type(
                owner,
                owner,
                json!({"request_id":Uuid::new_v4(),"name":"My Router","semantic_kind":"openwrt"}),
            )
            .await
            .unwrap();
        let spec = json!({"mode":"gateway_self","gateway_group_id":group["id"],"rule_key":null,"host_offset":null});
        let p = assets
            .preview_instance_address(
                owner,
                &node.to_string(),
                json!({"subject":{"resource_kind":"device","id":null},"address_spec":spec}),
            )
            .await
            .unwrap();
        let gateway=assets.create_device(owner,owner,&node.to_string(),json!({"request_id":Uuid::new_v4(),"name":"router","device_type_id":t["id"],"address_spec":spec,"preview_version":p["preview_version"]})).await.unwrap();
        let store = Store::new(&db, RuntimeObservations::default());
        let g = store
            .gateway(owner, gateway["id"].as_str().unwrap())
            .await
            .unwrap();
        let runtime = std::sync::Arc::new(Runtime::new(db.clone()).await.unwrap());
        Self {
            db,
            assets,
            store,
            g,
            runtime,
        }
    }
    async fn preview(&self, rows: Value) -> Value {
        let (op, _) = self
            .store
            .start(
                self.g.owner,
                &self.g,
                "devices",
                &json!({"request_id":Uuid::new_v4()}),
                None,
            )
            .await
            .unwrap();
        self.store.preview(&self.g,&op,&json!({"schema_version":1,"has_snapshot":true,"status":"ok","source":"network-aggregate","devices":rows,"errors":[]}),None).await.unwrap()
    }
    async fn import(&self, ip: &str) -> String {
        let p=self.preview(json!([{"ipv4":[ip],"hostname":"observed phone","mac":"02:11:22:33:44:55","observation":"cached"}])).await;
        let v = self.apply(&p, "create", None, vec![]).await;
        assert_eq!(v["rows"][0]["state"], "saved", "{v}");
        v["rows"][0]["device_id"].as_str().unwrap().into()
    }
    async fn apply(
        &self,
        p: &Value,
        action: &str,
        target: Option<&str>,
        keep: Vec<String>,
    ) -> Value {
        self.store.apply_rows(self.g.owner,&self.g,&json!({"request_id":Uuid::new_v4(),"preview_id":p["preview_id"],"preview_revision":p["revision"],"rows":[{"row_id":p["rows"][0]["row_id"],"action":action,"target_device_id":target,"keep_service_ids":keep}]})).await.unwrap()
    }
    async fn adb(&self, entries: Value, status: &str) -> (String, Value) {
        let (op, _) = self
            .store
            .start(
                self.g.owner,
                &self.g,
                "adb",
                &json!({"request_id":Uuid::new_v4()}),
                None,
            )
            .await
            .unwrap();
        let snapshot = json!({"schema_version":1,"has_snapshot":true,"status":status,"source":"avahi","devices":entries,"errors":[]});
        self.store
            .finish(&op, Some(&snapshot), &json!({}))
            .await
            .unwrap();
        let v = self
            .store
            .reconcile_adb(&self.g, &op, &[], true)
            .await
            .unwrap();
        (op, v)
    }
    async fn cleanup_proof(&self) {
        sqlx::query(
            "UPDATE discovery_endpoint_events SET cleanup_json='{\"state\":\"confirmed\"}'",
        )
        .execute(&self.db.inner())
        .await
        .unwrap();
    }
}
fn ep(ip: &str, port: i64) -> Value {
    json!({"address":ip,"port":port,"service":"_adb-tls-connect._tcp","protocol":"IPv4"})
}

#[tokio::test]
async fn supplemental_partial_review_only_finalizes_reviewed_ips_and_expiry_blocks_writes() {
    let f = Fixture::new().await;
    let (adb, _) = f
        .store
        .start(
            f.g.owner,
            &f.g,
            "adb",
            &json!({"request_id":Uuid::new_v4()}),
            None,
        )
        .await
        .unwrap();
    let snapshot = json!({"has_snapshot":true,"status":"ok","source":"avahi","devices":[ep("192.168.1.20",37001),ep("192.168.1.21",37002)],"errors":[]});
    let (op, _) = f
        .store
        .start(
            f.g.owner,
            &f.g,
            "devices",
            &json!({"request_id":Uuid::new_v4()}),
            Some(&adb),
        )
        .await
        .unwrap();
    let preview=f.store.preview(&f.g,&op,&json!({"has_snapshot":true,"status":"ok","devices":[{"ipv4":["192.168.1.20"]},{"ipv4":["192.168.1.21"]}]}),None).await.unwrap();
    f.store
        .finish(&adb, Some(&snapshot), &json!({"supplement_preview_id":op}))
        .await
        .unwrap();
    let first = f.store.reconcile_adb(&f.g, &adb, &[], false).await.unwrap();
    assert!(
        first["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["state"] == "pending_device")
    );
    f.apply(&preview, "skip", None, vec![]).await;
    let next = f.store.reconcile_adb(&f.g, &adb, &[], true).await.unwrap();
    for row in next["rows"].as_array().unwrap() {
        assert_eq!(
            row["state"],
            if row["ip"] == preview["rows"][0]["ipv4"] {
                "skipped"
            } else {
                "pending_device"
            }
        );
    }
    sqlx::query(
        "UPDATE discovery_operations SET expires_at='2000-01-01T00:00:00+00:00' WHERE id=?",
    )
    .bind(&op)
    .execute(&f.db.inner())
    .await
    .unwrap();
    let choice = json!({"row_id":preview["rows"][1]["row_id"],"action":"create"});
    assert!(f.store.apply_row(&f.g, &op, &choice).await.is_err());
}
#[tokio::test]
async fn preview_is_read_only_and_applies_cached_with_derived_address() {
    let f = Fixture::new().await;
    let p = f
        .preview(json!([{"ipv4":["192.168.1.20"],"name":"phone","observation":"cached"}]))
        .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM asset_devices")
            .fetch_one(&f.db.inner())
            .await
            .unwrap(),
        1
    );
    let result = f.apply(&p, "create", None, vec![]).await;
    assert_eq!(result["rows"][0]["state"], "saved", "{result}");
    let d = f
        .assets
        .get_device(
            f.g.owner,
            &f.g.node,
            result["rows"][0]["device_id"].as_str().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(d["overlay_ip"], "10.60.1.20");
    assert_eq!(d["discovery"]["observation"], "cached");
    assert_eq!(d["display_name_manual"], false);
}
#[tokio::test]
async fn request_replay_and_stale_service_review() {
    let f = Fixture::new().await;
    let did = f.import("192.168.1.20").await;
    let p = f
        .preview(json!([{"ipv4":["192.168.1.20"],"hostname":"changed"}]))
        .await;
    f.assets
        .create_service(
            f.g.owner,
            f.g.owner,
            &f.g.node,
            &did,
            json!({"request_id":Uuid::new_v4(),"name":"http","port":80,"protocols":["tcp"]}),
        )
        .await
        .unwrap();
    let result = f.apply(&p, "replace", Some(&did), vec![]).await;
    assert_eq!(result["rows"][0]["code"], "discovery_stale");
    let body = json!({"request_id":Uuid::new_v4()});
    let (id, fresh) = f
        .store
        .start(f.g.owner, &f.g, "adb", &body, None)
        .await
        .unwrap();
    assert!(fresh);
    assert_eq!(
        f.store
            .start(f.g.owner, &f.g, "adb", &body, None)
            .await
            .unwrap(),
        (id, false)
    );
    let mut changed = body;
    changed["changed"] = json!(true);
    assert!(
        f.store
            .start(f.g.owner, &f.g, "adb", &changed, None)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn adb_failed_snapshot_cannot_mutate_and_ambiguity_requires_saved_choice() {
    let f = Fixture::new().await;
    let did = f.import("192.168.1.20").await;
    f.adb(json!([ep("192.168.1.20", 37001)]), "error").await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM asset_services WHERE service_type='adb'"
        )
        .fetch_one(&f.db.inner())
        .await
        .unwrap(),
        0
    );
    let (op, result) = f
        .adb(
            json!([ep("192.168.1.20", 37001), ep("192.168.1.20", 37002)]),
            "ok",
        )
        .await;
    assert_eq!(result["rows"][0]["state"], "choose_endpoint");
    let choice = json!({"row_id":result["rows"][0]["row_id"],"selected_endpoint_id":result["rows"][0]["endpoints"][1]["endpoint_id"]});
    let r = f
        .store
        .reconcile_adb(&f.g, &op, &[choice], true)
        .await
        .unwrap();
    assert_eq!(r["rows"][0]["port"], 37002, "{r}");
    let d = f
        .assets
        .get_device(f.g.owner, &f.g.node, &did)
        .await
        .unwrap();
    assert_eq!(d["semantic_kind"], "phone");
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM discovery_endpoint_events")
        .fetch_one(&f.db.inner())
        .await
        .unwrap();
    f.adb(json!([ep("192.168.1.20", 37002)]), "ok").await;
    assert_eq!(
        before,
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM discovery_endpoint_events")
            .fetch_one(&f.db.inner())
            .await
            .unwrap()
    );
}
#[tokio::test]
async fn overwrite_keeps_selected_service_identity_and_rolls_back_invalid_selection() {
    let f = Fixture::new().await;
    let old = f.import("192.168.1.20").await;
    let s = f
        .assets
        .create_service(
            f.g.owner,
            f.g.owner,
            &f.g.node,
            &old,
            json!({"request_id":Uuid::new_v4(),"name":"http","port":80,"protocols":["tcp"]}),
        )
        .await
        .unwrap();
    let p = f
        .preview(json!([{"ipv4":["192.168.1.20"],"hostname":"replacement"}]))
        .await;
    let failed = f
        .apply(&p, "replace", Some(&old), vec![Uuid::new_v4().to_string()])
        .await;
    assert_eq!(failed["rows"][0]["state"], "failed");
    assert!(
        f.assets
            .get_device(f.g.owner, &f.g.node, &old)
            .await
            .is_ok()
    );
    let result = f
        .apply(
            &p,
            "replace",
            Some(&old),
            vec![s["id"].as_str().unwrap().into()],
        )
        .await;
    assert_eq!(result["rows"][0]["state"], "saved", "{result}");
    let new = result["rows"][0]["device_id"].as_str().unwrap();
    assert_ne!(new, old);
    assert!(
        f.assets
            .get_device(f.g.owner, &f.g.node, &old)
            .await
            .is_err()
    );
    let services = f
        .assets
        .list_services(f.g.owner, &f.g.node, new)
        .await
        .unwrap();
    assert_eq!(services["items"][0]["id"], s["id"]);
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&f.db.inner())
            .await
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn manual_name_and_phone_type_survive_unknown_discovery_fields() {
    let f = Fixture::new().await;
    let did = f.import("192.168.1.20").await;
    f.adb(json!([ep("192.168.1.20", 37001)]), "ok").await;
    let mut body = f
        .assets
        .get_device(f.g.owner, &f.g.node, &did)
        .await
        .unwrap();
    body =
        json!({"name":"My own name","model":"Known model","device_type_id":body["device_type_id"]});
    f.assets
        .update_device(f.g.owner, &f.g.node, &did, body)
        .await
        .unwrap();
    let p = f
        .preview(json!([{"ipv4":["192.168.1.20"],"hostname":null,"mac":null,"model":null}]))
        .await;
    let r = f.apply(&p, "update", Some(&did), vec![]).await;
    assert_eq!(r["rows"][0]["state"], "saved", "{r}");
    let d = f
        .assets
        .get_device(f.g.owner, &f.g.node, &did)
        .await
        .unwrap();
    assert_eq!(d["name"], "My own name");
    assert_eq!(d["model"], "Known model");
    assert_eq!(d["semantic_kind"], "phone");
    assert_eq!(d["discovery"]["mac"], "02:11:22:33:44:55");
}
#[tokio::test]
async fn port_change_revokes_only_one_device_in_a_two_device_run() {
    let f = Fixture::new().await;
    let a = f.import("192.168.1.20").await;
    let b = f.import("192.168.1.21").await;
    f.adb(
        json!([ep("192.168.1.20", 37001), ep("192.168.1.21", 37001)]),
        "ok",
    )
    .await;
    f.cleanup_proof().await;
    let automation = AutomationStore::new(f.db.clone());
    let(run,_)=automation.create_run(f.g.owner,f.g.owner,&json!({"request_id":Uuid::new_v4(),"name":"two phones","device_ids":[a,b],"parameters":{},"steps":[{"id":Uuid::new_v4(),"type":"key","key":"HOME"}],"concurrency":2})).await.unwrap();
    let rid = run["id"].as_str().unwrap();
    let executor = Uuid::new_v4().to_string();
    automation
        .bind_run_executor(f.g.owner, rid, &executor)
        .await
        .unwrap();
    let targets = automation.dispatch_payload(f.g.owner, rid).await.unwrap();
    f.adb(json!([ep("192.168.1.20", 37002)]), "ok").await;
    let run = automation.run(f.g.owner, rid).await.unwrap();
    assert_eq!(run["cancel_requested"], false);
    let devices = run["devices"].as_array().unwrap();
    assert_eq!(
        devices.iter().find(|d| d["device_id"] == a).unwrap()["withdraw_requested"],
        true
    );
    assert_eq!(
        devices.iter().find(|d| d["device_id"] == b).unwrap()["withdraw_requested"],
        false
    );
    for target in targets["targets"].as_array().unwrap() {
        let did = target["device_id"].as_str().unwrap();
        assert_eq!(
            automation
                .authorize_run_step(
                    rid,
                    did,
                    target["target_fingerprint"].as_str().unwrap(),
                    &executor
                )
                .await
                .unwrap(),
            did == b
        );
    }
}
#[tokio::test]
async fn withdrawal_before_dispatch_keeps_other_phone_dispatchable() {
    let f = Fixture::new().await;
    let a = f.import("192.168.1.20").await;
    let b = f.import("192.168.1.21").await;
    f.adb(
        json!([ep("192.168.1.20", 37001), ep("192.168.1.21", 37001)]),
        "ok",
    )
    .await;
    f.cleanup_proof().await;
    let store = AutomationStore::new(f.db.clone());
    let (run,_)=store.create_run(f.g.owner,f.g.owner,&json!({"request_id":Uuid::new_v4(),"name":"before dispatch","device_ids":[a,b],"steps":[{"id":Uuid::new_v4(),"type":"key","key":"HOME"}]})).await.unwrap();
    let rid = run["id"].as_str().unwrap();
    store
        .withdraw_device_runs(f.g.owner, &a, &Uuid::new_v4().to_string())
        .await
        .unwrap();
    let executor = Uuid::new_v4().to_string();
    store
        .bind_run_executor(f.g.owner, rid, &executor)
        .await
        .unwrap();
    let dispatch = store.dispatch_payload(f.g.owner, rid).await.unwrap();
    for t in dispatch["targets"].as_array().unwrap() {
        let did = t["device_id"].as_str().unwrap();
        assert_eq!(
            store
                .authorize_run_step(
                    rid,
                    did,
                    t["target_fingerprint"].as_str().unwrap(),
                    &executor
                )
                .await
                .unwrap(),
            did == b
        );
    }
}
#[tokio::test]
async fn same_ip_is_allowed_only_across_gateway_groups() {
    let f = Fixture::new().await;
    f.import("192.168.1.20").await;
    let other = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO asset_gateway_groups(id,owner_user_id,machine_id,name,created_at,updated_at) VALUES(?,?,?,'second',?,?)").bind(&other).bind(f.g.owner).bind(&f.g.node).bind(now()).bind(now()).execute(&f.db.inner()).await.unwrap();
    let insert = "INSERT INTO asset_devices(id,owner_user_id,machine_id,gateway_group_id,name,overlay_ip,lan_ip,created_at,updated_at) VALUES(?,?,?,?,'duplicate LAN','10.61.1.20','192.168.1.20',?,?)";
    assert!(
        sqlx::query(insert)
            .bind(Uuid::new_v4().to_string())
            .bind(f.g.owner)
            .bind(&f.g.node)
            .bind(&other)
            .bind(now())
            .bind(now())
            .execute(&f.db.inner())
            .await
            .is_ok()
    );
    assert!(
        sqlx::query(insert)
            .bind(Uuid::new_v4().to_string())
            .bind(f.g.owner)
            .bind(&f.g.node)
            .bind(&other)
            .bind(now())
            .bind(now())
            .execute(&f.db.inner())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn adb_publication_keeps_listener_and_identity_through_port_change_and_replacement() {
    use crate::db::assets::PublicationRuntimeProof;
    let f = Fixture::new().await;
    let did = f.import("192.168.1.20").await;
    let (_, r) = f.adb(json!([ep("192.168.1.20", 37001)]), "ok").await;
    let sid = r["rows"][0]["adb_service_id"].as_str().unwrap();
    let instance = Uuid::new_v4();
    let node: Uuid = f.g.node.parse().unwrap();
    sqlx::query("INSERT INTO node_asset_metadata(owner_user_id,machine_id,is_public_server) VALUES(?,?,1) ON CONFLICT(owner_user_id,machine_id) DO UPDATE SET is_public_server=1").bind(f.g.owner).bind(&f.g.node).execute(&f.db.inner()).await.unwrap();
    f.db.insert_or_update_user_network_config(
        (f.g.owner, node),
        instance,
        NetworkConfig {
            instance_id: Some(instance.into()),
            network_name: Some("Public".into()),
            ..Default::default()
        },
        ConfigSource::Web,
    )
    .await
    .unwrap();
    let assets = f
        .assets
        .clone()
        .with_publication_runtime_proof(Some(PublicationRuntimeProof {
            owner_user_id: f.g.owner,
            machine_id: f.g.node.clone(),
            instance_id: instance.to_string(),
        }));
    let published=assets.update_service(f.g.owner,&f.g.node,sid,json!({"name":"ADB","port":37001,"protocols":["tcp"],"publication":{"machine_id":node,"instance_id":instance,"bind_ip":"0.0.0.0","bind_port":41001,"dst_port":37001}})).await.unwrap();
    assert_eq!(published["publication"]["bind_port"], 41001);
    let (_, changed) = f.adb(json!([ep("192.168.1.20", 37002)]), "ok").await;
    assert_eq!(changed["rows"][0]["state"], "saved", "{changed}");
    let row=sqlx::query("SELECT bind_port,dst_port,config_record_id FROM asset_service_publications WHERE service_id=?").bind(sid).fetch_one(&f.db.inner()).await.unwrap();
    assert_eq!(row.get::<i64, _>("bind_port"), 41001);
    assert_eq!(row.get::<i64, _>("dst_port"), 37002);
    let config: i64 = row.get("config_record_id");
    let raw: String =
        sqlx::query_scalar("SELECT network_config FROM user_running_network_configs WHERE id=?")
            .bind(config)
            .fetch_one(&f.db.inner())
            .await
            .unwrap();
    let raw: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(raw["port_forwards"][0]["dst_port"], 37002);
    let preview = f
        .preview(json!([{"ipv4":["192.168.1.20"],"hostname":"Replacement"}]))
        .await;
    let replaced = f
        .apply(&preview, "replace", Some(&did), vec![sid.into()])
        .await;
    assert_eq!(replaced["rows"][0]["state"], "saved", "{replaced}");
    let new = replaced["rows"][0]["device_id"].as_str().unwrap();
    let row=sqlx::query("SELECT device_id,bind_port,dst_port,config_record_id FROM asset_service_publications WHERE service_id=?").bind(sid).fetch_one(&f.db.inner()).await.unwrap();
    assert_eq!(row.get::<String, _>("device_id"), new);
    assert_eq!(row.get::<i64, _>("config_record_id"), config);
    assert_eq!(row.get::<i64, _>("bind_port"), 41001);
    assert_eq!(row.get::<i64, _>("dst_port"), 37002);
    assert!(
        AutomationStore::new(f.db.clone())
            .target(f.g.owner, new)
            .await
            .is_err()
    );
    sqlx::query("UPDATE discovery_endpoint_events SET cleanup_json='{\"state\":\"confirmed\"}' WHERE device_id=?").bind(new).execute(&f.db.inner()).await.unwrap();
    assert!(
        AutomationStore::new(f.db.clone())
            .target(f.g.owner, new)
            .await
            .is_err(),
        "new identity must wait for predecessor cleanup"
    );
    f.cleanup_proof().await;
    assert!(
        AutomationStore::new(f.db.clone())
            .target(f.g.owner, new)
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn credentials_preserve_exact_bytes_and_never_leave_read_api() {
    let f = Fixture::new().await;
    let runtime = Runtime::new(f.db.clone()).await.unwrap();
    let secret = "  x$`\"中文\n exact  ";
    let saved=f.store.save_access(&f.g,&json!({"endpoint_url":"https://10.0.0.1/ubus","ca_pem":"","secret_action":"replace","secret":secret,"expected_revision":0}),&runtime).await.unwrap();
    assert_eq!(saved["has_secret"], true);
    assert!(!saved.to_string().contains(secret));
    let (_, plain) = f.store.client(&f.g, &runtime).await.unwrap();
    assert_eq!(plain, secret);
    f.store.save_access(&f.g,&json!({"endpoint_url":"https://10.0.0.1/ubus","ca_pem":"","secret_action":"preserve","expected_revision":1}),&runtime).await.unwrap();
    assert_eq!(f.store.client(&f.g, &runtime).await.unwrap().1, secret);
    f.store.save_access(&f.g,&json!({"endpoint_url":"https://10.0.0.1/ubus","ca_pem":"","secret_action":"clear","expected_revision":2}),&runtime).await.unwrap();
    assert!(f.store.client(&f.g, &runtime).await.is_err());
}
#[test]
fn failed_or_wrong_source_adb_arrays_never_become_targets() {
    let good = json!({"status":"ok","has_snapshot":true,"source":"avahi","devices":[ep("192.168.1.20",37001)],"errors":[]});
    assert_eq!(super::adb::endpoints(&good).len(), 1);
    for (key, value) in [
        ("status", json!("error")),
        ("status", json!("busy")),
        ("has_snapshot", json!(false)),
        ("source", json!("network-aggregate")),
        ("errors", json!([{"source":"avahi","code":"timeout"}])),
    ] {
        let mut v = good.clone();
        v[key] = value;
        assert!(super::adb::endpoints(&v).is_empty(), "{v}");
    }
    let mut partial = good.clone();
    partial["status"] = json!("partial");
    assert!(super::adb::endpoints(&partial).is_empty());
    partial["sources"] = json!([{"source":"avahi","status":"ok"}]);
    assert_eq!(super::adb::endpoints(&partial).len(), 1);
}

#[tokio::test]
async fn cleanup_failure_blocks_connect_and_explicit_stage_retry_does_not_repeat_success() {
    use crate::{
        central_network::service::CentralNetworkService,
        client_manager::{ClientManager, HeartbeatPolicy},
        discovery::reconnect,
        restful::automation::runner::Runner,
    };
    use axum::{
        Json, Router,
        extract::{Path, State},
        routing::post,
    };
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    #[derive(Clone)]
    struct Mock {
        calls: Arc<Mutex<Vec<String>>>,
        clean: Arc<AtomicBool>,
        executor: String,
    }
    async fn retire(
        State(s): State<Mock>,
        Path(did): Path<String>,
        Json(v): Json<Value>,
    ) -> Json<Value> {
        s.calls.lock().unwrap().push("retire".into());
        Json(
            json!({"device_id":did,"event_id":v["event_id"],"executor_id":s.executor,"cleanup_confirmed":s.clean.load(Ordering::SeqCst)}),
        )
    }
    async fn prepare(State(s): State<Mock>, Json(v): Json<Value>) -> Json<Value> {
        s.calls
            .lock()
            .unwrap()
            .push(format!("connect:{}", v["target"]["adb_port"]));
        Json(json!({"state":"connected"}))
    }
    let f = Fixture::new().await;
    let did = f.import("192.168.1.20").await;
    f.adb(json!([ep("192.168.1.20", 37001)]), "ok").await;
    f.cleanup_proof().await;
    let old = AutomationStore::new(f.db.clone())
        .target(f.g.owner, &did)
        .await
        .unwrap();
    let executor = Uuid::new_v4().to_string();
    let manager = Arc::new(ClientManager::new(
        f.db.clone(),
        None,
        HeartbeatPolicy::default(),
        Arc::new(crate::FeatureFlags::default()),
        Arc::new(crate::webhook::WebhookConfig::new(
            None, None, None, None, None,
        )),
    ));
    let service = CentralNetworkService::new(f.db.clone(), manager.clone());
    reconnect::remember_target(&service, &old, &executor)
        .await
        .unwrap();
    f.adb(json!([ep("192.168.1.20", 37002)]), "ok").await;
    let mock = Mock {
        calls: Arc::new(Mutex::new(Vec::new())),
        clean: Arc::new(AtomicBool::new(false)),
        executor: executor.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let health = json!({"protocol_version":1,"executor_id":executor});
    let app = Router::new()
        .route(
            "/internal/v1/health",
            axum::routing::get(move || {
                let v = health.clone();
                async move { Json(v) }
            }),
        )
        .route("/internal/v1/targets/{id}/retire", post(retire))
        .route("/internal/v1/prepare", post(prepare))
        .with_state(mock.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let runtime = Runtime::new(f.db.clone()).await.unwrap();
    runtime.settings.set_runner_for_test(
        Runner::configured(&format!("http://127.0.0.1:{port}"), "test-token".repeat(8)));
    let first = reconnect::advance(&manager, &service, &runtime, f.g.owner, &did, None)
        .await
        .unwrap();
    assert_eq!(first["cleanup"]["state"], "pending");
    assert_eq!(*mock.calls.lock().unwrap(), vec!["retire"]);
    let event = first["event_id"].as_str().unwrap();
    assert!(
        reconnect::advance(
            &manager,
            &service,
            &runtime,
            f.g.owner,
            &did,
            Some((event, "connection"))
        )
        .await
        .is_err()
    );
    mock.clean.store(true, Ordering::SeqCst);
    let clean = reconnect::advance(
        &manager,
        &service,
        &runtime,
        f.g.owner,
        &did,
        Some((event, "cleanup")),
    )
    .await
    .unwrap();
    assert_eq!(clean["cleanup"]["state"], "confirmed");
    assert_eq!(*mock.calls.lock().unwrap(), vec!["retire", "retire"]);
    let connected = reconnect::advance(
        &manager,
        &service,
        &runtime,
        f.g.owner,
        &did,
        Some((event, "connection")),
    )
    .await
    .unwrap();
    assert_eq!(connected["connection"]["state"], "connected", "{connected}");
    reconnect::advance(
        &manager,
        &service,
        &runtime,
        f.g.owner,
        &did,
        Some((event, "connection")),
    )
    .await
    .unwrap();
    assert_eq!(
        *mock.calls.lock().unwrap(),
        vec!["retire", "retire", "connect:37002"]
    );
    server.abort();
}

#[tokio::test]
async fn first_adb_publication_cleanup_needs_no_runner_health_when_no_resources_exist() {
    use crate::{
        central_network::service::CentralNetworkService,
        client_manager::{ClientManager, HeartbeatPolicy},
        discovery::reconnect,
        restful::automation::runner::Runner,
    };
    use std::sync::Arc;
    let f = Fixture::new().await;
    let did = f.import("192.168.1.20").await;
    f.adb(json!([ep("192.168.1.20", 37001)]), "ok").await;
    // No pairing, session, run or registered old endpoint has EVER existed.
    let old_targets: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM discovery_targets WHERE device_id=? AND retired=0"
    ).bind(&did).fetch_one(&f.db.inner()).await.unwrap();
    assert_eq!(old_targets, 0);
    let manager = Arc::new(ClientManager::new(
        f.db.clone(), None, HeartbeatPolicy::default(),
        Arc::new(crate::FeatureFlags::default()),
        Arc::new(crate::webhook::WebhookConfig::new(None, None, None, None, None)),
    ));
    let service = CentralNetworkService::new(f.db.clone(), manager.clone());
    let runtime = Runtime::new(f.db.clone()).await.unwrap();
    // Runner configuration exists but is not reachable during this first
    // registration; that must not be misreported as pending old-target cleanup.
    runtime.settings.set_runner_for_test(
        Runner::configured("http://127.0.0.1:1", "test-token".repeat(8)),
    );
    let result = reconnect::advance(&manager, &service, &runtime, f.g.owner, &did, None).await.unwrap();
    assert_eq!(result["cleanup"]["state"], "confirmed", "{result}");
    assert_eq!(result["cleanup"]["code"], "no_previous_resources");
    assert_eq!(result["connection"]["state"], "pending", "{result}");
    let event = result["event_id"].as_str().unwrap();
    let retry = reconnect::advance(
        &manager, &service, &runtime, f.g.owner, &did, Some((event, "cleanup"))
    ).await.unwrap();
    assert_eq!(retry["cleanup"]["state"], "confirmed");
    // First user-initiated pairing now passes the old-target retirement fence.
    assert!(AutomationStore::new(f.db.clone()).target(f.g.owner, &did).await.is_ok());
}
