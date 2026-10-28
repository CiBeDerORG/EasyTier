//! T49 · AutomationStore / SQLite migration / formal script CAS regression.
//! Uses real Db::memory_db migrations, not a fabricated SQL schema.
//! No Rust test/build, executor request, ADB or Android device is run in debug mode.

use serde_json::{Value, json};
use easytier::common::config::{ConfigSource, NetworkConfig};
use easytier_core::management::remote_client::Storage;
use uuid::Uuid;

use super::{AutomationStore, validation};
use crate::db::{Db, DeviceHeartbeatRecord, assets::AssetsStore};

struct Fixture {
    db: Db,
    store: AutomationStore,
    owner: i32,
    other: i32,
}

impl Fixture {
    async fn new() -> Self {
        let db = Db::memory_db().await;
        let owner = db.auto_create_user("t49-script-owner").await.unwrap().id;
        let other = db.auto_create_user("t49-script-other").await.unwrap().id;
        Self { db:db.clone(),store: AutomationStore::new(db), owner, other }
    }

    /// A real AssetsStore-created virtual phone, with a current route and
    /// exactly one TCP ADB service. This never contacts a physical phone.
    async fn phone(&self) -> String {
        let node=Uuid::new_v4();
        let instance=Uuid::new_v4();
        self.db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id:self.owner,machine_id:node,
            hostname:"T49 virtual node".into(),easytier_version:"test".into(),
            device_os:"{}".into(),client_url:"tcp://192.0.2.11:1001".into(),
        }).await.unwrap();
        self.db.insert_or_update_user_network_config(
            (self.owner,node),instance,
            NetworkConfig {
                instance_id:Some(instance.into()),
                virtual_ipv4:Some("10.89.0.20".into()),
                dhcp:Some(false),
                ..Default::default()
            },ConfigSource::Web,
        ).await.unwrap();
        let assets=AssetsStore::new(self.db.clone());
        let kind=assets.create_type(self.owner,self.owner,json!({
            "request_id":Uuid::new_v4(),"name":"T49 phone","semantic_kind":"phone"
        })).await.unwrap();
        let spec=json!({"mode":"direct_instance","instance_id":instance,"lan_ip":null});
        let preview=assets.preview_instance_address(
            self.owner,&node.to_string(),json!({
                "subject":{"resource_kind":"device","id":null},
                "address_spec":spec
            })
        ).await.unwrap();
        let device=assets.create_device(self.owner,self.owner,&node.to_string(),json!({
            "request_id":Uuid::new_v4(),"name":"T49 mobile",
            "address_spec":spec,"device_type_id":kind["id"],
            "preview_version":preview["preview_version"],
        })).await.unwrap();
        let id=device["id"].as_str().unwrap().to_owned();
        let adb=assets.create_service(self.owner,self.owner,&node.to_string(),&id,json!({
            "request_id":Uuid::new_v4(),"name":"T49 ADB",
            "protocols":["tcp"],"port":37001,
            "service_type":"adb","connection_mode":"wireless",
        })).await.unwrap();
        assert_eq!(adb["service_type"],"adb");
        // Asset creation deliberately records a pending endpoint retirement.
        // In this in-memory-only fixture, simulate the executor's *confirmed*
        // cleanup reply for this specific virtual phone before admitting it.
        // Do not relax the production target_cleanup_pending guard.
        let cleanup=sqlx::query(
            r#"UPDATE discovery_endpoint_events
               SET cleanup_json='{"state":"confirmed"}'
               WHERE owner_user_id=? AND device_id=?
                 AND seq=(SELECT MAX(seq) FROM discovery_endpoint_events
                          WHERE owner_user_id=? AND device_id=?)"#
        ).bind(self.owner).bind(&id).bind(self.owner).bind(&id)
        .execute(&self.db.inner()).await.unwrap();
        assert_eq!(cleanup.rows_affected(),1,
            "T49 fixture must explicitly acknowledge its virtual endpoint cleanup");
        let target=self.store.target(self.owner,&id).await.unwrap();
        assert_eq!(target["device_id"],id);
        assert!(target["target_fingerprint"].is_string());
        id
    }

    async fn create(&self, name: &str, steps: Value) -> Value {
        self.store.save_script(self.owner, None, &json!({
            "name":name, "description":"T49 regression", "steps":steps,
        })).await.unwrap()
    }
}

fn valid_step() -> Value {
    json!({"id":Uuid::new_v4().to_string(),"type":"key","key":"HOME"})
}

fn status(result: super::AutomationResult<Value>) -> u16 {
    result.unwrap_err().status
}

#[tokio::test]
async fn t49_empty_formal_script_can_be_persisted_but_is_not_an_executable_snapshot() {
    let f=Fixture::new().await;
    let saved=f.create("empty script",json!([])).await;
    assert_eq!(saved["revision"],1);
    assert_eq!(saved["steps"],json!([]));

    let listed=f.store.list_scripts(f.owner).await.unwrap();
    assert_eq!(listed["items"].as_array().unwrap().len(),1);
    assert_eq!(listed["items"][0]["steps"],json!([]));

    // create_run calls validation::steps on either formal or inline snapshots.
    // Only save_script calls saved_steps; the empty shape MUST fail to execute.
    assert!(validation::saved_steps(&saved["steps"]).is_ok());
    assert!(validation::steps(&saved["steps"]).is_err());
    assert!(validation::steps(&json!([])).is_err());
    assert!(validation::steps(&json!([valid_step()])).is_ok());
}

#[tokio::test]
async fn t49_update_requires_current_revision_and_preserves_newer_row_on_conflict() {
    let f=Fixture::new().await;
    let saved=f.create("initial",json!([valid_step()])).await;
    let id=saved["id"].as_str().unwrap();

    let changed=f.store.save_script(f.owner,Some(id),&json!({
        "name":"newer","description":"other browser saved",
        "steps":saved["steps"],"revision":1
    })).await.unwrap();
    assert_eq!(changed["revision"],2);
    let old=f.store.save_script(f.owner,Some(id),&json!({
        "name":"stale overwrite","description":"",
        "steps":saved["steps"],"revision":1
    })).await;
    assert_eq!(status(old),409,"CAS must refuse stale browser");

    let latest=f.store.list_scripts(f.owner).await.unwrap();
    assert_eq!(latest["items"][0]["name"],"newer");
    assert_eq!(latest["items"][0]["revision"],2);
    assert_eq!(latest["items"][0]["steps"],saved["steps"]);
}

#[tokio::test]
async fn t49_revisioned_delete_never_erases_newer_revision_or_foreign_owner_script() {
    let f=Fixture::new().await;
    let saved=f.create("keep current",json!([valid_step()])).await;
    let id=saved["id"].as_str().unwrap();
    let revised=f.store.save_script(f.owner,Some(id),&json!({
        "name":"new version","description":"unchanged","steps":saved["steps"],"revision":1,
    })).await.unwrap();
    assert_eq!(revised["revision"],2);
    assert_eq!(status(f.store.delete_script_if_revision(f.owner,id,1).await),409);
    assert_eq!(status(f.store.delete_script_if_revision(f.other,id,2).await),404);
    assert_eq!(status(f.store.delete_script_if_revision(f.owner,id,0).await),422);

    let current=f.store.list_scripts(f.owner).await.unwrap();
    assert_eq!(current["items"][0]["name"],"new version");
    assert_eq!(current["items"][0]["revision"],2);
    assert_eq!(f.store.list_scripts(f.other).await.unwrap()["items"],json!([]));

    let deleted=f.store.delete_script_if_revision(f.owner,id,2).await.unwrap();
    assert_eq!(deleted["deleted"],true);
    assert_eq!(deleted["id"],id);
    assert_eq!(deleted["revision"],2);
    assert_eq!(f.store.list_scripts(f.owner).await.unwrap()["items"],json!([]));
    assert_eq!(status(f.store.delete_script_if_revision(f.owner,id,2).await),404);
}

#[tokio::test]
async fn t49_owner_scoping_and_actor_authorization_are_not_inferred_from_a_script_id() {
    let f=Fixture::new().await;
    let saved=f.create("private",json!([])).await;
    let id=saved["id"].as_str().unwrap();
    assert_eq!(f.store.list_scripts(f.other).await.unwrap()["items"],json!([]));
    assert_eq!(status(f.store.save_script(f.other,Some(id),&json!({
        "name":"foreign edit","description":"","steps":[],
        "revision":1
    })).await),404);
    assert_eq!(status(f.store.delete_script_if_revision(f.other,id,1).await),404);
    let forbidden=f.store.authorize(f.other,f.owner).await.unwrap_err();
    assert_eq!(forbidden.status,403,"non-admin actor cannot manage another owner");
    f.store.authorize(f.owner,f.owner).await.unwrap();
    let own=f.store.list_scripts(f.owner).await.unwrap();
    assert_eq!(own["items"][0]["name"],"private");
}

#[tokio::test]
async fn t49_invalid_script_update_cannot_advance_revision_or_destroy_original() {
    let f=Fixture::new().await;
    let saved=f.create("valid first",json!([valid_step()])).await;
    let id=saved["id"].as_str().unwrap();
    let bad_step=json!({"id":Uuid::new_v4().to_string(),"type":"key","key":"UNKNOWN"});
    let invalid=f.store.save_script(f.owner,Some(id),&json!({
        "name":"bad next","description":"","steps":[bad_step],"revision":1,
    })).await;
    assert!(invalid.is_err(),"an unsupported action must not be persisted");

    let latest=f.store.list_scripts(f.owner).await.unwrap();
    assert_eq!(latest["items"][0]["revision"],1);
    assert_eq!(latest["items"][0]["name"],"valid first");
    assert_eq!(latest["items"][0]["steps"],saved["steps"]);
}

#[tokio::test]
async fn t49_foreign_actor_cannot_create_run_or_create_dispatch_rows() {
    let f=Fixture::new().await;
    let script=f.create("owner only",json!([valid_step()])).await;
    let device_id=Uuid::new_v4().to_string();
    let attempted=f.store.create_run(f.other,f.owner,&json!({
        "request_id":Uuid::new_v4().to_string(),
        "script_id":script["id"],"revision":script["revision"],
        "device_ids":[device_id],"parameters":{},"concurrency":1,
    })).await;
    let denied=attempted.unwrap_err();
    assert_eq!(denied.status,403,"account authorization must run BEFORE device discovery");
    assert_eq!(denied.code,"automation_access_denied");

    let runs=f.store.list_runs(f.owner).await.unwrap();
    assert_eq!(runs["items"],json!([]),"a foreign actor must not persist dispatch intent");
}

#[tokio::test]
async fn t49_reused_run_request_id_with_changed_body_fails_before_any_device_action() {
    let f=Fixture::new().await;
    // Persist an incomplete historical dispatch intent via the genuine
    // migrated SQL table, without starting an executor or an Android phone.
    // The request_id collision check happens before target discovery.
    let request=Uuid::new_v4().to_string();
    let old_run=Uuid::new_v4().to_string();
    let now=super::now();
    let db=f.store.pool.clone();
    sqlx::query(
        "INSERT INTO automation_runs(id,actor_user_id,owner_user_id,request_id,request_hash,\
         name,state,snapshot_json,parameters_json,concurrency,created_at,updated_at) \
         VALUES(?,?,?,?,?,'old run','dispatch_pending','{}','{}',1,?,?)"
    )
    .bind(&old_run).bind(f.owner).bind(f.owner).bind(&request).bind("old-digest")
    .bind(&now).bind(&now).execute(&db).await.unwrap();

    let attempt=f.store.create_run(f.owner,f.owner,&json!({
        "request_id":request,
        "name":"different payload",
        "steps":[], "device_ids":[Uuid::new_v4().to_string()],
        "parameters":{},"concurrency":1
    })).await.unwrap_err();
    assert_eq!(attempt.status,409);
    assert_eq!(attempt.code,"idempotency_conflict");

    let identity:String=sqlx::query_scalar(
        "SELECT request_hash FROM automation_runs WHERE id=?"
    ).bind(&old_run).fetch_one(&db).await.unwrap();
    assert_eq!(identity,"old-digest","a conflicting retry must leave the original intent untouched");
}

#[tokio::test]
async fn t49_empty_saved_script_is_rejected_by_real_create_run_even_with_a_valid_phone() {
    let f=Fixture::new().await;
    let phone=f.phone().await;
    let saved=f.create("empty published",json!([])).await;
    let body=json!({
        "request_id":Uuid::new_v4(),
        "script_id":saved["id"],"revision":saved["revision"],
        "device_ids":[phone],"parameters":{},"concurrency":1,
    });
    let rejected=f.store.create_run(f.owner,f.owner,&body).await.unwrap_err();
    assert_eq!(rejected.status,422,"an empty saved script is not executable");
    assert!(rejected.field_errors.contains_key("steps"));
    assert_eq!(f.store.list_runs(f.owner).await.unwrap()["items"],json!([]),
        "invalid execution must not persist a run or dispatch request");
}

#[tokio::test]
async fn t49_run_uuid_is_idempotent_and_revision_is_checked_before_dispatch() {
    let f=Fixture::new().await;
    let phone=f.phone().await;
    let script=f.create("versioned",json!([valid_step()])).await;
    let body=json!({
        "request_id":Uuid::new_v4().to_string(),
        "script_id":script["id"],"revision":1,
        "device_ids":[phone],"parameters":{},"concurrency":1,
    });
    let (initial,created)=f.store.create_run(f.owner,f.owner,&body).await.unwrap();
    assert!(created);
    assert_eq!(initial["state"],"dispatch_pending");
    let (replayed,created)=f.store.create_run(f.owner,f.owner,&body).await.unwrap();
    assert!(!created,"replaying an identical UUID must not create a second run");
    assert_eq!(replayed["id"],initial["id"]);
    let mut different=body.clone();
    different["concurrency"]=json!(2);
    let conflict=f.store.create_run(f.owner,f.owner,&different).await.unwrap_err();
    assert_eq!(conflict.status,409);
    assert_eq!(conflict.code,"idempotency_conflict");
    assert_eq!(f.store.list_runs(f.owner).await.unwrap()["items"].as_array().unwrap().len(),1);
}

#[tokio::test]
async fn t49_rust_media_to_view_only_transition_fences_run_until_confirmed() {
    let f=Fixture::new().await;
    let phone=f.phone().await;
    let script=f.create("viewer trial",json!([valid_step()])).await;
    let run=json!({
        "request_id":Uuid::new_v4().to_string(),
        "script_id":script["id"],"revision":script["revision"],
        "device_ids":[phone],"parameters":{},"concurrency":1,
    });
    let session=f.store.create_session(f.owner,f.owner,
        run["device_ids"][0].as_str().unwrap(),"wall").await.unwrap();
    let session_id=session["id"].as_str().unwrap();
    f.store.update_session(session_id,"active").await.unwrap();

    // A deliberately in-progress permission change remains fail-closed;
    // stable active and view_only sessions are independently admissible.
    f.store.begin_session_permission(f.owner,session_id,"view_only").await.unwrap();
    assert_eq!(f.store.session(f.owner,session_id).await.unwrap()["state"],"suspending");
    let rejected=f.store.create_run(f.owner,f.owner,&run).await.unwrap_err();
    assert_eq!(rejected.status,409,"unconfirmed downgrade MUST NOT grant a write lease");

    f.store.confirm_session_permission(f.owner,session_id,"view_only").await.unwrap();
    assert_eq!(f.store.session(f.owner,session_id).await.unwrap()["state"],"view_only");
    let (created,new)=f.store.create_run(f.owner,f.owner,&run).await.unwrap();
    assert!(new);
    assert_eq!(created["state"],"dispatch_pending");
    assert_eq!(f.store.session(f.owner,session_id).await.unwrap()["state"],"view_only",
        "the original media record must survive the run admission");
}

#[tokio::test]
async fn run_accepts_stable_same_actor_manual_viewer_without_changing_its_switch() {
    let f=Fixture::new().await;
    let phone=f.phone().await;
    let session=f.store.create_session(f.owner,f.owner,&phone,"wall").await.unwrap();
    let sid=session["id"].as_str().unwrap();
    f.store.update_session(sid,"active").await.unwrap();
    let body=json!({
        "request_id":Uuid::new_v4().to_string(),
        "name":"Active manual viewer test","steps":[valid_step()],
        "device_ids":[phone],"parameters":{},"concurrency":1
    });
    let (run,created)=f.store.create_run(f.owner,f.owner,&body).await.unwrap();
    assert!(created,"manual ON no longer blocks the scripted run admission");
    assert_eq!(run["state"],"dispatch_pending");
    assert_eq!(f.store.session(f.owner,sid).await.unwrap()["state"],"active",
        "submitting a script must not silently change the floating hand-control switch");

    let blocked=f.store.create_run(f.owner,f.owner,&json!({
        "request_id":Uuid::new_v4().to_string(),
        "name":"Second run","steps":[valid_step()],
        "device_ids":[phone],"parameters":{},"concurrency":1
    })).await.unwrap_err();
    assert_eq!(blocked.code,"device_busy",
        "allowing the viewer must not permit overlapping script action leases");
}

#[tokio::test]
async fn t49_rust_run_dispatch_fences_manual_restore_and_new_media_admission() {
    let f=Fixture::new().await;
    let phone=f.phone().await;
    let session=f.store.create_session(f.owner,f.owner,&phone,"wall").await.unwrap();
    let sid=session["id"].as_str().unwrap();
    f.store.update_session(sid,"active").await.unwrap();
    f.store.begin_session_permission(f.owner,sid,"view_only").await.unwrap();
    f.store.confirm_session_permission(f.owner,sid,"view_only").await.unwrap();

    let script=f.create("hold write",json!([valid_step()])).await;
    let (run,created)=f.store.create_run(f.owner,f.owner,&json!({
        "request_id":Uuid::new_v4().to_string(),
        "script_id":script["id"],"revision":1,
        "device_ids":[phone],"parameters":{},"concurrency":1
    })).await.unwrap();
    assert!(created);

    let before=f.store.begin_session_permission(f.owner,sid,"active").await.unwrap_err();
    assert_eq!(before.status,409);
    assert_eq!(before.code,"device_busy");
    assert_eq!(f.store.session(f.owner,sid).await.unwrap()["state"],"view_only");

    // Closing only the viewer is NOT proof the outstanding executor action
    // was cleaned up. The DB run lease still fences a new media session.
    f.store.update_session(sid,"closed").await.unwrap();
    let denied=f.store.create_session(f.owner,f.owner,&phone,"wall").await.unwrap_err();
    assert_eq!(denied.status,409);
    assert_eq!(denied.code,"device_busy");
    assert_eq!(f.store.run(f.owner,run["id"].as_str().unwrap()).await.unwrap()["state"],
        "dispatch_pending");
}
 
#[tokio::test]
async fn emergency_cleanup_releases_historical_device_id_still_holding_the_same_adb_port() {
    let f=Fixture::new().await;
    let phone=f.phone().await;
    let target=f.store.target(f.owner,&phone).await.unwrap();
    let key=super::target_key(&target).unwrap();
    let (run,created)=f.store.create_run(f.owner,f.owner,&json!({
        "request_id":Uuid::new_v4().to_string(),
        "name":"stale ADB endpoint lease","steps":[valid_step()],
        "device_ids":[phone],"parameters":{},"concurrency":1
    })).await.unwrap();
    assert!(created);
    let run_id=run["id"].as_str().unwrap();
    // A historical device identity was replaced but its immutable run target
    // retained the same IP:port. Old cleanup selected only the *new* ID.
    let old_device=Uuid::new_v4().to_string();
    sqlx::query("UPDATE automation_run_devices SET device_id=? WHERE run_id=?")
        .bind(&old_device).bind(run_id).execute(&f.db.inner()).await.unwrap();
    sqlx::query("UPDATE automation_runs SET runner_identity=? WHERE id=?")
        .bind(Uuid::new_v4().to_string()).bind(run_id)
        .execute(&f.db.inner()).await.unwrap();

    let denied=f.store.create_session(f.owner,f.owner,&phone,"wall").await.unwrap_err();
    assert_eq!(denied.code,"device_busy");
    let event=Uuid::new_v4().to_string();
    let fenced=f.store.withdraw_endpoint_runs(f.owner,&phone,&key,&event).await.unwrap();
    assert_eq!(fenced,1,"endpoint cleanup must include old device IDs");
    let settled=f.store.confirm_endpoint_cleanup(f.owner,&phone,&key).await.unwrap();
    assert_eq!(settled["admission_clear"],true);
    assert_eq!(settled["released_runs"],1);
    assert_eq!(settled["remaining_runs"],0);
    assert_eq!(settled["remaining_sessions"],0);
    assert_eq!(f.store.run(f.owner,run_id).await.unwrap()["devices"][0]["cleanup_confirmed"],true);
    let preview=f.store.create_session(f.owner,f.owner,&phone,"wall").await.unwrap();
    assert_eq!(preview["device_id"],phone);
}

#[tokio::test]
async fn t49_saved_run_rejects_stale_revision_before_persisting_any_action() {
    let f=Fixture::new().await;
    let phone=f.phone().await;
    let saved=f.create("script v1",json!([valid_step()])).await;
    let id=saved["id"].as_str().unwrap();
    let newer=f.store.save_script(f.owner,Some(id),&json!({
        "name":"script v2","description":"updated elsewhere",
        "steps":saved["steps"],"revision":1,
    })).await.unwrap();
    assert_eq!(newer["revision"],2);

    let old=json!({
        "request_id":Uuid::new_v4().to_string(),
        "script_id":id,"revision":1,
        "device_ids":[phone],"parameters":{},"concurrency":1,
    });
    let stale=f.store.create_run(f.owner,f.owner,&old).await.unwrap_err();
    assert_eq!(stale.status,409);
    assert_eq!(stale.code,"script_revision_changed");
    assert_eq!(f.store.list_runs(f.owner).await.unwrap()["items"],json!([]),
        "a stale formal revision must never leave a dispatch_pending run");

    let mut latest=old;
    latest["revision"]=json!(2);
    latest["request_id"]=json!(Uuid::new_v4().to_string());
    let (admitted,created)=f.store.create_run(f.owner,f.owner,&latest).await.unwrap();
    assert!(created);
    assert_eq!(admitted["script_snapshot"]["revision"],2);
    assert_eq!(admitted["state"],"dispatch_pending");
}


#[tokio::test]
async fn review_create_uuid_replay_returns_same_formal_script_and_refuses_different_body() {
    let f=Fixture::new().await;
    let request=Uuid::new_v4().to_string();
    let step=valid_step();
    let body=json!({"request_id":request.clone(),"name":"only once",
        "description":"create idempotency regression","steps":[step]});
    let created=f.store.save_script(f.owner,None,&body).await.unwrap();
    let replay=f.store.save_script(f.owner,None,&body).await.unwrap();
    assert_eq!(created,replay,"same UUID and payload must return same durable response");
    assert_eq!(f.store.list_scripts(f.owner).await.unwrap()["items"].as_array().unwrap().len(),1);

    let different=json!({"request_id":request.clone(),"name":"should be refused",
        "description":"create idempotency regression","steps":body["steps"]});
    let conflict=f.store.save_script(f.owner,None,&different).await.unwrap_err();
    assert_eq!(conflict.status,409);
    assert_eq!(conflict.code,"idempotency_conflict");

    let foreign=f.store.script_creation_receipt(f.other,&request).await;
    assert_eq!(status(foreign),404,"another owner cannot fetch this create receipt");
    let recovered=f.store.script_creation_receipt(f.owner,&request).await.unwrap();
    assert_eq!(recovered,created);

    // Delete never removes the UUID tombstone. A later retry cannot recreate.
    f.store.delete_script_if_revision(f.owner,created["id"].as_str().unwrap(),1).await.unwrap();
    let deleted_replay=f.store.save_script(f.owner,None,&body).await.unwrap();
    assert_eq!(deleted_replay,created,"do not recreate a script deleted after the original POST");
    assert_eq!(f.store.list_scripts(f.owner).await.unwrap()["items"],json!([]));
}
