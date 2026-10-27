//! Session-authenticated Agent pairing endpoints on existing server asset
//! devices. Ownership and server semantics are revalidated on every call.
use crate::{
    asset_agent::{self, Manager, PairInput, ProbeInput},
    central_network::service::CentralNetworkService,
    restful::{AppStateInner, ConsoleInfoConfig, assets::server_agent_entry, users::AuthSession},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    owner_user_id: i32,
    machine_id: String,
}
type Outcome<T> = Result<T, (StatusCode, Json<Value>)>;

fn failure(e: asset_agent::Error) -> (StatusCode, Json<Value>) {
    let (status, code) = match e {
        asset_agent::Error::Invalid(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_agent_input"),
        asset_agent::Error::Conflict(_) => (StatusCode::CONFLICT, "agent_already_bound"),
        asset_agent::Error::IdentityChanged => (StatusCode::CONFLICT, "agent_identity_mismatch"),
        asset_agent::Error::PairRejected => (StatusCode::UNPROCESSABLE_ENTITY, "pairing_code_rejected"),
        asset_agent::Error::Remote => (StatusCode::BAD_GATEWAY, "agent_tls_or_network_failure"),
        asset_agent::Error::Storage => (StatusCode::SERVICE_UNAVAILABLE, "agent_binding_storage_failed"),
    };
    (status, Json(json!({"code":code,"message":e.to_string()})))
}
async fn checked(
    auth: &AuthSession,
    config: &ConsoleInfoConfig,
    central: &CentralNetworkService,
    id: &str,
    scope: &Scope,
) -> Outcome<Value> {
    server_agent_entry::load_authorized(
        auth, config, central, scope.owner_user_id, &scope.machine_id, id,
    ).await
}

async fn status(
    auth: AuthSession,
    Extension(config): Extension<ConsoleInfoConfig>,
    Extension(central): Extension<Arc<CentralNetworkService>>,
    Extension(manager): Extension<Arc<Manager>>,
    Path(id): Path<String>,
    Query(scope): Query<Scope>,
) -> Outcome<Json<Value>> {
    let device = checked(&auth, &config, &central, &id, &scope).await?;
    manager.status(&device).await.map(Json).map_err(failure)
}

async fn probe(
    auth: AuthSession,
    Extension(config): Extension<ConsoleInfoConfig>,
    Extension(central): Extension<Arc<CentralNetworkService>>,
    Extension(manager): Extension<Arc<Manager>>,
    Path(id): Path<String>,
    Query(scope): Query<Scope>,
    Json(input): Json<ProbeInput>,
) -> Outcome<Json<Value>> {
    let device = checked(&auth, &config, &central, &id, &scope).await?;
    manager.probe(&device, input).await.map(Json).map_err(failure)
}

async fn pair(
    auth: AuthSession,
    Extension(config): Extension<ConsoleInfoConfig>,
    Extension(central): Extension<Arc<CentralNetworkService>>,
    Extension(manager): Extension<Arc<Manager>>,
    Path(id): Path<String>,
    Query(scope): Query<Scope>,
    Json(input): Json<PairInput>,
) -> Outcome<Json<Value>> {
    let device = checked(&auth, &config, &central, &id, &scope).await?;
    manager.pair(&device, input).await.map(Json).map_err(failure)
}

pub fn router(manager: Arc<Manager>) -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/assets/devices/{id}/agent/status", get(status))
        .route("/api/v1/assets/devices/{id}/agent/probe", post(probe))
        .route("/api/v1/assets/devices/{id}/agent/pair", post(pair))
        .layer(Extension(manager))
}
