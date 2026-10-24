use axum::{Extension, Json, extract::{Path, State}, http::{HeaderMap, StatusCode}};
use serde_json::{Value, json};

use super::{common::*, super::{AppStateInner, users::AuthSession}};

pub(super) async fn context(auth: AuthSession, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let actor_id = actor(&auth)?;
    if !service.db.is_user_active(actor_id).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR,"automation_database_error","无法读取账号状态。"))? {
        return Err(error(StatusCode::UNAUTHORIZED,"authentication_required","请重新登录。"));
    }
    let admin = service.db.is_active_administrator(actor_id).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR,"automation_database_error","无法读取账号权限。"))?;
    Ok(Json(json!({
        "available":!config.webhook_auth,
        "runner_configured":runtime.runner.is_some(),
        "actor":{"id":actor_id,"username":auth.user.as_ref().map(|user| &user.db_user.username)},
        "can_manage_others":admin && !config.webhook_auth,
        "protocol_version":1,
    })))
}

pub(super) async fn list(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, query: OwnerInput, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    Ok(Json(observed_store(&manager,&service,owner,None).await?.list_devices(owner).await.map_err(store_error)?))
}

pub(super) async fn connection(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let id = id(&raw_id)?;
    let payload = body(input)?;
    exact_fields(&payload,&["enabled","adb_port","connection_mode"])?;
    let observed=observed_store(&manager,&service,owner,Some(&[id.clone()])).await?;
    authorize(&auth,&config,&service,owner).await?;
    Ok(Json(observed.save_connection(owner,&id,&payload).await.map_err(store_error)?))
}

pub(super) async fn prepare(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let id = id(&raw_id)?;
    let payload = body(input)?;
    exact_fields(&payload,&["action","pairing_port","code"])?;
    let action = payload["action"].as_str().filter(|action| matches!(*action,"connect"|"pair"|"inspect"|"automation"))
        .ok_or_else(|| error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_action","请选择有效的设备准备操作。"))?;
    if action == "pair" {
        if !payload["pairing_port"].as_u64().is_some_and(|port| (1..=65535).contains(&port))
            || !payload["code"].as_str().is_some_and(|code| code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_pairing","请输入手机显示的配对端口和六位配对码。"));
        }
    } else if payload.get("pairing_port").is_some() || payload.get("code").is_some() {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_pairing","本次操作不接受配对凭据。"));
    }
    let target = observed_store(&manager,&service,owner,Some(&[id.clone()])).await?.target(owner,&id).await.map_err(store_error)?;
    authorize(&auth,&config,&service,owner).await?;
    let expected_state=match action {"pair"=>"paired","automation"=>"automation_ready",_=>"connected"};
    let mut outbound = payload;
    outbound["target"] = wire_target(&target);
    let executor=runner(&runtime)?.identity().await?;
    crate::discovery::reconnect::remember_target(&service,&target,&executor).await.map_err(|e|store_error(e.into()))?;
    let result=runner(&runtime)?.post("prepare",&outbound).await?;
    if result["state"]!=expected_state {
        return Err(error(StatusCode::BAD_GATEWAY,"runner_response_invalid","设备准备结果尚未确认，请手动检查设备状态。"));
    }
    Ok(Json(result))
}
