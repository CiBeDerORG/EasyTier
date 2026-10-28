use axum::{Extension, Json, extract::{Path, State}, http::{HeaderMap, StatusCode, header}, response::{Response, IntoResponse}};
use serde_json::{Value, json};
use uuid::Uuid;
use sqlx::Row;

use super::{common::*, super::{AppStateInner, users::AuthSession}};

pub(super) async fn create(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth,&config,&service,owner).await?;
    let runner = runner(&runtime)?;
    let payload = body(input)?;
    exact_fields(&payload,&["request_id","script_id","revision","name","steps","device_ids","parameters","concurrency"])?;
    let selected=payload["device_ids"].as_array().filter(|items|!items.is_empty()&&items.len()<=20)
        .ok_or_else(||error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_devices","请选择1至20台设备。"))?
        .iter().map(|value|id(value.as_str().unwrap_or_default())).collect::<HttpResult<Vec<_>>>()?;
    let store = observed_store(&manager,&service,owner,Some(&selected)).await?;
    let (run,newly_created) = store.create_run(actor,owner,&payload).await.map_err(store_error)?;
    if !newly_created { return Ok(Json(run)); }
    let run_id = id(run["id"].as_str().unwrap_or_default())?;
    let executor_id = match runner.identity().await {
        Ok(value)=>value,
        Err(_)=>{
            // No identity is bound and no dispatch was issued. A later explicit
            // cancellation can prove locally that this task never left the API.
            store.set_run_dispatch_unknown(owner,&run_id).await.map_err(store_error)?;
            return Ok(Json(store.run(owner,&run_id).await.map_err(store_error)?));
        }
    };
    store.bind_run_executor(owner,&run_id,&executor_id).await.map_err(store_error)?;
    let mut outbound = store.dispatch_payload(owner,&run_id).await.map_err(store_error)?;
    outbound["expected_executor_id"] = json!(executor_id);
    // Runtime values are transient. The Store persists only parameter names;
    // an idempotent replay returns the existing run instead of redispatching.
    outbound["parameters"] = payload.get("parameters").filter(|value|!value.is_null()).cloned().unwrap_or_else(||json!({}));
    if let Some(targets) = outbound["targets"].as_array_mut() {
        for target in targets { *target = wire_target(target); }
    }
    match runner.post("runs",&outbound).await {
        Ok(result) => {
            match store.apply_run_result(&Uuid::new_v4().to_string(),&run_id,&result).await {
                Ok(updated)=>Ok(Json(updated)),
                Err(cause) if cause.status<500 => {
                    // An authenticated but malformed/incompatible receipt does
                    // not prove rejection. Keep this task visible and occupied.
                    store.set_run_dispatch_unknown(owner,&run_id).await.map_err(store_error)?;
                    Ok(Json(store.run(owner,&run_id).await.map_err(store_error)?))
                }
                Err(cause)=>Err(store_error(cause)),
            }
        }
        Err((status,Json(failure))) => {
            if status.is_client_error() && status != StatusCode::REQUEST_TIMEOUT {
                // A definite rejection still has a durable run and target set.
                let mut rejected = run;
                rejected["executor_id"] = json!(executor_id);
                rejected["revision"] = json!(0);
                rejected["state"] = json!("failed");
                if let Some(devices) = rejected["devices"].as_array_mut() {
                    for device in devices {
                        device["state"] = json!("failed");
                        device["error"] = json!({"code":failure["code"],"message":failure["message"]});
                    }
                }
                Ok(Json(store.apply_run_result(&Uuid::new_v4().to_string(),&run_id,&rejected).await.map_err(store_error)?))
            } else {
                store.set_run_dispatch_unknown(owner,&run_id).await.map_err(store_error)?;
                Ok(Json(store.run(owner,&run_id).await.map_err(store_error)?))
            }
        }
    }
}

pub(super) async fn list(auth: AuthSession, Extension(service): Service, Extension(config): Config, query: OwnerInput, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    Ok(Json(store(&service).list_runs(owner).await.map_err(store_error)?))
}

pub(super) async fn get_one(auth: AuthSession, Extension(service): Service, Extension(config): Config, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let run_id = id(&raw_id)?;
    Ok(Json(store(&service).run(owner,&run_id).await.map_err(store_error)?))
}

pub(super) async fn refresh(auth: AuthSession, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let run_id = id(&raw_id)?;
    let store = store(&service);
    store.run(owner,&run_id).await.map_err(store_error)?;
    // A 404 is not evidence that an earlier dispatch cannot still arrive.
    // Preserve the main record; only an explicit cancel creates a refusal fence.
    let result = runner(&runtime)?.get(&format!("runs/{run_id}")).await?;
    Ok(Json(store.apply_run_result(&Uuid::new_v4().to_string(),&run_id,&result).await.map_err(store_error)?))
}

pub(super) async fn cancel(auth: AuthSession, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let run_id = id(&raw_id)?;
    let store = store(&service);
    let pending = store.cancel_run(owner,&run_id).await.map_err(store_error)?;
    let Some(executor_id) = pending["executor_id"].as_str() else { return Ok(Json(pending)); };
    let runner = match runner(&runtime) { Ok(value) => value, Err(_) => return Ok(Json(pending)) };
    let expected = pending["devices"].as_array().map_or(0,Vec::len);
    match runner.post(&format!("runs/{run_id}/cancel"),&json!({"expected_executor_id":executor_id})).await {
        Ok(result) if result["id"]==run_id && result["executor_id"]==executor_id
            && result["fenced"]==true && result["never_started"]==true =>
            Ok(Json(store.confirm_run_never_started(owner,&run_id,executor_id).await.map_err(store_error)?)),
        Ok(result) if expected > 0 && result["devices"].as_array().is_some_and(|devices|devices.len()==expected) =>
            Ok(Json(store.apply_run_result(&Uuid::new_v4().to_string(),&run_id,&result).await.map_err(store_error)?)),
        // A cancellation tombstone without device execution details fences a
        // late request, but must not invent known action outcomes in the UI.
        _ => Ok(Json(pending)),
    }
}

/// T44: stop only one run/device. The DB fences actions BEFORE any remote
/// request. Executor already exposes a durable per-device withdraw endpoint.
pub(super) async fn cancel_device(
    auth: AuthSession, Extension(service): Service, Extension(config): Config,
    Extension(runtime): Runtime, query: OwnerInput,
    Path((raw_id,raw_device)): Path<(String,String)>, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let run_id=id(&raw_id)?;
    let device_id=id(&raw_device)?;
    let store=store(&service);
    let (pending,grant)=store.cancel_run_device(owner,&run_id,&device_id).await.map_err(store_error)?;
    let Some(grant)=grant else {return Ok(Json(pending));};
    let Some(executor_id)=grant["executor_id"].as_str() else {return Ok(Json(pending));};
    let runner=match runner(&runtime){Ok(value)=>value,Err(_)=>return Ok(Json(pending))};
    let response=runner.post(
        &format!("runs/{run_id}/devices/{device_id}/withdraw"),
        &json!({
            "event_id":grant["event_id"],"expected_executor_id":executor_id,
            "old_target_fingerprint":grant["old_target_fingerprint"],
        }),
    ).await;
    match response {
        Ok(result) if result["id"]==run_id && result["device_id"]==device_id
            && result["executor_id"]==executor_id && result["event_id"]==grant["event_id"]
            && result["old_target_fingerprint"]==grant["old_target_fingerprint"]
            && result["fenced"]==true && result["never_started"]==true
            && result["cleanup_confirmed"]==true => {
                let fingerprint=grant["old_target_fingerprint"].as_str().unwrap_or_default();
                store.confirm_device_never_started(owner,&run_id,&device_id,executor_id,fingerprint)
                    .await.map_err(store_error)?;
                Ok(Json(store.run(owner,&run_id).await.map_err(store_error)?))
            },
        Ok(result) if result["id"]==run_id
            && result["devices"].as_array().is_some_and(|devices|
                devices.len()==pending["devices"].as_array().map_or(0,Vec::len)
                && devices.iter().any(|device|device["device_id"]==device_id)) => {
                // Persist only the Executor's authoritative action/cleanup
                // result. Another device remains scheduled and observable.
                Ok(Json(store.apply_run_result(
                    &Uuid::new_v4().to_string(),&run_id,&result,
                ).await.map_err(store_error)?))
            },
        // Errors, unknown timeouts or partial responses cannot prove stop.
        _ => Ok(Json(pending)),
    }
}

pub(super) async fn internal_authorize(State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    available(&config)?;
    runner(&runtime)?.authenticate(&headers)?;
    let payload = body(input)?;
    exact_fields(&payload,&["run_id","device_id","target_fingerprint","executor_id"])?;
    let run_id = id(payload["run_id"].as_str().unwrap_or_default())?;
    let device_id = id(payload["device_id"].as_str().unwrap_or_default())?;
    let executor_id = id(payload["executor_id"].as_str().unwrap_or_default())?;
    let fingerprint = payload["target_fingerprint"].as_str().filter(|value| !value.is_empty() && value.len() <= 256)
        .ok_or_else(||error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_target","设备目标版本无效。"))?;
    // Resolve scope only from the accepted run. Never let an executor callback
    // choose another account or trigger observations for arbitrary devices.
    let scope=sqlx::query("SELECT r.actor_user_id,r.owner_user_id FROM automation_runs r JOIN automation_run_devices d ON d.run_id=r.id WHERE r.id=? AND d.device_id=? AND r.runner_identity=? AND r.cancel_requested=0 AND d.target_fingerprint=? AND d.state IN ('dispatch_pending','queued','accepted','starting','running')")
        .bind(&run_id).bind(&device_id).bind(&executor_id).bind(fingerprint)
        .fetch_optional(&service.db.inner()).await.map_err(|cause|store_error(cause.into()))?
        .ok_or_else(||error(StatusCode::FORBIDDEN,"grant_revoked","任务授权已失效，禁止执行后续操作。"))?;
    let actor:i32=scope.try_get("actor_user_id").map_err(|cause|store_error(cause.into()))?;
    let owner:i32=scope.try_get("owner_user_id").map_err(|cause|store_error(cause.into()))?;
    store(&service).authorize(actor,owner).await.map_err(store_error)?;
    let observed=observed_store(&manager,&service,owner,Some(&[device_id.clone()])).await?;
    if !observed.authorize_run_step(&run_id,&device_id,fingerprint,&executor_id).await.map_err(store_error)? {
        return Err(error(StatusCode::FORBIDDEN,"grant_revoked","任务授权已失效，禁止执行后续操作。"));
    }
    Ok(Json(json!({"authorized":true,"lease_seconds":60})))
}

pub(super) async fn internal_event(Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    available(&config)?;
    runner(&runtime)?.authenticate(&headers)?;
    let payload = body(input)?;
    exact_fields(&payload,&["event_id","run_id","result","executor_id"])?;
    let event_id = id(payload["event_id"].as_str().unwrap_or_default())?;
    let run_id = id(payload["run_id"].as_str().unwrap_or_default())?;
    let executor_id = id(payload["executor_id"].as_str().unwrap_or_default())?;
    if !payload["result"].is_object() || payload["result"]["id"] != run_id || payload["result"]["executor_id"]!=executor_id {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_event","执行结果与任务标识不匹配。"));
    }
    store(&service).apply_run_result(&event_id,&run_id,&payload["result"]).await.map_err(store_error)?;
    Ok(Json(json!({"accepted":true,"event_id":event_id})))
}

pub(super) async fn artifact(auth: AuthSession, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path((raw_run,raw_artifact)): Path<(String,String)>, headers: HeaderMap) -> HttpResult<Response> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let run_id = id(&raw_run)?;
    let artifact_id = id(&raw_artifact)?;
    let run = store(&service).run(owner,&run_id).await.map_err(store_error)?;
    let known = run["devices"].as_array().is_some_and(|devices|devices.iter().any(|device|
        device["steps"].as_array().is_some_and(|steps|steps.iter().any(|step|step["artifact_id"]==artifact_id))));
    if !known {
        return Err(error(StatusCode::NOT_FOUND,"artifact_not_found","该任务尚无此截图记录，请先刷新执行结果。"));
    }
    let (mime,bytes) = runner(&runtime)?.artifact(&run_id,&artifact_id).await?;
    // Recheck authorization after the potentially slow remote read.
    authorize(&auth,&config,&service,owner).await?;
    Ok(([(header::CONTENT_TYPE,mime.as_str()),(header::CACHE_CONTROL,"private, no-store"),
        (header::X_CONTENT_TYPE_OPTIONS,"nosniff"),(header::CONTENT_DISPOSITION,"inline")],bytes).into_response())
}
