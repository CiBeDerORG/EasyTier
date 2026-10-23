//! Session-authenticated management of manually registered node assets.
//! Existing machine registration and network instance routes remain separate.
mod publication;
mod publication_apply;
pub(super) mod runtime;

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State, rejection::{JsonRejection, QueryRejection}},
    http::StatusCode,
    routing::{delete, get, patch, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use super::{AppStateInner, ConsoleInfoConfig, users::AuthSession};
use crate::{
    central_network::service::{CentralNetworkService, CentralNetworkServiceError},
    db::{Db, assets::{AssetError, AssetsStore, DeviceListQuery}, node_geo},
};

type Service = Extension<Arc<CentralNetworkService>>;
type Config = Extension<ConsoleInfoConfig>;
type HttpError = (StatusCode, Json<Value>);
type HttpResult<T> = Result<T, HttpError>;
type Body = Result<Json<Value>, JsonRejection>;
type OwnerInput = Result<Query<OwnerQuery>, QueryRejection>;
type ScopeInput = Result<Query<ScopeQuery>, QueryRejection>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerQuery {
    owner_user_id: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeQuery {
    owner_user_id: i32,
    machine_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeDeleteQuery {
    owner_user_id: i32,
    #[serde(default)]
    block: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DevicesQuery {
    owner_user_id: i32,
    machine_id: String,
    page: Option<u32>,
    page_size: Option<u32>,
    group_id: Option<String>,
    type_id: Option<String>,
    keyword: Option<String>,
    locate_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstancesQuery {
    owner_user_id: i32,
    machine_id: String,
    role: String,
    subject_kind: Option<String>,
    subject_id: Option<String>,
}

async fn observed_store(manager: &AppStateInner, service: &CentralNetworkService, owner: i32, node: &str) -> AssetsStore {
    let observations = runtime::observe(manager, &service.db, owner, node).await;
    AssetsStore::new(service.db.clone()).with_runtime_observations(observations)
}

async fn publication_store(
    manager: &AppStateInner,
    service: &CentralNetworkService,
    owner: i32,
    body: &Value,
) -> (AssetsStore, Option<HttpError>) {
    let store = AssetsStore::new(service.db.clone());
    let Some(binding) = body.get("publication").filter(|value| value.is_object()) else {
        return (store, None);
    };
    let (Some(machine), Some(instance)) = (binding["machine_id"].as_str(), binding["instance_id"].as_str()) else {
        // Strict repository validation reports malformed fields.
        return (store, None);
    };
    match publication::prove_instance(manager, service, owner, machine.trim(), instance.trim()).await {
        Ok(proof) => (store.with_publication_runtime_proof(Some(proof)), None),
        // A completed create replay and an unchanged publication do not need a
        // fresh online target. Let the transaction decide whether proof is required.
        Err(error) => (store, Some(error)),
    }
}

fn publication_write_error(error: AssetError, runtime_error: Option<HttpError>) -> HttpError {
    if error.code == "publication_runtime_required" {
        if let Some(error) = runtime_error { return error; }
    }
    asset_error(error)
}

async fn notify_service_publication(
    manager: &AppStateInner,
    service: &CentralNetworkService,
    owner: i32,
    mut previous_targets: Vec<String>,
    plan: publication_apply::ApplyPlan,
    response: &mut Value,
) {
    if let Some(machine) = response["publication"]["machine_id"].as_str() {
        previous_targets.push(machine.to_owned());
    }
    let report = publication_apply::apply(manager, service, owner, plan, Some(response)).await;
    response["publication_sync"] = report;
    publication::notify_targets(manager, owner, &previous_targets).await;
}

fn asset_error(error: AssetError) -> HttpError {
    (
        StatusCode::from_u16(error.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(json!({
            "code": error.code,
            "message": error.message,
            "field_errors": error.field_errors,
        })),
    )
}

fn database_error(error: sqlx::Error) -> HttpError {
    asset_error(error.into())
}

fn query_error(_: QueryRejection) -> HttpError {
    asset_error(AssetError::new(422, "invalid_query", "The request context or query is invalid."))
}

fn body_value(body: Body) -> HttpResult<Value> {
    body.map(|Json(value)| value)
        .map_err(|_| asset_error(AssetError::new(422, "invalid_json", "A valid JSON request body is required.")))
}

fn machine_id(value: &str) -> HttpResult<String> {
    Uuid::parse_str(value).map(|id| id.to_string())
        .map_err(|_| asset_error(AssetError::field("machine_id", "A valid node identifier is required.")))
}

fn actor(auth: &AuthSession) -> HttpResult<(i32, &str)> {
    let user = auth.user.as_ref()
        .ok_or_else(|| asset_error(AssetError::new(401, "authentication_required", "Please sign in.")))?;
    Ok((user.db_user.id, user.db_user.username.as_str()))
}

async fn is_admin(db: &Db, actor_id: i32) -> HttpResult<bool> {
    let found: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users_groups ug \
         JOIN groups g ON g.id = ug.group_id WHERE ug.user_id = ? AND g.name = 'admins')",
    )
    .bind(actor_id)
    .fetch_one(&db.inner())
    .await
    .map_err(database_error)?;
    Ok(found != 0)
}

fn require_available(config: &ConsoleInfoConfig) -> HttpResult<()> {
    if config.webhook_auth {
        return Err(asset_error(AssetError::new(
            403, "asset_mode_unavailable", "Asset management is unavailable in external webhook mode.",
        )));
    }
    Ok(())
}

async fn authorize(
    auth: &AuthSession,
    config: &ConsoleInfoConfig,
    service: &CentralNetworkService,
    owner: i32,
) -> HttpResult<i32> {
    let (actor_id, _) = actor(auth)?;
    require_available(config)?;
    if actor_id != owner && !is_admin(&service.db, actor_id).await? {
        return Err(asset_error(AssetError::new(403, "asset_access_denied", "You cannot manage this account's assets.")));
    }
    let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ?)")
        .bind(owner).fetch_one(&service.db.inner()).await.map_err(database_error)?;
    if exists == 0 {
        return Err(asset_error(AssetError::new(404, "asset_owner_not_found", "The selected account no longer exists.")));
    }
    Ok(actor_id)
}

async fn scoped_context(
    auth: &AuthSession,
    config: &ConsoleInfoConfig,
    service: &CentralNetworkService,
    query: ScopeInput,
) -> HttpResult<(i32, i32, String)> {
    let Query(query) = query.map_err(query_error)?;
    let actor_id = authorize(auth, config, service, query.owner_user_id).await?;
    // Do not check the node here: create replay must be able to return 410 for
    // an already deleted result before the repository checks its parents.
    Ok((actor_id, query.owner_user_id, machine_id(&query.machine_id)?))
}

async fn context(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
) -> HttpResult<Json<Value>> {
    let (id, username) = actor(&auth)?;
    let can_manage_others = is_admin(&service.db, id).await?;
    Ok(Json(json!({
        "actor": {"id": id, "username": username},
        "can_manage_others": can_manage_others,
        "available": !config.webhook_auth,
    })))
}

async fn accounts(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
) -> HttpResult<Json<Value>> {
    let (id, _) = actor(&auth)?;
    require_available(&config)?;
    if !is_admin(&service.db, id).await? {
        return Err(asset_error(AssetError::new(403, "asset_access_denied", "Administrator access is required.")));
    }
    let rows = sqlx::query("SELECT id, username FROM users ORDER BY username, id")
        .fetch_all(&service.db.inner()).await.map_err(database_error)?;
    let items = rows.into_iter().map(|row| {
        Ok(json!({
            "id": row.try_get::<i32, _>("id").map_err(database_error)?,
            "username": row.try_get::<String, _>("username").map_err(database_error)?,
        }))
    }).collect::<HttpResult<Vec<_>>>()?;
    Ok(Json(json!({"items": items})))
}

const NODE_COLUMNS: &str = "SELECT d.user_id AS owner_user_id, d.machine_id, d.hostname, d.alias, \
    d.site_location, d.device_model, d.connection_ip, d.ip_country, d.ip_region, d.ip_city, \
    g.owner_user_id AS geo_owner_user_id, g.country AS geo_country, g.region AS geo_region, \
    g.city AS geo_city, g.district AS geo_district, g.latitude AS geo_latitude, g.longitude AS geo_longitude, \
    COALESCE(m.is_public_server, 0) AS is_public_server, \
    (SELECT COUNT(*) FROM asset_devices a WHERE a.owner_user_id = d.user_id AND a.machine_id = d.machine_id) AS device_count \
    FROM devices d LEFT JOIN node_asset_metadata m ON m.owner_user_id = d.user_id AND m.machine_id = d.machine_id \
    LEFT JOIN node_geo_locations g ON g.owner_user_id = d.user_id AND g.machine_id = d.machine_id";

fn node_json(row: SqliteRow) -> HttpResult<Value> {
    Ok(json!({
        "owner_user_id": row.try_get::<i32, _>("owner_user_id").map_err(database_error)?,
        "machine_id": row.try_get::<String, _>("machine_id").map_err(database_error)?,
        "hostname": row.try_get::<String, _>("hostname").map_err(database_error)?,
        "alias": row.try_get::<String, _>("alias").map_err(database_error)?,
        "site_location": row.try_get::<Option<String>, _>("site_location").map_err(database_error)?,
        "geo_location": node_geo::row_location(&row).map_err(database_error)?,
        "device_model": row.try_get::<Option<String>, _>("device_model").map_err(database_error)?,
        "connection_ip": row.try_get::<Option<String>, _>("connection_ip").map_err(database_error)?,
        "ip_country": row.try_get::<Option<String>, _>("ip_country").map_err(database_error)?,
        "ip_region": row.try_get::<Option<String>, _>("ip_region").map_err(database_error)?,
        "ip_city": row.try_get::<Option<String>, _>("ip_city").map_err(database_error)?,
        "is_public_server": row.try_get::<i64, _>("is_public_server").map_err(database_error)? != 0,
        "device_count": row.try_get::<i64, _>("device_count").map_err(database_error)?,
    }))
}

async fn nodes(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    authorize(&auth, &config, &service, query.owner_user_id).await?;
    let sql = format!("{NODE_COLUMNS} WHERE d.user_id = ? ORDER BY d.last_seen_time DESC, d.machine_id");
    let rows = sqlx::query(&sql).bind(query.owner_user_id)
        .fetch_all(&service.db.inner()).await.map_err(database_error)?;
    let items = rows.into_iter().map(node_json).collect::<HttpResult<Vec<_>>>()?;
    Ok(Json(json!({"items": items})))
}

async fn device_counts(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    authorize(&auth, &config, &service, query.owner_user_id).await?;
    let rows = sqlx::query(
        "SELECT d.machine_id, COUNT(a.id) AS device_count FROM devices d \
         LEFT JOIN asset_devices a ON a.owner_user_id = d.user_id AND a.machine_id = d.machine_id \
         WHERE d.user_id = ? GROUP BY d.machine_id ORDER BY d.machine_id",
    )
    .bind(query.owner_user_id).fetch_all(&service.db.inner()).await.map_err(database_error)?;
    let items = rows.into_iter().map(|row| {
        Ok(json!({
            "machine_id": row.try_get::<String, _>("machine_id").map_err(database_error)?,
            "device_count": row.try_get::<i64, _>("device_count").map_err(database_error)?,
        }))
    }).collect::<HttpResult<Vec<_>>>()?;
    Ok(Json(json!({"items": items})))
}

fn manual_attribute(body: &Value, field: &str, max: usize) -> HttpResult<Option<String>> {
    match body.get(field) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            // Match the existing node attribute rules, including checking for
            // control characters before trimming and location's 256 limit.
            if value.chars().any(char::is_control) {
                return Err(asset_error(AssetError::field(field, "Control characters are not allowed.")));
            }
            let value = value.trim();
            if value.chars().count() > max {
                return Err(asset_error(AssetError::field(field, &format!("Use at most {max} characters."))));
            }
            Ok((!value.is_empty()).then(|| value.to_owned()))
        }
        _ => Err(asset_error(AssetError::field(field, "Provide text or null for this field."))),
    }
}

async fn update_node(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(node): Path<String>,
    query: OwnerInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    authorize(&auth, &config, &service, query.owner_user_id).await?;
    let node = machine_id(&node)?;
    let body = body_value(body)?;
    let object = body.as_object().ok_or_else(|| asset_error(AssetError::field("body", "Expected an object.")))?;
    for key in object.keys() {
        if !["is_public_server", "site_location", "device_model", "geo_location"].contains(&key.as_str()) {
            return Err(asset_error(AssetError::field(key, "This field is not editable.")));
        }
    }
    let is_public = body.get("is_public_server").and_then(Value::as_bool)
        .ok_or_else(|| asset_error(AssetError::field("is_public_server", "Select a valid node role.")))?;
    let location = manual_attribute(&body, "site_location", 256)?;
    let model = manual_attribute(&body, "device_model", 128)?;
    let geo_location = body.get("geo_location").map(node_geo::parse_geo_location).transpose()
        .map_err(|error| asset_error(AssetError::field(&error.field, &error.message)))?;
    let _mutation = service.lock_mutations().await;
    let pool = service.db.inner();
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(database_error)?;
    let updated = sqlx::query(
        "UPDATE devices SET site_location = ?, device_model = ? WHERE user_id = ? AND machine_id = ?",
    )
    .bind(location).bind(model).bind(query.owner_user_id).bind(&node)
    .execute(&mut *transaction).await.map_err(database_error)?;
    if updated.rows_affected() == 0 {
        return Err(asset_error(AssetError::new(404, "asset_node_not_found", "The selected node no longer exists.")));
    }
    sqlx::query(
        "INSERT INTO node_asset_metadata(owner_user_id, machine_id, is_public_server) VALUES (?, ?, ?) \
         ON CONFLICT(owner_user_id, machine_id) DO UPDATE SET is_public_server = excluded.is_public_server",
    )
    .bind(query.owner_user_id).bind(&node).bind(is_public)
    .execute(&mut *transaction).await.map_err(database_error)?;
    if let Some(location) = geo_location {
        node_geo::write_location(&mut transaction, query.owner_user_id, &node, location.as_ref())
            .await.map_err(database_error)?;
    }
    let sql = format!("{NODE_COLUMNS} WHERE d.user_id = ? AND d.machine_id = ?");
    let row = sqlx::query(&sql).bind(query.owner_user_id).bind(&node)
        .fetch_one(&mut *transaction).await.map_err(database_error)?;
    let result = node_json(row)?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(result))
}

async fn delete_node(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(node): Path<String>,
    query: Result<Query<NodeDeleteQuery>, QueryRejection>,
) -> HttpResult<StatusCode> {
    let Query(query) = query.map_err(query_error)?;
    authorize(&auth, &config, &service, query.owner_user_id).await?;
    let node = Uuid::parse_str(&machine_id(&node)?)
        .map_err(|_| asset_error(AssetError::field("machine_id", "A valid node identifier is required.")))?;
    // The existing service acquires the shared mutation lock and performs the
    // child-asset guard before any network/blacklist side effect. Do not relock.
    let deleted = service.delete_device(query.owner_user_id, node, query.block).await
        .map_err(|error| match error {
            CentralNetworkServiceError::Invalid(message) => asset_error(AssetError::new(422, "asset_node_invalid", &message)),
            CentralNetworkServiceError::NotFound(_) => asset_error(AssetError::new(404, "asset_node_not_found", "The selected node no longer exists.")),
            CentralNetworkServiceError::Conflict(message) => asset_error(AssetError::new(409, "asset_node_conflict", &message)),
            CentralNetworkServiceError::Database(error) => {
                tracing::error!(error = %error, "node asset deletion failed");
                asset_error(AssetError::new(500, "asset_database_error", "The node could not be deleted."))
            }
        })?;
    if !deleted {
        return Err(asset_error(AssetError::new(404, "asset_node_not_found", "The selected node no longer exists.")));
    }
    Ok(StatusCode::NO_CONTENT)
}

// Repository routes below share the same authorization and write coordination.
// Bodies are deliberately passed to the repository's strict field validators.

async fn list_groups(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: ScopeInput,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let store = observed_store(&manager, &service, owner, &node).await;
    let result = store.list_groups(owner, &node).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn create_group(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (actor_id, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = observed_store(&manager, &service, owner, &node).await;
    let _mutation = service.lock_mutations().await;
    let result = store.create_group(actor_id, owner, &node, body).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn update_group(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    let result = store.update_group(owner, &node, &id, body).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn delete_group(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
) -> HttpResult<StatusCode> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    store.delete_group(owner, &node, &id).await.map_err(asset_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_device(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let store = observed_store(&manager, &service, owner, &node).await;
    let result = store.get_device(owner, &node, &id).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn create_device(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (actor_id, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = observed_store(&manager, &service, owner, &node).await;
    let _mutation = service.lock_mutations().await;
    let result = store.create_device(actor_id, owner, &node, body).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn update_device(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    let targets = store.publication_targets_for_node(owner, &node).await.map_err(asset_error)?;
    let plan = publication_apply::snapshot(&service, owner, &node, None).await?;
    let mut result = store.update_device(owner, &node, &id, body).await.map_err(asset_error)?;
    result["publication_sync"] = publication_apply::apply(&manager, &service, owner, plan, None).await;
    publication::notify_targets(&manager, owner, &targets).await;
    Ok(Json(result))
}

async fn delete_device(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
) -> HttpResult<StatusCode> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    let targets = store.publication_targets_for_node(owner, &node).await.map_err(asset_error)?;
    let plan = publication_apply::snapshot(&service, owner, &node, None).await?;
    store.delete_device(owner, &node, &id).await.map_err(asset_error)?;
    let _report = publication_apply::apply(&manager, &service, owner, plan, None).await;
    publication::notify_targets(&manager, owner, &targets).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn service_catalog(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    authorize(&auth, &config, &service, query.owner_user_id).await?;
    let result = AssetsStore::new(service.db.clone()).service_catalog(query.owner_user_id).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn set_service_navigation(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let _mutation = service.lock_mutations().await;
    let result = AssetsStore::new(service.db.clone()).set_service_navigation(owner, &node, &id, body).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn list_services(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let store = AssetsStore::new(service.db.clone());
    let result = store.list_services(owner, &node, &id).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn create_service(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (actor_id, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let _mutation = service.lock_mutations().await;
    let (store, runtime_error) = publication_store(&manager, &service, owner, &body).await;
    let targets = store.publication_targets_for_node(owner, &node).await.map_err(asset_error)?;
    let plan = publication_apply::snapshot(&service, owner, &node, body.get("publication")).await?;
    let mut result = store.create_service(actor_id, owner, &node, &id, body).await
        .map_err(|error| publication_write_error(error, runtime_error))?;
    notify_service_publication(&manager, &service, owner, targets, plan, &mut result).await;
    Ok(Json(result))
}

async fn update_service(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let _mutation = service.lock_mutations().await;
    let (store, runtime_error) = publication_store(&manager, &service, owner, &body).await;
    let targets = store.publication_targets_for_node(owner, &node).await.map_err(asset_error)?;
    let plan = publication_apply::snapshot(&service, owner, &node, body.get("publication")).await?;
    let mut result = store.update_service(owner, &node, &id, body).await
        .map_err(|error| publication_write_error(error, runtime_error))?;
    notify_service_publication(&manager, &service, owner, targets, plan, &mut result).await;
    Ok(Json(result))
}

async fn delete_service(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    let targets = store.publication_targets_for_node(owner, &node).await.map_err(asset_error)?;
    let plan = publication_apply::snapshot(&service, owner, &node, None).await?;
    store.delete_service(owner, &node, &id).await.map_err(asset_error)?;
    let report = publication_apply::apply(&manager, &service, owner, plan, None).await;
    publication::notify_targets(&manager, owner, &targets).await;
    Ok(Json(json!({"deleted":true,"publication_sync":report})))
}

async fn preview_subnets(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = AssetsStore::new(service.db.clone());
    let result = store.preview_subnets(owner, &node, body).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn preview_device_address(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = AssetsStore::new(service.db.clone());
    let result = store.preview_device_address(owner, &node, body).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn list_devices(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: Result<Query<DevicesQuery>, QueryRejection>,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    authorize(&auth, &config, &service, query.owner_user_id).await?;
    let node = machine_id(&query.machine_id)?;
    let owner = query.owner_user_id;
    let filter = DeviceListQuery {
        page: query.page,
        page_size: query.page_size,
        group_id: query.group_id,
        type_id: query.type_id,
        keyword: query.keyword,
        locate_id: query.locate_id,
    };
    let store = observed_store(&manager, &service, owner, &node).await;
    let result = store.list_devices(owner, &node, filter).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn initialize_types(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    let owner = query.owner_user_id;
    authorize(&auth, &config, &service, owner).await?;
    let body = body_value(body)?;
    if !body.as_object().is_some_and(|object| object.is_empty()) {
        return Err(asset_error(AssetError::field("body", "Initialization expects an empty object.")));
    }
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    let result = store.initialize_types(owner).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn list_types(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    let owner = query.owner_user_id;
    authorize(&auth, &config, &service, owner).await?;
    let store = AssetsStore::new(service.db.clone());
    let result = store.list_types(owner).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn create_type(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    let owner = query.owner_user_id;
    let actor_id = authorize(&auth, &config, &service, owner).await?;
    let body = body_value(body)?;
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    let result = store.create_type(actor_id, owner, body).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn update_type(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: OwnerInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    let owner = query.owner_user_id;
    authorize(&auth, &config, &service, owner).await?;
    let body = body_value(body)?;
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    let result = store.update_type(owner, &id, body).await.map_err(asset_error)?;
    Ok(Json(result))
}

async fn delete_type(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: OwnerInput,
) -> HttpResult<StatusCode> {
    let Query(query) = query.map_err(query_error)?;
    let owner = query.owner_user_id;
    authorize(&auth, &config, &service, owner).await?;
    let _mutation = service.lock_mutations().await;
    let store = AssetsStore::new(service.db.clone());
    store.delete_type(owner, &id).await.map_err(asset_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_network_instances(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: Result<Query<InstancesQuery>, QueryRejection>,
) -> HttpResult<Json<Value>> {
    let Query(query) = query.map_err(query_error)?;
    authorize(&auth, &config, &service, query.owner_user_id).await?;
    let node = machine_id(&query.machine_id)?;
    let store = observed_store(&manager, &service, query.owner_user_id, &node).await;
    Ok(Json(store.list_instance_candidates(query.owner_user_id, &node, &query.role,
        query.subject_kind.as_deref(), query.subject_id.as_deref()).await.map_err(asset_error)?))
}

async fn list_publication_networks(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: ScopeInput,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    Ok(Json(publication::list_instances(&manager, &service, owner, &node).await?))
}

async fn preview_instance_address(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = observed_store(&manager, &service, owner, &node).await;
    Ok(Json(store.preview_instance_address(owner, &node, body).await.map_err(asset_error)?))
}

async fn preview_binding(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = observed_store(&manager, &service, owner, &node).await;
    Ok(Json(store.preview_binding(owner, &node, body).await.map_err(asset_error)?))
}

async fn apply_gateway_binding(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (actor, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = observed_store(&manager, &service, owner, &node).await;
    let _mutation = service.lock_mutations().await;
    let targets = store.publication_targets_for_node(owner, &node).await.map_err(asset_error)?;
    let plan = publication_apply::snapshot(&service, owner, &node, None).await?;
    let mut result = store.apply_binding(actor, owner, &node, "gateway", &id, body).await.map_err(asset_error)?;
    result["publication_sync"] = publication_apply::apply(&manager, &service, owner, plan, None).await;
    publication::notify_targets(&manager, owner, &targets).await;
    Ok(Json(result))
}

async fn apply_device_binding(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (actor, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = observed_store(&manager, &service, owner, &node).await;
    let _mutation = service.lock_mutations().await;
    let targets = store.publication_targets_for_node(owner, &node).await.map_err(asset_error)?;
    let plan = publication_apply::snapshot(&service, owner, &node, None).await?;
    let mut result = store.apply_binding(actor, owner, &node, "device", &id, body).await.map_err(asset_error)?;
    result["publication_sync"] = publication_apply::apply(&manager, &service, owner, plan, None).await;
    publication::notify_targets(&manager, owner, &targets).await;
    Ok(Json(result))
}

async fn apply_device_address(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
    body: Body,
) -> HttpResult<Json<Value>> {
    let (actor, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let body = body_value(body)?;
    let store = observed_store(&manager, &service, owner, &node).await;
    let _mutation = service.lock_mutations().await;
    let targets = store.publication_targets_for_node(owner, &node).await.map_err(asset_error)?;
    let plan = publication_apply::snapshot(&service, owner, &node, None).await?;
    let mut result = store.apply_address(actor, owner, &node, &id, body).await.map_err(asset_error)?;
    result["publication_sync"] = publication_apply::apply(&manager, &service, owner, plan, None).await;
    publication::notify_targets(&manager, owner, &targets).await;
    Ok(Json(result))
}

pub(crate) fn router() -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/assets/context", get(context))
        .route("/api/v1/assets/accounts", get(accounts))
        .route("/api/v1/assets/nodes", get(nodes))
        .route("/api/v1/assets/device-counts", get(device_counts))
        .route("/api/v1/assets/network-instances", get(list_network_instances))
        .route("/api/v1/assets/service-publication/networks", get(list_publication_networks))
        .route("/api/v1/assets/nodes/{machine_id}/metadata", patch(update_node))
        .route("/api/v1/assets/nodes/{machine_id}", delete(delete_node))
        .route("/api/v1/assets/gateway-groups", get(list_groups).post(create_group))
        .route("/api/v1/assets/gateway-groups/{id}", put(update_group).delete(delete_group))
        .route("/api/v1/assets/gateway-groups/{id}/binding", put(apply_gateway_binding))
        .route("/api/v1/assets/device-types/initialize", post(initialize_types))
        .route("/api/v1/assets/device-types", get(list_types).post(create_type))
        .route("/api/v1/assets/device-types/{id}", put(update_type).delete(delete_type))
        .route("/api/v1/assets/devices", get(list_devices).post(create_device))
        .route("/api/v1/assets/devices/{id}", get(get_device).put(update_device).delete(delete_device))
        .route("/api/v1/assets/devices/{id}/binding", put(apply_device_binding))
        .route("/api/v1/assets/devices/{id}/address", put(apply_device_address))
        .route("/api/v1/assets/devices/{id}/services", get(list_services).post(create_service))
        .route("/api/v1/assets/services", get(service_catalog))
        .route("/api/v1/assets/services/{id}/navigation", patch(set_service_navigation))
        .route("/api/v1/assets/services/{id}", put(update_service).delete(delete_service))
        .route("/api/v1/assets/validation/gateway-subnets", post(preview_subnets))
        .route("/api/v1/assets/validation/device-address", post(preview_device_address))
        .route("/api/v1/assets/validation/instance-address", post(preview_instance_address))
        .route("/api/v1/assets/validation/binding", post(preview_binding))
}

#[cfg(test)]
mod tests;
