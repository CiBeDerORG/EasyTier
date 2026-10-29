use std::sync::Arc;

use axum::{Extension, Json, extract::{Query, rejection::{JsonRejection, QueryRejection}}, http::{HeaderMap, StatusCode}};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{central_network::service::CentralNetworkService, db::{assets::AssetError, automation::AutomationStore}};
use futures::StreamExt;
use super::{AutomationRuntime, super::{AppStateInner, ConsoleInfoConfig, users::AuthSession}};

pub(crate) type Service = Extension<Arc<CentralNetworkService>>;
pub(crate) type Config = Extension<ConsoleInfoConfig>;
pub(crate) type Runtime = Extension<Arc<AutomationRuntime>>;
pub(crate) type HttpError = (StatusCode, Json<Value>);
pub(crate) type HttpResult<T> = Result<T, HttpError>;
pub(crate) type Body = Result<Json<Value>, JsonRejection>;
pub(crate) type OwnerInput = Result<Query<Owner>, QueryRejection>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner {
    pub(crate) owner_user_id: i32,
}

pub(crate) fn error(status: StatusCode, code: &str, message: &str) -> HttpError {
    (status, Json(json!({"code":code,"message":message})))
}

pub(crate) fn store_error(cause: AssetError) -> HttpError {
    let status=StatusCode::from_u16(cause.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut payload=json!({"code":cause.code,"message":cause.message,"field_errors":cause.field_errors});
    if let Some(details)=cause.details {
        payload["diagnostics"]=details;
    }
    (status,Json(payload))
}

pub(crate) fn store(service: &CentralNetworkService) -> AutomationStore {
    AutomationStore::new(service.db.clone())
}

/// Called only after checking actor/owner scope. Query registered asset nodes
/// for this request; never scan other accounts or start a background probe.
pub(crate) async fn observed_store(manager: &AppStateInner, service: &CentralNetworkService, owner: i32, device_ids: Option<&[String]>) -> HttpResult<AutomationStore> {
    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new("SELECT DISTINCT machine_id FROM asset_devices WHERE owner_user_id=");
    query.push_bind(owner);
    if let Some(ids) = device_ids {
        if ids.is_empty() { return Ok(store(service)); }
        if ids.len()>20 {return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"too_many_devices","一次最多操作20台设备。"));}
        query.push(" AND id IN (");
        let mut separated = query.separated(",");
        for device_id in ids { separated.push_bind(device_id); }
        separated.push_unseparated(")");
    }
    let nodes = query.build_query_scalar::<String>().fetch_all(&service.db.inner()).await
        .map_err(|cause|store_error(cause.into()))?;
    let observations = futures::stream::iter(nodes).map(|node|async move {
        let values=super::super::assets::runtime::observe(manager,&service.db,owner,&node).await;
        (node,values)
    }).buffer_unordered(3).collect::<Vec<_>>().await;
    let mut store=store(service);
    for (node,values) in observations {
        store=store.with_runtime_observations(owner,&node,values);
    }
    Ok(store)
}

pub(crate) fn available(config: &ConsoleInfoConfig) -> HttpResult<()> {
    if config.webhook_auth {
        return Err(error(StatusCode::FORBIDDEN,"automation_unavailable","外部认证模式暂不支持自动化中心。"));
    }
    Ok(())
}

pub(crate) fn body(input: Body) -> HttpResult<Value> {
    let Json(value) = input.map_err(|_| error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_json","请提交有效的 JSON 请求。"))?;
    if !value.is_object() {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_json","请求必须是对象。"));
    }
    Ok(value)
}

pub(crate) fn owner(input: OwnerInput) -> HttpResult<i32> {
    let Query(value) = input.map_err(|_| error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_query","请选择有效的账号。"))?;
    if value.owner_user_id <= 0 {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_owner","请选择有效的账号。"));
    }
    Ok(value.owner_user_id)
}

pub(crate) fn id(value: &str) -> HttpResult<String> {
    Uuid::parse_str(value).map(|id| id.to_string())
        .map_err(|_| error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_id","资源标识无效。"))
}

pub(crate) fn exact_fields(value: &Value, allowed: &[&str]) -> HttpResult<()> {
    if value.as_object().is_none_or(|object| object.keys().any(|key| !allowed.contains(&key.as_str()))) {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_fields","请求包含不支持的字段。"));
    }
    Ok(())
}

pub(crate) fn actor(auth: &AuthSession) -> HttpResult<i32> {
    auth.user.as_ref().map(|user| user.db_user.id)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED,"authentication_required","请先登录。"))
}

pub(crate) async fn authorize(auth: &AuthSession, config: &ConsoleInfoConfig, service: &CentralNetworkService, owner: i32) -> HttpResult<i32> {
    available(config)?;
    let actor = actor(auth)?;
    store(service).authorize(actor, owner).await.map_err(store_error)?;
    Ok(actor)
}

pub(crate) fn require_origin(headers: &HeaderMap, required: bool) -> HttpResult<()> {
    let reject = || error(StatusCode::FORBIDDEN,"origin_denied","当前请求来源不受信任。请从控制台页面操作。");
    let Some(origin) = headers.get(axum::http::header::ORIGIN) else {
        if required || headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("cross-site") {
            return Err(reject());
        }
        return Ok(());
    };
    let origin = origin.to_str().ok().and_then(|value| url::Url::parse(value).ok()).ok_or_else(reject)?;
    if !matches!(origin.scheme(), "http" | "https") || origin.username() != "" || origin.password().is_some() {
        return Err(reject());
    }
    let host = headers.get(axum::http::header::HOST).and_then(|value| value.to_str().ok()).ok_or_else(reject)?;
    let expected = url::Url::parse(&format!("{}://{host}", origin.scheme())).map_err(|_| reject())?;
    if origin.host_str() != expected.host_str() || origin.port_or_known_default() != expected.port_or_known_default() {
        return Err(reject());
    }
    Ok(())
}

pub(crate) fn runner(runtime: &AutomationRuntime) -> HttpResult<&super::runner::Runner> {
    runtime.runner.as_ref().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE,"runner_not_configured","尚未配置手机执行器，请由管理员配置执行器地址和访问凭据。"))
}

pub(crate) fn public_session(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.remove("target");
        object.remove("actor_user_id");
        object.remove("owner_user_id");
    }
    value
}

pub(crate) fn wire_target(value: &Value) -> Value {
    let mut target = serde_json::Map::new();
    for field in ["device_id","name","owner_user_id","machine_id","ip","adb_port","connection_mode","target_fingerprint","route_context","adb_service_id","endpoint_revision"] {
        if let Some(value) = value.get(field) { target.insert(field.to_owned(), value.clone()); }
    }
    Value::Object(target)
}
