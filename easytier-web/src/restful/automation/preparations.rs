//! Per-phone UiAutomator preparation: scoped, persistent, and never runs on a GET.
use axum::{extract::{Path, State}, http::{HeaderMap, StatusCode}, Extension, Json};
use serde_json::{json, Value};
use super::{super::users::AuthSession, common::*};
use super::super::{AppStateInner, ConsoleInfoConfig};
use super::AutomationRuntime;
use std::sync::Arc;
use crate::central_network::service::CentralNetworkService;

async fn observed(
    auth: &AuthSession, manager: &AppStateInner, service: &CentralNetworkService,
    config: &ConsoleInfoConfig, owner: i32, did: &str,
) -> HttpResult<Value> {
    authorize(auth, config, service, owner).await?;
    observed_store(manager, service, owner, Some(&[did.to_owned()]))
        .await?.target(owner, did).await.map_err(store_error)
}

fn guard(result: &Value, did: &str, owner: i32) -> HttpResult<()> {
    if result["device_id"] != did || result["owner_user_id"] != owner {
        return Err(error(StatusCode::NOT_FOUND, "preparation_not_found", "初始化记录不存在"));
    }
    Ok(())
}

pub(super) async fn start(
    auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service,
    Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput,
    Path(raw_id): Path<String>, headers: HeaderMap, input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let did = id(&raw_id)?;
    let body = body(input)?;
    exact_fields(&body, &["request_id"])?;
    let request_id = id(body["request_id"].as_str().unwrap_or_default())?;
    let target = observed(&auth, &manager, &service, &config, owner, &did).await?;
    let result = runner(&runtime)?.post("preparations", &json!({
        "request_id": request_id, "target": wire_target(&target),
    })).await?;
    guard(&result, &did, owner)?;
    Ok(Json(result))
}

pub(super) async fn status(
    auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service,
    Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput,
    Path(raw_id): Path<String>, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let did = id(&raw_id)?;
    let target = observed(&auth, &manager, &service, &config, owner, &did).await?;
    let result = runner(&runtime)?.post("automation/status", &json!({"target":wire_target(&target)})).await?;
    if result["device_id"] != did {
        return Err(error(StatusCode::BAD_GATEWAY, "runner_response_invalid", "组件状态与设备不匹配"));
    }
    Ok(Json(result))
}

/// Read-only, explicitly requested UI hierarchy for a single authorized asset.
pub(super) async fn hierarchy(
    auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service,
    Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput,
    Path(raw_id): Path<String>, headers: HeaderMap,
) -> HttpResult<([(&'static str, &'static str); 1], Json<Value>)> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let did = id(&raw_id)?;
    let target = observed(&auth, &manager, &service, &config, owner, &did).await?;
    let result = runner(&runtime)?.post("automation/hierarchy", &json!({"target":wire_target(&target)})).await?;
    let snapshot = &result["snapshot"];
    if result["device_id"] != did || result["target_fingerprint"] != target["target_fingerprint"]
        || snapshot["width"].as_u64().is_none_or(|value| value == 0)
        || snapshot["height"].as_u64().is_none_or(|value| value == 0)
        || snapshot["nodes"].as_array().is_none()
    {
        return Err(error(StatusCode::BAD_GATEWAY, "runner_response_invalid", "控件树与当前设备不匹配"));
    }
    Ok(([("Cache-Control", "no-store")], Json(result)))
}

pub(super) async fn adb_status(
    auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service,
    Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput,
    Path(raw_id): Path<String>, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let did = id(&raw_id)?;
    let target = observed(&auth, &manager, &service, &config, owner, &did).await?;
    // Single target, verified against this account's currently registered ADB service.
    let result = runner(&runtime)?.post("adb/status", &json!({"target":wire_target(&target)})).await?;
    if result["device_id"] != did || !matches!(result["state"].as_str(),
        Some("connected" | "offline" | "unauthorized" | "disconnected" | "unavailable")) {
        return Err(error(StatusCode::BAD_GATEWAY, "runner_response_invalid", "ADB 状态与目标设备不匹配"));
    }
    Ok(Json(result))
}

pub(super) async fn history(
    auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service,
    Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput,
    Path(raw_id): Path<String>, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let did = id(&raw_id)?;
    observed(&auth, &manager, &service, &config, owner, &did).await?;
    let result = runner(&runtime)?.get(&format!("preparations/device/{did}?owner={owner}")).await?;
    let items = result["items"].as_array().ok_or_else(||error(StatusCode::BAD_GATEWAY,"runner_response_invalid","历史记录格式无效"))?;
    for item in items { guard(item, &did, owner)?; }
    Ok(Json(result))
}

async fn scoped_record(
    auth: &AuthSession, manager: &AppStateInner, service: &CentralNetworkService,
    config: &ConsoleInfoConfig, runtime: &AutomationRuntime, owner: i32, did: &str, pid: &str,
) -> HttpResult<Value> {
    observed(auth, manager, service, config, owner, did).await?;
    let result = runner(runtime)?.get(&format!("preparations/{pid}")).await?;
    guard(&result, did, owner)?;
    Ok(result)
}

pub(super) async fn detail(
    auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service,
    Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput,
    Path((raw_id,raw_pid)): Path<(String,String)>, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?; let did = id(&raw_id)?; let pid = id(&raw_pid)?;
    Ok(Json(scoped_record(&auth,&manager,&service,&config,&runtime,owner,&did,&pid).await?))
}

pub(super) async fn logs(
    auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service,
    Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput,
    Path((raw_id,raw_pid)): Path<(String,String)>, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?; let did = id(&raw_id)?; let pid = id(&raw_pid)?;
    scoped_record(&auth,&manager,&service,&config,&runtime,owner,&did,&pid).await?;
    Ok(Json(runner(&runtime)?.get(&format!("preparations/{pid}/logs")).await?))
}

pub(super) async fn cancel(
    auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service,
    Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput,
    Path((raw_id,raw_pid)): Path<(String,String)>, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?; let did = id(&raw_id)?; let pid = id(&raw_pid)?;
    scoped_record(&auth,&manager,&service,&config,&runtime,owner,&did,&pid).await?;
    let result = runner(&runtime)?.post(&format!("preparations/{pid}/cancel"), &json!({})).await?;
    guard(&result, &did, owner)?;
    Ok(Json(result))
}
