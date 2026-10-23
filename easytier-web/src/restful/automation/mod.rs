//! Session-authenticated automation APIs. Media never traverses these routes.
mod common;
mod devices;
mod runner;
mod runs;
mod scripts;
mod sessions;

use std::{collections::HashSet, sync::Arc};

use axum::{Extension, Router, extract::DefaultBodyLimit, routing::{get, post, put}};
use tokio::sync::Mutex;
use futures::StreamExt;
use serde_json::{Value, json};
use sqlx::Row;
use crate::central_network::service::CentralNetworkService;

use super::AppStateInner;

pub(super) struct AutomationRuntime {
    runner: Option<runner::Runner>,
    controls: Mutex<HashSet<String>>,
}

pub(crate) fn router() -> Router<AppStateInner> {
    let runtime = Arc::new(AutomationRuntime {
        runner: runner::Runner::from_environment(),
        controls: Mutex::new(HashSet::new()),
    });
    Router::new()
        .route("/api/v1/automation/context", get(devices::context))
        .route("/api/v1/automation/devices", get(devices::list))
        .route("/api/v1/automation/devices/{id}/connection", put(devices::connection))
        .route("/api/v1/automation/devices/{id}/prepare", post(devices::prepare))
        .route("/api/v1/automation/sessions", post(sessions::create))
        .route("/api/v1/automation/sessions/{id}/offer", post(sessions::offer))
        .route("/api/v1/automation/sessions/{id}/control", get(sessions::control))
        .route("/api/v1/automation/sessions/{id}/close", post(sessions::close))
        .route("/api/v1/automation/scripts", get(scripts::list).post(scripts::create))
        .route("/api/v1/automation/scripts/{id}", put(scripts::update).delete(scripts::remove))
        .route("/api/v1/automation/runs", get(runs::list).post(runs::create))
        .route("/api/v1/automation/runs/{id}", get(runs::get_one))
        .route("/api/v1/automation/runs/{id}/refresh", post(runs::refresh))
        .route("/api/v1/automation/runs/{id}/cancel", post(runs::cancel))
        .route("/api/v1/automation/runs/{id}/artifacts/{artifact_id}", get(runs::artifact))
        // These routes authenticate the configured Runner, never a browser session.
        .route("/api/v1/automation/internal/authorize", post(runs::internal_authorize))
        .route("/api/v1/automation/internal/events", post(runs::internal_event))
        .layer(DefaultBodyLimit::max(512 * 1024))
        .layer(Extension(runtime))
}

/// Best-effort notification after the account retirement transaction commits.
/// The transaction triggers, per-command checks and finite Runner grants remain
/// authoritative when this network notification cannot be delivered.
pub(super) async fn revoke_actor_or_owner(service: &CentralNetworkService, user_id: i32) -> Value {
    let runtime = AutomationRuntime { runner:runner::Runner::from_environment(),controls:Mutex::new(HashSet::new()) };
    let Some(runner) = runtime.runner.as_ref() else {
        return json!({"requested":true,"confirmed":false,"reason":"runner_not_configured"});
    };
    let sessions = sqlx::query("SELECT id FROM automation_sessions WHERE (actor_user_id=? OR owner_user_id=?) AND state NOT IN ('closed','failed')")
        .bind(user_id).bind(user_id).fetch_all(&service.db.inner()).await;
    let runs = sqlx::query("SELECT id,owner_user_id FROM automation_runs WHERE (actor_user_id=? OR owner_user_id=?) AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending')")
        .bind(user_id).bind(user_id).fetch_all(&service.db.inner()).await;
    let (Ok(sessions),Ok(runs))=(sessions,runs) else {
        return json!({"requested":true,"confirmed":false,"reason":"database_unavailable"});
    };
    let actions = sessions.into_iter().filter_map(|row|row.try_get::<String,_>("id").ok().map(|id|(id,None)))
        .chain(runs.into_iter().filter_map(|row|Some((row.try_get::<String,_>("id").ok()?,Some(row.try_get::<i32,_>("owner_user_id").ok()?)))))
        .collect::<Vec<_>>();
    let count = actions.len();
    let notifications = futures::stream::iter(actions).map(|(id,owner)| {
        let runtime=&runtime;
        async move {
            if let Some(owner)=owner {
                let store=common::store(service);
                let pending=match store.cancel_run(owner,&id).await {Ok(value)=>value,Err(_)=>return false};
                let Some(executor_id)=pending["executor_id"].as_str() else {return pending["state"]=="cancelled";};
                match runner.post(&format!("runs/{id}/cancel"),&json!({"expected_executor_id":executor_id})).await {
                    Ok(result) if result["id"]==id && result["executor_id"]==executor_id
                        && result["fenced"]==true && result["never_started"]==true =>
                        store.confirm_run_never_started(owner,&id,executor_id).await.is_ok(),
                    Ok(result) if result["devices"].is_array() => match store.apply_run_result(&uuid::Uuid::new_v4().to_string(),&id,&result).await {
                        Ok(saved)=>saved["devices"].as_array().is_some_and(|devices|!devices.is_empty()&&devices.iter().all(|device|
                            device["cleanup_confirmed"]==true || matches!(device["state"].as_str(),Some("completed"|"succeeded"|"failed"|"cancelled"|"interrupted")))),
                        Err(_)=>false,
                    },
                    _=>false,
                }
            } else {
                sessions::close_remote(service,runtime,&id).await
                    .is_ok_and(|result|result["cleanup_pending"].as_bool()==Some(false))
            }
        }
    }).buffer_unordered(3).collect::<Vec<_>>();
    match tokio::time::timeout(std::time::Duration::from_secs(30),notifications).await {
        Ok(results)=>json!({"requested":true,"resources":count,"confirmed":results.iter().all(|confirmed|*confirmed),"confirmed_resources":results.iter().filter(|confirmed|**confirmed).count()}),
        Err(_)=>json!({"requested":true,"resources":count,"confirmed":false,"reason":"notification_timeout"}),
    }
}
