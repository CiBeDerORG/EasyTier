//! One-shot post-commit processing. Every retry is tied to a durable endpoint event.
use super::Runtime;
use crate::{
    central_network::service::CentralNetworkService,
    client_manager::ClientManager,
    db::{assets::common::now, automation::AutomationStore},
    restful::automation::{
        AutomationRuntime,
        common::{observed_store, wire_target},
        sessions::close_remote,
    },
};
use serde_json::{Value, json};
use sqlx::Row;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use tokio::sync::Mutex;

pub(crate) async fn remember_target(
    service: &CentralNetworkService,
    target: &Value,
    executor: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO discovery_targets(device_id,owner_user_id,executor_id,target_fingerprint,target_json,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(device_id,executor_id,target_fingerprint) DO NOTHING")
        .bind(target["device_id"].as_str()).bind(target["owner_user_id"].as_i64()).bind(executor).bind(target["target_fingerprint"].as_str()).bind(wire_target(target).to_string()).bind(now()).execute(&service.db.inner()).await?;
    Ok(())
}
async fn stage(
    service: &CentralNetworkService,
    event: &str,
    column: &str,
    value: &Value,
) -> Result<(), sqlx::Error> {
    debug_assert!(matches!(column, "cleanup_json" | "connection_json"));
    sqlx::query(&format!(
        "UPDATE discovery_endpoint_events SET {column}=?,updated_at=? WHERE event_id=?"
    ))
    .bind(value.to_string())
    .bind(now())
    .bind(event)
    .execute(&service.db.inner())
    .await?;
    Ok(())
}
pub(crate) async fn advance(
    manager: &Arc<ClientManager>,
    service: &CentralNetworkService,
    runtime: &Runtime,
    owner: i32,
    device: &str,
    requested: Option<(&str, &str)>,
) -> Result<Value, crate::db::assets::AssetError> {
    use crate::db::assets::AssetError;
    let _configuration = runtime.settings.operations.read().await;
    let _device = runtime.lock(format!("device:{owner}:{device}")).await;
    let pool = service.db.inner();
    let row=sqlx::query("SELECT * FROM discovery_endpoint_events WHERE owner_user_id=? AND device_id=? ORDER BY seq DESC LIMIT 1").bind(owner).bind(device).fetch_optional(&pool).await?.ok_or_else(AssetError::not_found)?;
    let event: String = row.try_get("event_id")?;
    let revision: i64 = row.try_get("endpoint_revision")?;
    if requested.is_some_and(|(id, _)| id != event) {
        return Err(AssetError::conflict(
            "endpoint_event_superseded",
            "A newer endpoint event replaced this retry.",
        ));
    }
    let mut cleanup: Value = serde_json::from_str(&row.try_get::<String, _>("cleanup_json")?)
        .unwrap_or(json!({"state":"pending"}));
    let mut connection: Value = serde_json::from_str(&row.try_get::<String, _>("connection_json")?)
        .unwrap_or(json!({"state":"pending"}));
    let _=runtime.events.send(json!({"event_id":event,"owner_user_id":owner,"device_id":device,"endpoint_revision":revision,"cleanup":cleanup,"connection":connection}));
    let automation = AutomationRuntime {
        runner: runtime.settings.runner(),
        controls: Arc::new(Mutex::new(HashSet::new())),
    };
    if requested.is_some_and(|(_, stage)| stage == "connection") && cleanup["state"] != "confirmed"
    {
        return Err(AssetError::conflict(
            "cleanup_stage_required",
            "Confirm the original cleanup stage before retrying a connection.",
        ));
    }
    let store = AutomationStore::new(service.db.clone());
    // Persist the per-device fence before any network request or connection attempt.
    let withdrawals = store.withdraw_device_runs(owner, device, &event).await?;
    let sessions=sqlx::query("SELECT id,target_json FROM automation_sessions WHERE owner_user_id=? AND device_id=? AND state NOT IN ('closed','failed')").bind(owner).bind(device).fetch_all(&pool).await?;
    let old_targets:Vec<String>=sqlx::query_scalar("SELECT target_json FROM discovery_targets WHERE owner_user_id=? AND device_id=? AND retired=0 UNION SELECT d.target_json FROM automation_run_devices d JOIN automation_runs r ON r.id=d.run_id WHERE r.owner_user_id=? AND d.device_id=? AND d.withdraw_requested=1 AND d.cleanup_confirmed=0 UNION SELECT target_json FROM automation_sessions WHERE owner_user_id=? AND device_id=? AND state NOT IN ('closed','failed')").bind(owner).bind(device).bind(owner).bind(device).bind(owner).bind(device).fetch_all(&pool).await?;
    let Some(runner) = automation.runner.as_ref() else {
        cleanup = if withdrawals.is_empty() && sessions.is_empty() && old_targets.is_empty() {
            json!({"state":"confirmed","code":"no_previous_resources"})
        } else {
            json!({"state":"pending","code":"runner_not_configured"})
        };
        let adb: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM asset_services WHERE device_id=? AND service_type='adb')",
        )
        .bind(device)
        .fetch_one(&pool)
        .await?;
        connection = if adb {
            json!({"state":"pending","code":"runner_not_configured"})
        } else {
            json!({"state":"not_applicable","code":"adb_service_missing"})
        };
        stage(service, &event, "cleanup_json", &cleanup).await?;
        stage(service, &event, "connection_json", &connection).await?;
        let result = json!({"event_id":event,"owner_user_id":owner,"device_id":device,"endpoint_revision":revision,"cleanup":cleanup,"connection":connection});
        let _ = runtime.events.send(result.clone());
        return Ok(result);
    };
    let executor = runner.identity().await.map_err(|(_, v)| {
        AssetError::new(
            502,
            "runner_unreachable",
            v.0["message"].as_str().unwrap_or("Executor unavailable."),
        )
    })?;
    let old_executors:Vec<String>=sqlx::query_scalar("SELECT DISTINCT executor_id FROM discovery_targets WHERE owner_user_id=? AND device_id=? AND retired=0").bind(owner).bind(device).fetch_all(&pool).await?;
    if cleanup["state"] != "confirmed" && old_executors.iter().any(|id| id != &executor) {
        return Err(AssetError::conflict(
            "executor_identity_changed",
            "Restore the original executor to confirm cleanup of its resources.",
        ));
    }
    if cleanup["state"] != "confirmed" {
        let mut clean = true;
        for withdrawal in withdrawals {
            if withdrawal["cleanup_confirmed"] == true {
                continue;
            }
            let run = withdrawal["run_id"].as_str().unwrap();
            let expected = withdrawal["executor_id"].as_str().unwrap_or("");
            let fp = withdrawal["old_target_fingerprint"].as_str().unwrap_or("");
            if expected != executor {
                clean = false;
                continue;
            }
            match runner.post(&format!("runs/{run}/devices/{device}/withdraw"),&json!({"event_id":event,"expected_executor_id":executor,"old_target_fingerprint":fp})).await{
                Ok(v) if v["never_started"]==true&&v["fenced"]==true&&v["executor_id"]==executor&&v["device_id"]==device=>{clean&=store.confirm_device_never_started(owner,run,device,&executor,fp).await.is_ok();},
                Ok(v)=>{if let Ok(saved)=store.apply_run_result(&uuid::Uuid::new_v4().to_string(),run,&v).await{clean&=saved["devices"].as_array().is_some_and(|a|a.iter().any(|d|d["device_id"]==device&&d["cleanup_confirmed"]==true));}else{clean=false;}},
                Err(_)=>clean=false,
            }
        }
        for session in sessions {
            let sid: String = session.try_get("id")?;
            clean &= close_remote(service, &automation, &sid)
                .await
                .is_ok_and(|v| v["cleanup_pending"] == false);
        }
        let mut unique = BTreeMap::new();
        for raw in old_targets {
            if let Ok(target) = serde_json::from_str::<Value>(&raw) {
                if let Some(fp) = target["target_fingerprint"].as_str() {
                    unique.insert(fp.to_owned(), target);
                }
            }
        }
        for target in unique.into_values() {
            let v=runner.post(&format!("targets/{device}/retire"),&json!({"event_id":event,"expected_executor_id":executor,"target":wire_target(&target)})).await;
            let retired = v.is_ok_and(|v| {
                v["cleanup_confirmed"] == true
                    && v["device_id"] == device
                    && v["event_id"] == event
                    && v["executor_id"] == executor
            });
            if retired {
                sqlx::query("UPDATE discovery_targets SET retired=1 WHERE owner_user_id=? AND device_id=? AND executor_id=? AND target_fingerprint=?").bind(owner).bind(device).bind(&executor).bind(target["target_fingerprint"].as_str()).execute(&pool).await?;
            }
            clean &= retired;
        }
        cleanup = json!({"state":if clean{"confirmed"}else{"pending"},"code":if clean{"old_target_retired"}else{"cleanup_unconfirmed"}});
        stage(service, &event, "cleanup_json", &cleanup).await?;
    }
    // A replacement cannot connect until its predecessor's resources are retired.
    if let Some(predecessor) = row.try_get::<Option<String>, _>("predecessor_device_id")? {
        let predecessor_cleanup:Option<String>=sqlx::query_scalar("SELECT json_extract(cleanup_json,'$.state') FROM discovery_endpoint_events WHERE owner_user_id=? AND device_id=? ORDER BY seq DESC LIMIT 1").bind(owner).bind(&predecessor).fetch_optional(&pool).await?;
        if predecessor_cleanup.as_deref() != Some("confirmed") {
            cleanup = json!({"state":"pending","code":"predecessor_cleanup_pending","predecessor_device_id":predecessor});
            stage(service, &event, "cleanup_json", &cleanup).await?;
            let result = json!({"event_id":event,"owner_user_id":owner,"device_id":device,"endpoint_revision":revision,"cleanup":cleanup,"connection":connection});
            let _ = runtime.events.send(result.clone());
            return Ok(result);
        }
    }
    if cleanup["state"] == "confirmed"
        && connection["state"] != "connected"
        && requested.is_none_or(|(_, s)| s != "cleanup")
    {
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT endpoint_revision FROM asset_devices WHERE id=? AND owner_user_id=?",
        )
        .bind(device)
        .bind(owner)
        .fetch_optional(&pool)
        .await?;
        if current.is_none() {
            connection = json!({"state":"not_applicable","code":"device_removed"});
        } else if current != Some(revision) {
            connection = json!({"state":"superseded"});
        } else {
            match observed_store(manager, service, owner, Some(&[device.to_owned()])).await {
                Ok(store) => match store.target(owner, device).await {
                    Ok(target) => {
                        remember_target(service, &target, &executor).await?;
                        let result = runner
                            .post(
                                "prepare",
                                &json!({"action":"connect","target":wire_target(&target)}),
                            )
                            .await;
                        let latest:Option<i64>=sqlx::query_scalar("SELECT endpoint_revision FROM asset_devices WHERE id=? AND owner_user_id=?").bind(device).bind(owner).fetch_optional(&pool).await?;
                        connection = if latest != Some(revision) {
                            json!({"state":"superseded"})
                        } else {
                            match result {
                                Ok(v) if v["state"] == "connected" => {
                                    json!({"state":"connected","target_fingerprint":target["target_fingerprint"]})
                                }
                                Ok(v) => json!({"state":"pending","detail":v}),
                                Err((_, v)) => {
                                    json!({"state":"pending","code":v.0["code"],"message":v.0["message"]})
                                }
                            }
                        };
                    }
                    Err(e) => {
                        connection =
                            json!({"state":"not_applicable","code":e.code,"message":e.message})
                    }
                },
                Err((_, e)) => connection = json!({"state":"pending","code":e.0["code"]}),
            }
        }
        stage(service, &event, "connection_json", &connection).await?;
    }
    sqlx::query("UPDATE discovery_endpoint_events SET connection_json='{\"state\":\"superseded\"}',cleanup_json=CASE WHEN ? THEN ? ELSE cleanup_json END WHERE owner_user_id=? AND device_id=? AND seq<?").bind(cleanup["state"]=="confirmed").bind(cleanup.to_string()).bind(owner).bind(device).bind(row.try_get::<i64,_>("seq")?).execute(&pool).await?;
    let result = json!({"event_id":event,"owner_user_id":owner,"device_id":device,"endpoint_revision":revision,"cleanup":cleanup,"connection":connection});
    let _ = runtime.events.send(result.clone());
    Ok(result)
}
