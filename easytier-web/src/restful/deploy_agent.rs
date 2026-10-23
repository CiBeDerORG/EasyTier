use super::{AppStateInner, HttpHandleError, authed_user_id, other_error, users::AuthSession};
use crate::deploy_agent::{AgentView, Error, NewAgent, Service, StartTask};
use axum::{
    Extension, Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post},
};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

type AgentService = Extension<Arc<Service>>;
fn convert(error: Error) -> HttpHandleError {
    let status = match &error {
        Error::Invalid(_) => StatusCode::BAD_REQUEST,
        Error::NotFound => StatusCode::NOT_FOUND,
        Error::Conflict(_) => StatusCode::CONFLICT,
        Error::Remote(_) => StatusCode::BAD_GATEWAY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(other_error(error.to_string())))
}
async fn list(
    auth: AuthSession,
    Extension(service): AgentService,
) -> Result<Json<Vec<AgentView>>, HttpHandleError> {
    service
        .list(authed_user_id(&auth)?)
        .await
        .map(Json)
        .map_err(convert)
}
async fn create(
    auth: AuthSession,
    Extension(service): AgentService,
    Json(request): Json<NewAgent>,
) -> Result<Json<Value>, HttpHandleError> {
    service
        .create(authed_user_id(&auth)?, request)
        .await
        .map(|id| Json(json!({"id":id})))
        .map_err(convert)
}
async fn remove(
    auth: AuthSession,
    Extension(service): AgentService,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, HttpHandleError> {
    service
        .delete(authed_user_id(&auth)?, &id.to_string())
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(convert)
}
async fn refresh(
    auth: AuthSession,
    Extension(service): AgentService,
    Path((id, action)): Path<(Uuid, String)>,
) -> Result<Json<Value>, HttpHandleError> {
    service
        .refresh(authed_user_id(&auth)?, &id.to_string(), &action)
        .await
        .map(Json)
        .map_err(convert)
}
async fn tasks(
    auth: AuthSession,
    Extension(service): AgentService,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Value>>, HttpHandleError> {
    service
        .tasks(authed_user_id(&auth)?, &id.to_string())
        .await
        .map(Json)
        .map_err(convert)
}
async fn start(
    auth: AuthSession,
    Extension(service): AgentService,
    Path(id): Path<Uuid>,
    Json(request): Json<StartTask>,
) -> Result<Json<Value>, HttpHandleError> {
    service
        .start(authed_user_id(&auth)?, &id.to_string(), request)
        .await
        .map(Json)
        .map_err(convert)
}
async fn task_action(
    auth: AuthSession,
    Extension(service): AgentService,
    Path((id, task, action)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<Value>, HttpHandleError> {
    if action == "resubmit" {
        return service
            .resubmit(authed_user_id(&auth)?, &id.to_string(), task)
            .await
            .map(Json)
            .map_err(convert);
    }
    if action != "refresh" && action != "fetch-logs" {
        return Err(convert(Error::NotFound));
    }
    service
        .refresh_task(
            authed_user_id(&auth)?,
            &id.to_string(),
            task,
            action == "fetch-logs",
        )
        .await
        .map(Json)
        .map_err(convert)
}
pub fn router(service: Arc<Service>) -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/deploy/agents", get(list).post(create))
        .route("/api/v1/deploy/agents/{id}", delete(remove))
        .route("/api/v1/deploy/agents/{id}/tasks", get(tasks).post(start))
        .route(
            "/api/v1/deploy/agents/{id}/tasks/{task}/{action}",
            post(task_action),
        )
        .route("/api/v1/deploy/agents/{id}/{action}", post(refresh))
        .layer(Extension(service))
}
