use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
};
use uuid::Uuid;

use super::{AppStateInner, HttpHandleError, authed_user_id, other_error, users::AuthSession};
use crate::{
    central_network::service::CentralNetworkService,
    db::network_space::{NetworkMapConfig, ServiceEntry, ServiceInput, SpaceError},
};

type Service = Extension<Arc<CentralNetworkService>>;

fn convert_error(error: SpaceError) -> HttpHandleError {
    let status = match &error {
        SpaceError::Invalid(_) => StatusCode::BAD_REQUEST,
        SpaceError::NotFound(_) => StatusCode::NOT_FOUND,
        SpaceError::Conflict(_) => StatusCode::CONFLICT,
        SpaceError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(other_error(error)))
}

#[derive(serde::Deserialize)]
struct ServiceQuery {
    network_id: Option<Uuid>,
}

#[derive(serde::Serialize)]
struct ServicesResponse {
    services: Vec<ServiceEntry>,
}

async fn get_map(
    auth: AuthSession,
    Extension(service): Service,
    Path(network_id): Path<Uuid>,
) -> Result<Json<NetworkMapConfig>, HttpHandleError> {
    let user_id = authed_user_id(&auth)?;
    service
        .db
        .get_network_map(user_id, network_id)
        .await
        .map(Json)
        .map_err(convert_error)
}

async fn save_map(
    auth: AuthSession,
    Extension(service): Service,
    Path(network_id): Path<Uuid>,
    Json(config): Json<NetworkMapConfig>,
) -> Result<Json<NetworkMapConfig>, HttpHandleError> {
    let user_id = authed_user_id(&auth)?;
    let _mutation = service.lock_mutations().await;
    service
        .db
        .save_network_map(user_id, network_id, config)
        .await
        .map(Json)
        .map_err(convert_error)
}

async fn list_services(
    auth: AuthSession,
    Extension(service): Service,
    Query(query): Query<ServiceQuery>,
) -> Result<Json<ServicesResponse>, HttpHandleError> {
    let services = service
        .db
        .list_network_services(authed_user_id(&auth)?, query.network_id)
        .await
        .map_err(convert_error)?;
    Ok(Json(ServicesResponse { services }))
}

async fn create_service(
    auth: AuthSession,
    Extension(service): Service,
    Json(input): Json<ServiceInput>,
) -> Result<(StatusCode, Json<ServiceEntry>), HttpHandleError> {
    let user_id = authed_user_id(&auth)?;
    let _mutation = service.lock_mutations().await;
    service
        .db
        .write_network_service(user_id, None, input)
        .await
        .map(|entry| (StatusCode::CREATED, Json(entry)))
        .map_err(convert_error)
}

async fn update_service(
    auth: AuthSession,
    Extension(service): Service,
    Path(id): Path<Uuid>,
    Json(input): Json<ServiceInput>,
) -> Result<Json<ServiceEntry>, HttpHandleError> {
    let user_id = authed_user_id(&auth)?;
    let _mutation = service.lock_mutations().await;
    service
        .db
        .write_network_service(user_id, Some(id), input)
        .await
        .map(Json)
        .map_err(convert_error)
}

async fn delete_service(
    auth: AuthSession,
    Extension(service): Service,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, HttpHandleError> {
    let user_id = authed_user_id(&auth)?;
    let _mutation = service.lock_mutations().await;
    service
        .db
        .delete_network_service(user_id, id)
        .await
        .map_err(convert_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Merged into the authenticated, central-mode router only.
pub(super) fn router() -> Router<AppStateInner> {
    Router::new()
        .route(
            "/api/v1/networks/{network-id}/map",
            get(get_map).put(save_map),
        )
        .route("/api/v1/services", get(list_services).post(create_service))
        .route(
            "/api/v1/services/{id}",
            axum::routing::put(update_service).delete(delete_service),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_keep_validation_ownership_and_revision_distinct() {
        assert_eq!(
            convert_error(SpaceError::Invalid("invalid".into())).0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            convert_error(SpaceError::NotFound("missing".into())).0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            convert_error(SpaceError::Conflict("stale".into())).0,
            StatusCode::CONFLICT
        );
    }
}
