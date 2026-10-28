//! T02-07 DB-level discovery regression fixtures. No Docker/Agent process,
//! SSH or real FnOS device is required for these deterministic tests.
use crate::{db::Db, docker_absence::reconcile_and_cache};
use serde_json::{Value, json};

async fn seed_server(db: &Db, username: &str, node: &str, device: &str) -> i64 {
    let pool = db.inner();
    let owner = sqlx::query("INSERT INTO users(username,password) VALUES(?,?)")
        .bind(username).bind("regression-only-hash")
        .execute(&pool).await.unwrap().last_insert_rowid();
    sqlx::query("INSERT INTO devices(user_id,machine_id,first_seen_time,last_seen_time) VALUES(?,?,?,?)")
        .bind(owner).bind(node).bind("2026-10-28").bind("2026-10-28")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO asset_devices(id,owner_user_id,machine_id,name,overlay_ip,created_at,updated_at) VALUES(?,?,?,?,?,?,?)"
    )
        .bind(device).bind(owner).bind(node).bind(username).bind("10.10.10.10")
        .bind("2026-10-28").bind("2026-10-28")
        .execute(&pool).await.unwrap();
    owner
}

fn docker_resource(id: &str, status: &str, containers: &[(&str, &str)]) -> Value {
    json!({
        "resource_id":id,
        "kind":if id.starts_with("compose:") {"compose_project"} else {"container"},
        "name":id,
        "state":status,
        "managed":false,
        "containers":containers.iter().map(|(id, state)| json!({
            "container_id":id,"name":id,"image":"fixture:latest","state":state,
            "ports":[]
        })).collect::<Vec<_>>(),
        "ports":[]
    })
}

fn snapshot(agent: &str, observed: &str, list: Vec<Value>) -> Value {
    json!({"agent_id":agent,"observed_at":observed,"complete":true,
        "status":"ok","resources":list})
}

#[tokio::test]
async fn t0207_multi_compose_standalone_and_stopped_rescans_do_not_duplicate_records() {
    let db = Db::memory_db().await;
    let node = "10000000-0000-4000-8000-000000000001";
    let device = "10000000-0000-4000-8000-000000000002";
    let owner = seed_server(&db,"discovery-single-host",node,device).await;
    let pool = db.inner();
    let web = docker_resource("compose:web","running",&[("web-1","running")]);
    let sub2api = docker_resource("compose:sub2api","partial", &[
        ("api-1","running"),("postgres-1","exited"),("redis-1","running")
    ]);
    let portainer = docker_resource("compose:portainer","stopped",&[("portainer-1","exited")]);
    let standalone = docker_resource("container:redis-alone","stopped",&[("redis-alone","exited")]);
    let inventory = snapshot("agent-1","2026-10-28T12:00:00Z",
        vec![web.clone(),sub2api.clone(),portainer.clone(),standalone.clone()]);
    for _ in 0..4 {
        let evidence = reconcile_and_cache(&db,device,"agent-1",&inventory).await.unwrap();
        assert!(evidence.is_empty(),"still-present stopped/Compose resources must not disappear");
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM asset_agent_docker_snapshots WHERE asset_device_id=?"
    ).bind(device).fetch_one(&pool).await.unwrap();
    assert_eq!(count,1,"repeated scans must upsert one host snapshot");
    let raw: String = sqlx::query_scalar(
        "SELECT snapshot_json FROM asset_agent_docker_snapshots WHERE asset_device_id=?"
    ).bind(device).fetch_one(&pool).await.unwrap();
    let cached: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(cached["resources"].as_array().unwrap().len(),4);
    assert_eq!(cached["resources"][1]["containers"].as_array().unwrap().len(),3);
    assert_eq!(cached["resources"][2]["state"],"stopped");
    let missing_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM asset_agent_docker_absences WHERE asset_device_id=?"
    ).bind(device).fetch_one(&pool).await.unwrap();
    assert_eq!(missing_count,0);

    // A user-created service remains untouched even after two authoritative
    // scans indicate that a different Docker resource vanished.
    let service = "10000000-0000-4000-8000-000000000003";
    sqlx::query("INSERT INTO asset_services(id,owner_user_id,machine_id,device_id,name,port,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(service).bind(owner).bind(node).bind(device).bind("manual web").bind(8080)
        .bind("2026-10-28").bind("2026-10-28")
        .execute(&pool).await.unwrap();
    let reduced = snapshot("agent-1","2026-10-28T12:10:00Z",
        vec![sub2api.clone(),portainer.clone(),standalone.clone()]);
    let first = reconcile_and_cache(&db,device,"agent-1",&reduced).await.unwrap();
    let second = reconcile_and_cache(&db,device,"agent-1",&reduced).await.unwrap();
    assert_eq!(first.len(),1);
    assert_eq!(first[0]["state"],"needs_recheck");
    assert_eq!(second[0]["state"],"confirmed_missing");
    assert_eq!(second[0]["resource_id"],"compose:web");
    assert_eq!(second[0]["cleanup_allowed"],false);
    let service_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM asset_services WHERE id=? AND device_id=?"
    ).bind(service).bind(device).fetch_one(&pool).await.unwrap();
    assert_eq!(service_count,1,"missing Docker project must not delete a manual service");
    let recovered = reconcile_and_cache(&db,device,"agent-1",&inventory).await.unwrap();
    assert!(recovered.is_empty(),"restoration must cancel outstanding absence evidence");
}

#[tokio::test]
async fn t0207_invalid_scans_leave_last_complete_snapshot_unchanged() {
    let db = Db::memory_db().await;
    let node = "20000000-0000-4000-8000-000000000001";
    let device = "20000000-0000-4000-8000-000000000002";
    seed_server(&db,"discovery-failed-scan",node,device).await;
    let pool = db.inner();
    let baseline = snapshot("agent-2","2026-10-28T10:00:00Z",
        vec![docker_resource("compose:sub2api","stopped",&[("db-1","exited")])]);
    assert!(reconcile_and_cache(&db,device,"agent-2",&baseline).await.unwrap().is_empty());
    let mut invalid = snapshot("agent-2","2026-10-28T10:10:00Z",vec![]);
    invalid["complete"] = json!(false);
    assert!(reconcile_and_cache(&db,device,"agent-2",&invalid).await.is_err());
    invalid["complete"] = json!(true);
    invalid["status"] = json!("error");
    assert!(reconcile_and_cache(&db,device,"agent-2",&invalid).await.is_err());
    invalid["status"] = json!("ok");
    invalid["agent_id"] = json!("wrong-agent");
    assert!(reconcile_and_cache(&db,device,"agent-2",&invalid).await.is_err());
    let repeated = snapshot("agent-2","2026-10-28T10:10:00Z",vec![
        docker_resource("container:same","running",&[("id-1","running")]),
        docker_resource("container:same","running",&[("id-1","running")])
    ]);
    assert!(reconcile_and_cache(&db,device,"agent-2",&repeated).await.is_err());
    let raw: String = sqlx::query_scalar(
        "SELECT snapshot_json FROM asset_agent_docker_snapshots WHERE asset_device_id=?"
    ).bind(device).fetch_one(&pool).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(),baseline);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM asset_agent_docker_absences WHERE asset_device_id=?"
    ).bind(device).fetch_one(&pool).await.unwrap();
    assert_eq!(count,0);
}

#[tokio::test]
async fn t0207_same_compose_names_and_adoption_intents_are_per_device() {
    let db = Db::memory_db().await;
    let node_a = "30000000-0000-4000-8000-000000000001";
    let device_a = "30000000-0000-4000-8000-000000000002";
    let node_b = "40000000-0000-4000-8000-000000000001";
    let device_b = "40000000-0000-4000-8000-000000000002";
    let user_a = seed_server(&db,"discovery-owner-a",node_a,device_a).await;
    let user_b = seed_server(&db,"discovery-owner-b",node_b,device_b).await;
    let pool = db.inner();
    let first = snapshot("agent-a","2026-10-28T13:00:00Z",
        vec![docker_resource("compose:sub2api","running",&[("host-a-web","running")])]);
    let second = snapshot("agent-b","2026-10-28T13:00:00Z",
        vec![docker_resource("compose:sub2api","stopped",&[("host-b-web","exited")])]);
    assert!(reconcile_and_cache(&db,device_a,"agent-a",&first).await.unwrap().is_empty());
    assert!(reconcile_and_cache(&db,device_b,"agent-b",&second).await.unwrap().is_empty());

    let adopt = |device:&str, owner:i64, agent:&str, resource:&str, app:&str| {
        let db = db.clone();
        let device = device.to_string();
        let agent = agent.to_string();
        let resource = resource.to_string();
        let app = app.to_string();
        async move {
            sqlx::query("INSERT INTO asset_agent_adoption_intents(asset_device_id,owner_user_id,agent_id,resource_id,application_id,container_ids_json,confirmed_at) VALUES(?,?,?,?,?,?,?)")
                .bind(device).bind(owner).bind(agent).bind(resource).bind(app)
                .bind("[\"web\"]").bind("2026-10-28T13:10:00Z")
                .execute(&db.inner()).await
        }
    };
    adopt(device_a,user_a,"agent-a","compose:sub2api","sub2api").await.unwrap();
    adopt(device_b,user_b,"agent-b","compose:sub2api","sub2api").await.unwrap();
    assert!(adopt(device_a,user_a,"agent-a","compose:sub2api","different").await.is_err(),
        "same host/resource cannot attach a second application");
    assert!(adopt(device_a,user_a,"agent-a","compose:other","sub2api").await.is_err(),
        "same host/app cannot attach to a second resource");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_agent_adoption_intents")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count,2,"two independent hosts must keep separate adoption records");

    let removed = snapshot("agent-a","2026-10-28T13:20:00Z",vec![]);
    let only_a = reconcile_and_cache(&db,device_a,"agent-a",&removed).await.unwrap();
    assert_eq!(only_a.len(),1);
    let cached_b: String = sqlx::query_scalar(
        "SELECT snapshot_json FROM asset_agent_docker_snapshots WHERE asset_device_id=?"
    ).bind(device_b).fetch_one(&pool).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&cached_b).unwrap(),second);
    let intents_b: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM asset_agent_adoption_intents WHERE asset_device_id=? AND agent_id=?"
    ).bind(device_b).bind("agent-b").fetch_one(&pool).await.unwrap();
    assert_eq!(intents_b,1,"other host's adoption intent must survive a disappearance");
}
