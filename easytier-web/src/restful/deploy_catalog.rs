//! Session-authenticated read-only Deploy store index. Only admins can
//! manually refresh Git; GET never touches Git or an Agent.
use super::{AppStateInner, ConsoleInfoConfig, authed_user_id, users::AuthSession};
use crate::{central_network::service::CentralNetworkService, deploy_catalog::{Catalog, Error}};
use axum::{Extension, Json, Router, extract::Path, http::StatusCode, routing::{get, post}};
use serde_json::{Value, json};
use std::sync::Arc;

type Outcome = Result<Json<Value>, (StatusCode, Json<Value>)>;
type CatalogService = Extension<Arc<Catalog>>;
type Central = Extension<Arc<CentralNetworkService>>;

fn fail(status: StatusCode, code: &'static str, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"code":code,"message":message})))
}

fn available(auth: &AuthSession, config: &ConsoleInfoConfig) -> Result<i32, (StatusCode, Json<Value>)> {
    if config.webhook_auth {
        return Err(fail(StatusCode::FORBIDDEN, "catalog_unavailable",
            "Deploy store is unavailable in external webhook mode"));
    }
    authed_user_id(auth).map_err(|_| fail(StatusCode::UNAUTHORIZED, "auth_required", "Login required"))
}

fn failed(e: Error) -> (StatusCode, Json<Value>) {
    match e {
        Error::Uninitialized => fail(StatusCode::NOT_FOUND, "catalog_not_initialized", &e.to_string()),
        Error::Invalid(_) => fail(StatusCode::UNPROCESSABLE_ENTITY, "catalog_invalid", &e.to_string()),
        Error::Remote => fail(StatusCode::BAD_GATEWAY, "catalog_refresh_failed", &e.to_string()),
        Error::RevisionUnavailable => fail(StatusCode::CONFLICT, "catalog_revision_unavailable", &e.to_string()),
        Error::PackageConflict => fail(StatusCode::CONFLICT, "catalog_package_conflict", &e.to_string()),
        Error::Storage => fail(StatusCode::SERVICE_UNAVAILABLE, "catalog_storage_failed", &e.to_string()),
    }
}

async fn index(
    auth: AuthSession,
    Extension(config): Extension<ConsoleInfoConfig>,
    Extension(catalog): CatalogService,
) -> Outcome {
    available(&auth, &config)?;
    catalog.list().await.map(Json).map_err(failed)
}

async fn detail(
    auth: AuthSession,
    Extension(config): Extension<ConsoleInfoConfig>,
    Extension(catalog): CatalogService,
    Path(id): Path<String>,
) -> Outcome {
    available(&auth, &config)?;
    catalog.detail(&id).await.map(Json).map_err(failed)
}

async fn prepare_package(
    auth: AuthSession,
    Extension(config): Extension<ConsoleInfoConfig>,
    Extension(catalog): CatalogService,
    Path(id): Path<String>,
) -> Outcome {
    available(&auth, &config)?;
    // Package preparation consumes only the selected catalog revision. Form
    // values and secrets are intentionally not accepted by this endpoint.
    let package = catalog.prepare_package(&id).await.map_err(failed)?;
    Ok(Json(json!(package)))
}

async fn refresh(
    auth: AuthSession,
    Extension(config): Extension<ConsoleInfoConfig>,
    Extension(service): Central,
    Extension(catalog): CatalogService,
) -> Outcome {
    let user = available(&auth, &config)?;
    let is_admin: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users_groups ug INNER JOIN groups g ON ug.group_id=g.id WHERE ug.user_id=? AND g.name='admins')"
    ).bind(user).fetch_one(&service.db.inner()).await
        .map_err(|_| fail(StatusCode::SERVICE_UNAVAILABLE, "catalog_storage_failed", "Cannot verify administrator access"))?;
    if is_admin == 0 {
        return Err(fail(StatusCode::FORBIDDEN, "catalog_refresh_denied",
            "Only administrators may refresh the shared Deploy store"));
    }
    catalog.refresh().await.map(Json).map_err(failed)
}

pub fn router(catalog: Arc<Catalog>) -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/deploy/catalog", get(index))
        .route("/api/v1/deploy/catalog/{id}", get(detail))
        .route("/api/v1/deploy/catalog/{id}/package", post(prepare_package))
        .route("/api/v1/deploy/catalog/refresh", post(refresh))
        .layer(Extension(catalog))
}
