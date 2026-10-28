//! T49 · AutomationStore / SQLite migration / formal script CAS regression.
//! Uses real Db::memory_db migrations, not a fabricated SQL schema.
//! No Rust test/build, executor request, ADB or Android device is run in debug mode.

use serde_json::{Value, json};
use uuid::Uuid;

use super::{AutomationStore, validation};
use crate::db::Db;

struct Fixture {
    store: AutomationStore,
    owner: i32,
    other: i32,
}

impl Fixture {
    async fn new() -> Self {
        let db = Db::memory_db().await;
        let owner = db.auto_create_user("t49-script-owner").await.unwrap().id;
        let other = db.auto_create_user("t49-script-other").await.unwrap().id;
        Self { store: AutomationStore::new(db), owner, other }
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
