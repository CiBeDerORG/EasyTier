use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, rejection::PathRejection},
    http::StatusCode,
    routing::{delete, get},
};
use serde_json::{Value, json};

use crate::{central_network::service::CentralNetworkService, db::user_admin::UserAdminError};

use super::{AppStateInner, ConsoleInfoConfig, users::AuthSession};

type Service = Extension<Arc<CentralNetworkService>>;
type Config = Extension<ConsoleInfoConfig>;
type HttpError = (StatusCode, Json<Value>);
type HttpResult<T> = Result<T, HttpError>;

fn error(status: StatusCode, code: &str, message: &str) -> HttpError {
    (status, Json(json!({"code": code, "message": message})))
}

fn store_error(err: UserAdminError) -> HttpError {
    match err {
        UserAdminError::Forbidden => error(
            StatusCode::FORBIDDEN,
            "user_admin_forbidden",
            "Administrator access is required.",
        ),
        UserAdminError::NotFound => error(StatusCode::NOT_FOUND, "user_not_found", "User not found."),
        UserAdminError::DeleteSelf => error(
            StatusCode::CONFLICT,
            "user_delete_self",
            "You cannot delete your own account.",
        ),
        UserAdminError::LastAdministrator => error(
            StatusCode::CONFLICT,
            "user_last_admin",
            "The last active administrator cannot be deleted.",
        ),
        UserAdminError::Database(cause) => {
            tracing::error!(%cause, "user management database operation failed");
            database_error()
        }
    }
}

fn database_error() -> HttpError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "user_admin_database_error",
        "Could not update user management data.",
    )
}

fn require_available(config: &ConsoleInfoConfig) -> HttpResult<()> {
    if config.webhook_auth {
        return Err(error(
            StatusCode::FORBIDDEN,
            "user_admin_unavailable",
            "User management is unavailable with external authentication.",
        ));
    }
    Ok(())
}

async fn active_actor(auth: &AuthSession, service: &CentralNetworkService) -> HttpResult<i32> {
    let actor = auth.user.as_ref().ok_or_else(|| {
        error(StatusCode::UNAUTHORIZED, "user_admin_unauthenticated", "Please sign in.")
    })?;
    if !service.db.is_user_active(actor.db_user.id).await.map_err(|_| database_error())? {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "user_admin_unauthenticated",
            "Please sign in.",
        ));
    }
    Ok(actor.db_user.id)
}

async fn context(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
) -> HttpResult<Json<Value>> {
    let actor_id = active_actor(&auth, &service).await?;
    let can_manage_users = !config.webhook_auth
        && service.db.is_active_administrator(actor_id).await.map_err(|_| database_error())?;
    Ok(Json(json!({
        "actor": {"id": actor_id, "username": auth.user.as_ref().map(|user| &user.db_user.username)},
        "available": !config.webhook_auth,
        "can_manage_users": can_manage_users,
    })))
}

async fn list_users(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
) -> HttpResult<Json<Value>> {
    require_available(&config)?;
    let actor_id = active_actor(&auth, &service).await?;
    let items = service.db.list_managed_users(actor_id).await.map_err(store_error)?;
    Ok(Json(json!({"items": items})))
}

async fn delete_user(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    discovery: Option<Extension<std::sync::Arc<crate::discovery::Runtime>>>,
    path: Result<Path<i32>, PathRejection>,
) -> HttpResult<Json<Value>> {
    require_available(&config)?;
    let actor_id = active_actor(&auth, &service).await?;
    let Path(user_id) = path.map_err(|_| {
        error(StatusCode::BAD_REQUEST, "user_not_found", "Invalid user ID.")
    })?;
    let _mutation = service.lock_mutations().await;
    let deleted = service.db.retire_user_and_disable_networks(actor_id, user_id)
        .await.map_err(store_error)?;
    // Account retirement and configuration disabling are already committed.
    // Runtime failures are a separate result and never imply a rollback.
    let runtime = match service.disable_deleted_user_runtime(user_id).await {
        Ok(report) => Some(report),
        Err(cause) => {
            tracing::warn!(user_id, %cause, "account retired; network stop remains unconfirmed");
            None
        }
    };
    // Database triggers have already recorded automation revocation in the
    // account retirement transaction. Notify the executor after releasing the
    // existing configuration lock; its failure cannot undo account deletion.
    drop(_mutation);
    let automation_service = service.clone();
    if let Some(Extension(discovery)) = discovery {
        tokio::spawn(async move {
            super::automation::revoke_actor_or_owner(&automation_service, &discovery.settings, user_id).await;
        });
    }
    Ok(Json(json!({
        "user_id": deleted.user_id,
        "deleted_at": deleted.deleted_at,
        "already_deleted": deleted.already_deleted,
        "disabled_network_instance_count": deleted.disabled_network_instance_count,
        "runtime_error": runtime.is_none(),
        "runtime": runtime,
    })))
}

pub(crate) fn router() -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/admin/users/context", get(context))
        .route("/api/v1/admin/users", get(list_users))
        .route("/api/v1/admin/users/{id}", delete(delete_user))
}
