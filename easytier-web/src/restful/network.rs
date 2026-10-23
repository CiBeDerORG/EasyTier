use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Path, Query};
use axum::http::StatusCode;
use axum::routing::{delete, post, put};
use axum::{Extension, Json, Router, extract::State, routing::get};
use axum_login::AuthUser;
use easytier::common::config::{
    ConfigSource as RuntimeConfigSource, NetworkConfig, config_source_from_rpc,
};
use easytier::proto::api::config::VpnPortalClientPatch;
use easytier::proto::common::Void;
use easytier::proto::{api::manage::*, web::*};
use easytier_core::management::remote_client::{
    GetNetworkMetasResponse, ListNetworkInstanceIdsJsonResp, RemoteClientError, RemoteClientManager,
};
use sea_orm::DbErr;

use crate::central_network::service::{CentralNetworkService, DeviceNetworkSummary};
use crate::client_manager::session::Location;
use crate::db::UserIdInDb;

use super::users::AuthSession;
use super::{
    AppState, AppStateInner, Error, HttpHandleError, RpcError, convert_db_error, other_error,
};

const MAX_MANAGED_CONFIG_REQUEST_BODY_SIZE: usize = 32 * 1024 * 1024;

pub(super) fn central_ownership_conflict() -> HttpHandleError {
    (
        StatusCode::CONFLICT,
        Json(Error {
            message: "central network config must be changed through its network intent".to_owned(),
            code: Some("central_network_ownership_conflict".to_owned()),
            current_config_revision: None,
        }),
    )
}

async fn ensure_direct_mutation_allowed(
    service: &CentralNetworkService,
    user_id: UserIdInDb,
    machine_id: uuid::Uuid,
    instance_id: uuid::Uuid,
    network_name: Option<&str>,
) -> Result<(), HttpHandleError> {
    let central = service
        .db
        .central_owned_runtime_configs(user_id, machine_id)
        .await
        .map_err(convert_db_error)?;
    if central.iter().any(|config| {
        config.instance_id == instance_id
            || network_name.is_some_and(|name| config.network_name == name)
    }) {
        return Err(central_ownership_conflict());
    }
    Ok(())
}

fn convert_rpc_error(e: RpcError) -> (StatusCode, Json<Error>) {
    let status_code = match &e {
        RpcError::ExecutionError(_) => StatusCode::BAD_REQUEST,
        RpcError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
        _ => StatusCode::BAD_GATEWAY,
    };
    let error = Error {
        message: format!("{:?}", e),
        code: None,
        current_config_revision: None,
    };
    (status_code, Json(error))
}

fn convert_error(e: RemoteClientError<DbErr>) -> (StatusCode, Json<Error>) {
    match e {
        RemoteClientError::PersistentError(e) => convert_db_error(e),
        RemoteClientError::RpcError(e) => convert_rpc_error(e),
        RemoteClientError::ClientNotFound => (
            StatusCode::NOT_FOUND,
            other_error("Client not found").into(),
        ),
        RemoteClientError::NotFound(msg) => (StatusCode::NOT_FOUND, other_error(msg).into()),
        RemoteClientError::Other(msg) if msg.starts_with("invalid instance ID:") => {
            (StatusCode::BAD_REQUEST, other_error(msg).into())
        }
        RemoteClientError::Other(msg) => {
            (StatusCode::INTERNAL_SERVER_ERROR, other_error(msg).into())
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ValidateConfigJsonReq {
    config: NetworkConfig,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct SaveNetworkJsonReq {
    config: NetworkConfig,
}

#[derive(Debug, serde::Deserialize)]
struct PatchVpnPortalClientsJsonReq {
    patches: Vec<VpnPortalClientPatch>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct RunNetworkJsonReq {
    config: NetworkConfig,
    save: bool,
    source: Option<i32>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct CollectNetworkInfoJsonReq {
    inst_ids: Option<Vec<uuid::Uuid>>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct UpdateNetworkStateJsonReq {
    disabled: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct GetNetworkMetasJsonReq {
    instance_ids: Vec<uuid::Uuid>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct RemoveNetworkJsonReq {
    inst_ids: Vec<uuid::Uuid>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct DeleteMachineParams {
    block: Option<bool>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct UpdateDeviceAliasJsonReq {
    alias: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateDeviceAttributesJsonReq {
    site_location: String,
    device_model: String,
}

#[derive(Debug, serde::Serialize)]
struct DeviceAttributesJsonResp {
    site_location: Option<String>,
    device_model: Option<String>,
}

fn normalize_device_attribute(
    value: &str,
    field: &str,
    max_chars: usize,
) -> Result<Option<String>, HttpHandleError> {
    // Check before trimming so a trailing newline or NUL is never accepted.
    if value.chars().any(char::is_control) {
        return Err((
            StatusCode::BAD_REQUEST,
            other_error(format!("{field} must not contain control characters")).into(),
        ));
    }
    let value = value.trim();
    if value.chars().count() > max_chars {
        return Err((
            StatusCode::BAD_REQUEST,
            other_error(format!("{field} must be at most {max_chars} characters")).into(),
        ));
    }
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

#[derive(Debug, serde::Serialize)]
struct BlockedDeviceItem {
    id: String,
    user_id: UserIdInDb,
    hostname: String,
    blocked_time: chrono::DateTime<chrono::FixedOffset>,
    attempt_count: i32,
    last_attempt_time: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl From<crate::db::entity::blocked_devices::Model> for BlockedDeviceItem {
    fn from(device: crate::db::entity::blocked_devices::Model) -> Self {
        Self {
            id: device.machine_id,
            user_id: device.user_id,
            hostname: device.hostname,
            blocked_time: device.blocked_time,
            attempt_count: device.attempt_count,
            last_attempt_time: device.last_attempt_time,
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct ListBlockedDevicesJsonResp {
    blocked: Vec<BlockedDeviceItem>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ManagedNetworkConfigJson {
    instance_id: uuid::Uuid,
    network_config: serde_json::Value,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ReconcileManagedNetworkConfigsJsonReq {
    managed_network_configs: Vec<ManagedNetworkConfigJson>,
    config_revision: Option<String>,
    expected_config_revision: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct PatchManagedNetworkConfigsJsonReq {
    upserts: Vec<ManagedNetworkConfigJson>,
    delete_instance_ids: Vec<uuid::Uuid>,
    config_revision: String,
    expected_config_revision: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ListMachineItem {
    client_url: Option<url::Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    info: Option<HeartbeatRequest>,
    location: Option<Location>,
    connection_ip: Option<String>,
    ip_country: Option<String>,
    ip_region: Option<String>,
    ip_city: Option<String>,
    #[serde(default = "unknown_ip_location_status")]
    ip_location_status: String,
    ip_observed_at: Option<String>,
    site_location: Option<String>,
    device_model: Option<String>,
    #[serde(default)]
    networks: Vec<DeviceNetworkSummary>,
    #[serde(default)]
    online: bool,
    #[serde(default)]
    last_seen: Option<String>,
}

fn unknown_ip_location_status() -> String {
    "unknown".to_owned()
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ListMachineJsonResp {
    machines: Vec<ListMachineItem>,
}

pub struct NetworkApi;

const DEVICE_ALIAS_MAX_CHARS: usize = 64;
const SITE_LOCATION_MAX_CHARS: usize = 256;
const DEVICE_MODEL_MAX_CHARS: usize = 128;

impl NetworkApi {
    fn convert_managed_config_error(error: anyhow::Error) -> HttpHandleError {
        let (status, code, current_config_revision) =
            match error.downcast_ref::<crate::client_manager::ManagedConfigError>() {
                Some(crate::client_manager::ManagedConfigError::Invalid(_)) => {
                    (StatusCode::BAD_REQUEST, None, None)
                }
                Some(crate::client_manager::ManagedConfigError::RevisionConflict {
                    current,
                    ..
                }) => (
                    StatusCode::CONFLICT,
                    Some("managed_config_revision_conflict".to_string()),
                    current.clone(),
                ),
                Some(crate::client_manager::ManagedConfigError::OwnershipConflict { .. }) => (
                    StatusCode::CONFLICT,
                    Some("managed_config_ownership_conflict".to_string()),
                    None,
                ),
                None => (StatusCode::INTERNAL_SERVER_ERROR, None, None),
            };
        (
            status,
            Json(Error {
                message: error.to_string(),
                code,
                current_config_revision,
            }),
        )
    }

    fn get_user_id(auth_session: &AuthSession) -> Result<UserIdInDb, (StatusCode, Json<Error>)> {
        super::authed_user_id(auth_session)
    }

    async fn handle_validate_config(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path(machine_id): Path<uuid::Uuid>,
        Json(payload): Json<ValidateConfigJsonReq>,
    ) -> Result<Json<ValidateConfigResponse>, HttpHandleError> {
        Ok(client_mgr
            .handle_validate_config(
                (Self::get_user_id(&auth_session)?, machine_id),
                payload.config,
            )
            .await
            .map_err(convert_error)?
            .into())
    }

    async fn handle_run_network_instance(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path(machine_id): Path<uuid::Uuid>,
        Json(mut payload): Json<RunNetworkJsonReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        let user_id = Self::get_user_id(&auth_session)?;
        let instance_id = match payload.config.instance_id.as_deref() {
            Some(raw) => uuid::Uuid::parse_str(raw).map_err(|error| {
                (
                    StatusCode::BAD_REQUEST,
                    other_error(format!("invalid instance ID: {error}")).into(),
                )
            })?,
            None => uuid::Uuid::new_v4(),
        };
        payload.config.instance_id = Some(instance_id.to_string());
        let _mutation = network_service.lock_mutations().await;
        ensure_direct_mutation_allowed(
            &network_service,
            user_id,
            machine_id,
            instance_id,
            payload.config.network_name.as_deref(),
        )
        .await?;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        let result = client_mgr
            .handle_run_network_instance_with_source(
                (user_id, machine_id),
                payload.config,
                payload.save,
                RuntimeConfigSource::Web,
            )
            .await;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        result.map_err(convert_error)?;
        Ok(Void::default().into())
    }

    async fn handle_collect_one_network_info(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((machine_id, inst_id)): Path<(uuid::Uuid, uuid::Uuid)>,
    ) -> Result<Json<CollectNetworkInfoResponse>, HttpHandleError> {
        Ok(client_mgr
            .handle_collect_network_info(
                (Self::get_user_id(&auth_session)?, machine_id),
                Some(vec![inst_id]),
            )
            .await
            .map_err(convert_error)?
            .into())
    }

    async fn handle_collect_network_info(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path(machine_id): Path<uuid::Uuid>,
        Json(payload): Json<CollectNetworkInfoJsonReq>,
    ) -> Result<Json<CollectNetworkInfoResponse>, HttpHandleError> {
        Ok(client_mgr
            .handle_collect_network_info(
                (Self::get_user_id(&auth_session)?, machine_id),
                payload.inst_ids,
            )
            .await
            .map_err(convert_error)?
            .into())
    }

    async fn handle_list_network_instance_ids(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path(machine_id): Path<uuid::Uuid>,
    ) -> Result<Json<ListNetworkInstanceIdsJsonResp>, HttpHandleError> {
        Ok(client_mgr
            .handle_list_network_instance_ids((Self::get_user_id(&auth_session)?, machine_id))
            .await
            .map_err(convert_error)?
            .into())
    }

    async fn handle_remove_network_instance(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path((machine_id, inst_id)): Path<(uuid::Uuid, uuid::Uuid)>,
    ) -> Result<(), HttpHandleError> {
        let user_id = Self::get_user_id(&auth_session)?;
        let _mutation = network_service.lock_mutations().await;
        ensure_direct_mutation_allowed(&network_service, user_id, machine_id, inst_id, None)
            .await?;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        let result = client_mgr
            .handle_remove_network_instances((user_id, machine_id), vec![inst_id])
            .await;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        result.map_err(convert_error)?;
        Ok(())
    }

    async fn handle_delete_machine(
        auth_session: AuthSession,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path(machine_id): Path<uuid::Uuid>,
        Query(params): Query<DeleteMachineParams>,
    ) -> Result<StatusCode, HttpHandleError> {
        let deleted = network_service
            .delete_device(
                Self::get_user_id(&auth_session)?,
                machine_id,
                params.block.unwrap_or(false),
            )
            .await
            .map_err(super::central_network::convert_error)?;
        if !deleted {
            return Err((
                StatusCode::NOT_FOUND,
                other_error(format!("device not found: {machine_id}")).into(),
            ));
        }
        Ok(StatusCode::NO_CONTENT)
    }

    async fn handle_update_machine_alias(
        auth_session: AuthSession,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path(machine_id): Path<uuid::Uuid>,
        Json(request): Json<UpdateDeviceAliasJsonReq>,
    ) -> Result<StatusCode, HttpHandleError> {
        let alias = request.alias.trim().to_owned();
        if alias.chars().count() > DEVICE_ALIAS_MAX_CHARS {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error(format!(
                    "alias must be at most {DEVICE_ALIAS_MAX_CHARS} characters"
                ))
                .into(),
            ));
        }
        let updated = network_service
            .db
            .set_device_alias((Self::get_user_id(&auth_session)?, machine_id), alias)
            .await
            .map_err(convert_db_error)?;
        if !updated {
            return Err((
                StatusCode::NOT_FOUND,
                other_error(format!("device not found: {machine_id}")).into(),
            ));
        }
        Ok(StatusCode::NO_CONTENT)
    }

    async fn handle_update_machine_attributes(
        auth_session: AuthSession,
        Extension(config): Extension<super::ConsoleInfoConfig>,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path(machine_id): Path<uuid::Uuid>,
        request: Result<
            Json<UpdateDeviceAttributesJsonReq>,
            axum::extract::rejection::JsonRejection,
        >,
    ) -> Result<Json<DeviceAttributesJsonResp>, HttpHandleError> {
        let user_id = Self::get_user_id(&auth_session)?;
        if config.webhook_auth {
            return Err((
                StatusCode::FORBIDDEN,
                other_error("node attributes are unavailable in external authentication mode")
                    .into(),
            ));
        }
        let Json(request) = request.map_err(|error| {
            let status = match error.status() {
                StatusCode::UNPROCESSABLE_ENTITY => StatusCode::BAD_REQUEST,
                status => status,
            };
            (status, Json(other_error(error.body_text())))
        })?;
        let attributes = DeviceAttributesJsonResp {
            site_location: normalize_device_attribute(
                &request.site_location,
                "site_location",
                SITE_LOCATION_MAX_CHARS,
            )?,
            device_model: normalize_device_attribute(
                &request.device_model,
                "device_model",
                DEVICE_MODEL_MAX_CHARS,
            )?,
        };
        let updated = network_service
            .db
            .set_device_attributes(
                (user_id, machine_id),
                attributes.site_location.as_deref(),
                attributes.device_model.as_deref(),
            )
            .await
            .map_err(convert_db_error)?;
        if !updated {
            return Err((
                StatusCode::NOT_FOUND,
                other_error(format!("device not found: {machine_id}")).into(),
            ));
        }
        Ok(Json(attributes))
    }

    async fn handle_list_blocked_devices(
        auth_session: AuthSession,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
    ) -> Result<Json<ListBlockedDevicesJsonResp>, HttpHandleError> {
        let blocked = network_service
            .db
            .list_blocked_devices(Self::get_user_id(&auth_session)?)
            .await
            .map_err(convert_db_error)?
            .into_iter()
            .map(BlockedDeviceItem::from)
            .collect();
        Ok(Json(ListBlockedDevicesJsonResp { blocked }))
    }

    async fn handle_unblock_device(
        auth_session: AuthSession,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path(machine_id): Path<uuid::Uuid>,
    ) -> Result<StatusCode, HttpHandleError> {
        network_service
            .db
            .unblock_device((Self::get_user_id(&auth_session)?, machine_id))
            .await
            .map_err(convert_db_error)?;
        Ok(StatusCode::NO_CONTENT)
    }

    async fn handle_list_machines(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Extension(config): Extension<super::ConsoleInfoConfig>,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
    ) -> Result<Json<ListMachineJsonResp>, HttpHandleError> {
        let user_id = Self::get_user_id(&auth_session)?;
        if config.webhook_auth {
            let client_urls = client_mgr.list_machine_by_user_id(user_id).await;
            let mut machines = Vec::with_capacity(client_urls.len());
            for client_url in client_urls {
                let info = client_mgr.get_heartbeat_requests(&client_url).await;
                let location = client_mgr.get_machine_location(&client_url).await;
                machines.push(ListMachineItem {
                    client_url: Some(client_url),
                    online: info.is_some(),
                    last_seen: info.as_ref().map(|info| info.report_time.clone()),
                    info,
                    location,
                    connection_ip: None,
                    ip_country: None,
                    ip_region: None,
                    ip_city: None,
                    ip_location_status: unknown_ip_location_status(),
                    ip_observed_at: None,
                    site_location: None,
                    device_model: None,
                    alias: None,
                    networks: Vec::new(),
                });
            }
            return Ok(Json(ListMachineJsonResp { machines }));
        }
        let devices = network_service
            .db
            .list_devices(user_id)
            .await
            .map_err(convert_db_error)?;
        let mut device_networks = network_service
            .list_device_networks(user_id)
            .await
            .map_err(super::central_network::convert_error)?;

        let mut machines = Vec::with_capacity(devices.len());
        for device in devices {
            let machine_id = uuid::Uuid::parse_str(&device.machine_id).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    other_error(format!(
                        "invalid device id in registry: {}",
                        device.machine_id
                    ))
                    .into(),
                )
            })?;
            let session = client_mgr.get_session_by_machine_id(user_id, &machine_id);
            let client_url = match &session {
                Some(session) => session.get_token().await.map(|token| token.client_url),
                None => device.client_url.parse::<url::Url>().ok(),
            };
            // Keep the saved address and location in one snapshot, even when a
            // newer connection exists but its metadata has not been saved yet.
            // Legacy Location requires a country; partial region/city results
            // remain available through the new nullable fields below.
            let location = device.ip_country.as_ref().map(|country| Location {
                country: country.clone(),
                region: device.ip_region.clone(),
                city: device.ip_city.clone(),
            });
            let online = session.is_some();
            let info = match session {
                Some(session) => session.get_heartbeat_req().await,
                None => Some(HeartbeatRequest {
                    machine_id: Some(machine_id.into()),
                    inst_id: None,
                    user_token: String::new(),
                    easytier_version: device.easytier_version.clone(),
                    report_time: device.last_seen_time.to_rfc3339(),
                    hostname: device.hostname.clone(),
                    running_network_instances: Vec::new(),
                    device_os: serde_json::from_str(&device.device_os).ok(),
                    support_config_source: false,
                    failed_network_instances: Vec::new(),
                    support_heartbeat_policy: false,
                    support_manual_routes_patch: false,
                }),
            };
            machines.push(ListMachineItem {
                client_url,
                alias: (!device.alias.is_empty()).then_some(device.alias),
                info,
                location,
                connection_ip: device.connection_ip,
                ip_country: device.ip_country,
                ip_region: device.ip_region,
                ip_city: device.ip_city,
                ip_location_status: device.ip_location_status,
                ip_observed_at: device.ip_observed_at.map(|time| time.to_rfc3339()),
                site_location: device.site_location,
                device_model: device.device_model,
                networks: device_networks
                    .remove(&device.machine_id)
                    .unwrap_or_default(),
                online,
                last_seen: Some(device.last_seen_time.to_rfc3339()),
            });
        }

        machines.sort_by(|a, b| {
            b.online
                .cmp(&a.online)
                .then_with(|| b.last_seen.cmp(&a.last_seen))
        });

        Ok(Json(ListMachineJsonResp { machines }))
    }

    async fn handle_update_network_state(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path((machine_id, inst_id)): Path<(uuid::Uuid, Option<uuid::Uuid>)>,
        Json(payload): Json<UpdateNetworkStateJsonReq>,
    ) -> Result<(), HttpHandleError> {
        let Some(inst_id) = inst_id else {
            // not implement disable all
            return Err((
                StatusCode::NOT_IMPLEMENTED,
                other_error("Not implemented".to_string()).into(),
            ));
        };

        let user_id = Self::get_user_id(&auth_session)?;
        let _mutation = network_service.lock_mutations().await;
        ensure_direct_mutation_allowed(&network_service, user_id, machine_id, inst_id, None)
            .await?;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        let result = client_mgr
            .handle_update_network_state((user_id, machine_id), inst_id, payload.disabled)
            .await;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        result.map_err(convert_error)?;
        Ok(())
    }

    async fn handle_get_network_metas(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path(machine_id): Path<uuid::Uuid>,
        Json(payload): Json<GetNetworkMetasJsonReq>,
    ) -> Result<Json<GetNetworkMetasResponse>, HttpHandleError> {
        Ok(Json(
            client_mgr
                .handle_get_network_metas(
                    (Self::get_user_id(&auth_session)?, machine_id),
                    payload.instance_ids,
                )
                .await
                .map_err(convert_error)?,
        ))
    }

    async fn handle_save_network_config(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path((machine_id, inst_id)): Path<(uuid::Uuid, uuid::Uuid)>,
        Json(payload): Json<SaveNetworkJsonReq>,
    ) -> Result<(), HttpHandleError> {
        if payload.config.instance_id() != inst_id.to_string() {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error("Instance ID mismatch".to_string()).into(),
            ));
        }
        let user_id = Self::get_user_id(&auth_session)?;
        let _mutation = network_service.lock_mutations().await;
        ensure_direct_mutation_allowed(
            &network_service,
            user_id,
            machine_id,
            inst_id,
            payload.config.network_name.as_deref(),
        )
        .await?;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        let result = client_mgr
            .handle_save_network_config_with_source(
                (user_id, machine_id),
                inst_id,
                payload.config,
                RuntimeConfigSource::Web,
            )
            .await;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        result.map_err(convert_error)?;
        Ok(())
    }

    async fn handle_get_network_config(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((machine_id, inst_id)): Path<(uuid::Uuid, uuid::Uuid)>,
    ) -> Result<Json<NetworkConfig>, HttpHandleError> {
        Ok(client_mgr
            .handle_get_network_config((auth_session.user.unwrap().id(), machine_id), inst_id)
            .await
            .map_err(convert_error)?
            .into())
    }

    async fn handle_patch_vpn_portal_clients(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Extension(network_service): Extension<Arc<CentralNetworkService>>,
        Path((machine_id, inst_id)): Path<(uuid::Uuid, uuid::Uuid)>,
        Json(payload): Json<PatchVpnPortalClientsJsonReq>,
    ) -> Result<(), HttpHandleError> {
        let user_id = Self::get_user_id(&auth_session)?;
        let _mutation = network_service.lock_mutations().await;
        ensure_direct_mutation_allowed(&network_service, user_id, machine_id, inst_id, None)
            .await?;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        let result = client_mgr
            .handle_patch_vpn_portal_clients((user_id, machine_id), inst_id, payload.patches)
            .await;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        result.map_err(convert_error)
    }

    // --- Token-authenticated machine-scoped handlers (no AuthSession) ---

    async fn handle_run_network_instance_internal(
        State(client_mgr): AppState,
        Path((user_id, machine_id)): Path<(UserIdInDb, uuid::Uuid)>,
        Json(payload): Json<RunNetworkJsonReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        let source = payload
            .source
            .and_then(config_source_from_rpc)
            .unwrap_or(RuntimeConfigSource::Web);
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        let result = client_mgr
            .handle_run_network_instance_with_source(
                (user_id, machine_id),
                payload.config,
                payload.save,
                source,
            )
            .await;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        result.map_err(convert_error)?;
        Ok(Void::default().into())
    }

    async fn handle_remove_network_instance_internal(
        State(client_mgr): AppState,
        Path((user_id, machine_id, inst_id)): Path<(UserIdInDb, uuid::Uuid, uuid::Uuid)>,
    ) -> Result<(), HttpHandleError> {
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        let result = client_mgr
            .handle_remove_network_instances((user_id, machine_id), vec![inst_id])
            .await;
        client_mgr
            .invalidate_applied_config_revision(user_id, machine_id)
            .await;
        result.map_err(convert_error)?;
        Ok(())
    }

    async fn handle_reconcile_managed_network_configs_internal(
        State(client_mgr): AppState,
        Path((user_id, machine_id)): Path<(UserIdInDb, uuid::Uuid)>,
        Json(payload): Json<ReconcileManagedNetworkConfigsJsonReq>,
    ) -> Result<StatusCode, HttpHandleError> {
        let desired = payload
            .managed_network_configs
            .into_iter()
            .map(|item| crate::webhook::ManagedNetworkConfig {
                instance_id: item.instance_id.to_string(),
                network_config: item.network_config,
            })
            .collect();
        client_mgr
            .reconcile_managed_network_configs(
                user_id,
                machine_id,
                desired,
                payload.config_revision,
                payload.expected_config_revision,
            )
            .await
            .map_err(Self::convert_managed_config_error)?;
        Ok(StatusCode::NO_CONTENT)
    }

    async fn handle_patch_managed_network_configs_internal(
        State(client_mgr): AppState,
        Path((user_id, machine_id)): Path<(UserIdInDb, uuid::Uuid)>,
        Json(payload): Json<PatchManagedNetworkConfigsJsonReq>,
    ) -> Result<StatusCode, HttpHandleError> {
        let upserts = payload
            .upserts
            .into_iter()
            .map(|item| crate::webhook::ManagedNetworkConfig {
                instance_id: item.instance_id.to_string(),
                network_config: item.network_config,
            })
            .collect();
        client_mgr
            .patch_managed_network_configs(
                user_id,
                machine_id,
                upserts,
                payload.delete_instance_ids,
                payload.config_revision,
                payload.expected_config_revision,
            )
            .await
            .map_err(Self::convert_managed_config_error)?;
        Ok(StatusCode::NO_CONTENT)
    }

    async fn handle_list_network_instance_ids_internal(
        State(client_mgr): AppState,
        Path((user_id, machine_id)): Path<(UserIdInDb, uuid::Uuid)>,
    ) -> Result<Json<ListNetworkInstanceIdsJsonResp>, HttpHandleError> {
        Ok(client_mgr
            .handle_list_network_instance_ids((user_id, machine_id))
            .await
            .map_err(convert_error)?
            .into())
    }

    async fn handle_collect_network_info_internal(
        State(client_mgr): AppState,
        Path((user_id, machine_id)): Path<(UserIdInDb, uuid::Uuid)>,
        Json(payload): Json<CollectNetworkInfoJsonReq>,
    ) -> Result<Json<CollectNetworkInfoResponse>, HttpHandleError> {
        Ok(client_mgr
            .handle_collect_network_info((user_id, machine_id), payload.inst_ids)
            .await
            .map_err(convert_error)?
            .into())
    }

    pub fn build_route_internal() -> Router<AppStateInner> {
        Router::new()
            .route(
                "/api/internal/users/{user-id}/machines/{machine-id}/networks",
                put(Self::handle_reconcile_managed_network_configs_internal)
                    .patch(Self::handle_patch_managed_network_configs_internal)
                    .layer(DefaultBodyLimit::max(MAX_MANAGED_CONFIG_REQUEST_BODY_SIZE))
                    .post(Self::handle_run_network_instance_internal)
                    .get(Self::handle_list_network_instance_ids_internal),
            )
            .route(
                "/api/internal/users/{user-id}/machines/{machine-id}/networks/{inst-id}",
                delete(Self::handle_remove_network_instance_internal),
            )
            .route(
                "/api/internal/users/{user-id}/machines/{machine-id}/networks/info",
                get(Self::handle_collect_network_info_internal),
            )
    }

    pub fn build_route() -> Router<AppStateInner> {
        Router::new()
            .route("/api/v1/machines", get(Self::handle_list_machines))
            .route(
                "/api/v1/machines/{machine-id}",
                delete(Self::handle_delete_machine),
            )
            .route(
                "/api/v1/machines/{machine-id}/alias",
                put(Self::handle_update_machine_alias),
            )
            .route(
                "/api/v1/machines/{machine-id}/attributes",
                put(Self::handle_update_machine_attributes),
            )
            .route(
                "/api/v1/blocked-devices",
                get(Self::handle_list_blocked_devices),
            )
            .route(
                "/api/v1/blocked-devices/{machine-id}",
                delete(Self::handle_unblock_device),
            )
            .route(
                "/api/v1/machines/{machine-id}/validate-config",
                post(Self::handle_validate_config),
            )
            .route(
                "/api/v1/machines/{machine-id}/networks",
                post(Self::handle_run_network_instance).get(Self::handle_list_network_instance_ids),
            )
            .route(
                "/api/v1/machines/{machine-id}/networks/{inst-id}",
                delete(Self::handle_remove_network_instance).put(Self::handle_update_network_state),
            )
            .route(
                "/api/v1/machines/{machine-id}/networks/info",
                get(Self::handle_collect_network_info),
            )
            .route(
                "/api/v1/machines/{machine-id}/networks/info/{inst-id}",
                get(Self::handle_collect_one_network_info),
            )
            .route(
                "/api/v1/machines/{machine-id}/networks/config/{inst-id}",
                get(Self::handle_get_network_config).put(Self::handle_save_network_config),
            )
            .route(
                "/api/v1/machines/{machine-id}/networks/{inst-id}/vpn-portal-clients",
                axum::routing::patch(Self::handle_patch_vpn_portal_clients),
            )
            .route(
                "/api/v1/machines/{machine-id}/networks/metas",
                post(Self::handle_get_network_metas),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn attributes_request(
        db: crate::db::Db,
        user: Option<crate::db::entity::users::Model>,
        machine_id: uuid::Uuid,
        webhook_auth: bool,
        payload: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        use tower::ServiceExt as _;

        let manager = Arc::new(crate::client_manager::ClientManager::new(
            db.clone(),
            None,
            crate::client_manager::HeartbeatPolicy::from_millis(1_000, 10_000).unwrap(),
            Arc::new(crate::FeatureFlags::default()),
            Arc::new(crate::webhook::WebhookConfig::new(
                None, None, None, None, None,
            )),
        ));
        let service = Arc::new(CentralNetworkService::new(db.clone(), manager.clone()));
        let session_store = tower_sessions_sqlx_store::SqliteStore::new(db.inner());
        session_store.migrate().await.unwrap();
        let auth_layer = axum_login::AuthManagerLayerBuilder::new(
            super::super::users::Backend::new(db),
            axum_login::tower_sessions::SessionManagerLayer::new(session_store),
        )
        .build();
        let response = NetworkApi::build_route()
            .route_layer(axum_login::login_required!(super::super::users::Backend))
            .layer(axum::middleware::from_fn(
                move |mut request: axum::extract::Request, next: axum::middleware::Next| {
                    let user = user.clone();
                    async move {
                        request
                            .extensions_mut()
                            .get_mut::<AuthSession>()
                            .unwrap()
                            .user = user.map(|db_user| super::super::users::User {
                            db_user,
                            tokens: vec![],
                        });
                        next.run(request).await
                    }
                },
            ))
            .layer(Extension(super::super::ConsoleInfoConfig {
                config_server_protocol: "tcp".into(),
                config_server_port: 22020,
                webhook_auth,
            }))
            .layer(Extension(service))
            .layer(auth_layer)
            .with_state(manager)
            .oneshot(
                axum::http::Request::put(format!("/api/v1/machines/{machine_id}/attributes"))
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        (
            status,
            if body.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&body).unwrap()
            },
        )
    }

    async fn register_attribute_device(db: &crate::db::Db, user_id: i32, machine_id: uuid::Uuid) {
        db.upsert_device_heartbeat(crate::db::DeviceHeartbeatRecord {
            user_id,
            machine_id,
            hostname: "attributes-router".into(),
            easytier_version: "test".into(),
            device_os: "{}".into(),
            client_url: "tcp://192.0.2.1:1000".into(),
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn machine_attributes_save_trim_clear_and_list() {
        let db = crate::db::Db::memory_db().await;
        let user = db.auto_create_user("attributes-api-save").await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        register_attribute_device(&db, user.id, machine_id).await;
        let (status, body) = attributes_request(db.clone(), Some(user.clone()), machine_id, false,
            serde_json::json!({"site_location": "  家中 机柜  ", "device_model": "  OpenWrt R4S  "}),
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({"site_location":"家中 机柜", "device_model":"OpenWrt R4S"})
        );
        let listed = central_machine_list(db.clone(), user.clone()).await;
        assert_eq!(listed["machines"][0]["site_location"], "家中 机柜");
        assert_eq!(listed["machines"][0]["device_model"], "OpenWrt R4S");

        // Limits count Unicode characters, rather than UTF-8 bytes.
        let boundary =
            serde_json::json!({"site_location": "地".repeat(256), "device_model":"型".repeat(128)});
        let (status, body) = attributes_request(
            db.clone(),
            Some(user.clone()),
            machine_id,
            false,
            boundary.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, boundary);
        let (status, body) = attributes_request(
            db.clone(),
            Some(user.clone()),
            machine_id,
            false,
            serde_json::json!({"site_location": "  ", "device_model": ""}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({"site_location":null, "device_model":null})
        );
        let row = db.get_device((user.id, machine_id)).await.unwrap().unwrap();
        assert_eq!((row.site_location, row.device_model), (None, None));
    }

    #[tokio::test]
    async fn machine_attributes_reject_invalid_or_read_only_fields_without_saving() {
        let db = crate::db::Db::memory_db().await;
        let user = db
            .auto_create_user("attributes-api-validation")
            .await
            .unwrap();
        let machine_id = uuid::Uuid::new_v4();
        register_attribute_device(&db, user.id, machine_id).await;
        db.set_device_attributes((user.id, machine_id), Some("Original"), Some("Router"))
            .await
            .unwrap();
        let before = db.get_device((user.id, machine_id)).await.unwrap().unwrap();
        let mut invalid = vec![
            serde_json::json!({"site_location":"地".repeat(257), "device_model":"Router"}),
            serde_json::json!({"site_location":"Office", "device_model":"型".repeat(129)}),
            serde_json::json!({"site_location":"Office\n", "device_model":"Router"}),
            serde_json::json!({"site_location":"Office", "device_model":"\u{0}"}),
            serde_json::json!({"site_location":"\t", "device_model":"Router"}),
            serde_json::json!({"site_location":"Office"}),
            serde_json::json!({"site_location":null, "device_model":"Router"}),
        ];
        for field in [
            "connection_ip",
            "ip_country",
            "ip_region",
            "ip_city",
            "ip_location_status",
            "ip_observed_at",
            "alias",
            "user_id",
        ] {
            let mut payload =
                serde_json::json!({"site_location":"Office", "device_model":"Router"});
            payload[field] = serde_json::json!("forged");
            invalid.push(payload);
        }
        for payload in invalid {
            let (status, body) = attributes_request(
                db.clone(),
                Some(user.clone()),
                machine_id,
                false,
                payload.clone(),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{payload}: {body}");
            assert!(
                body["message"]
                    .as_str()
                    .is_some_and(|message| !message.is_empty())
            );
            assert_eq!(
                db.get_device((user.id, machine_id)).await.unwrap().unwrap(),
                before
            );
        }
    }

    #[tokio::test]
    async fn machine_attributes_require_owner_and_central_authentication() {
        let db = crate::db::Db::memory_db().await;
        let owner = db.auto_create_user("attributes-api-owner").await.unwrap();
        let other = db.auto_create_user("attributes-api-other").await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        register_attribute_device(&db, owner.id, machine_id).await;
        let before = db
            .get_device((owner.id, machine_id))
            .await
            .unwrap()
            .unwrap();
        let payload = serde_json::json!({"site_location":"Office", "device_model":"Router"});
        for (user, target_id, external, expected) in [
            (None, machine_id, false, StatusCode::UNAUTHORIZED),
            (Some(other), machine_id, false, StatusCode::NOT_FOUND),
            (
                Some(owner.clone()),
                uuid::Uuid::new_v4(),
                false,
                StatusCode::NOT_FOUND,
            ),
            (Some(owner.clone()), machine_id, true, StatusCode::FORBIDDEN),
        ] {
            let (status, _) =
                attributes_request(db.clone(), user, target_id, external, payload.clone()).await;
            assert_eq!(status, expected);
        }
        assert_eq!(
            db.get_device((owner.id, machine_id))
                .await
                .unwrap()
                .unwrap(),
            before
        );
        assert_eq!(db.list_devices(owner.id).await.unwrap().len(), 1);
    }

    async fn central_machine_list(
        db: crate::db::Db,
        user: crate::db::entity::users::Model,
    ) -> serde_json::Value {
        use tower::ServiceExt as _;

        let manager = Arc::new(crate::client_manager::ClientManager::new(
            db.clone(),
            None,
            crate::client_manager::HeartbeatPolicy::from_millis(1_000, 10_000).unwrap(),
            Arc::new(crate::FeatureFlags::default()),
            Arc::new(crate::webhook::WebhookConfig::new(
                None, None, None, None, None,
            )),
        ));
        let service = Arc::new(CentralNetworkService::new(db.clone(), manager.clone()));
        let session_store = tower_sessions_sqlx_store::SqliteStore::new(db.inner());
        session_store.migrate().await.unwrap();
        let auth_layer = axum_login::AuthManagerLayerBuilder::new(
            super::super::users::Backend::new(db),
            axum_login::tower_sessions::SessionManagerLayer::new(session_store),
        )
        .build();
        let response = Router::new()
            .route(
                "/api/v1/machines",
                get(move |mut auth: AuthSession| async move {
                    auth.user = Some(super::super::users::User {
                        db_user: user,
                        tokens: vec![],
                    });
                    NetworkApi::handle_list_machines(
                        auth,
                        State(manager),
                        Extension(super::super::ConsoleInfoConfig {
                            config_server_protocol: "tcp".into(),
                            config_server_port: 22020,
                            webhook_auth: false,
                        }),
                        Extension(service),
                    )
                    .await
                }),
            )
            .layer(auth_layer)
            .oneshot(
                axum::http::Request::get("/api/v1/machines")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn machine_list_keeps_offline_ip_snapshots_scoped_to_the_logged_in_user() {
        use crate::db::{Db, DeviceConnectionObservation, DeviceHeartbeatRecord, IpLocationStatus};

        let db = Db::memory_db().await;
        let owner = db.auto_create_user("ip-list-owner").await.unwrap();
        let other = db.auto_create_user("ip-list-other").await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        let legacy_id = uuid::Uuid::new_v4();
        let observed_at =
            chrono::DateTime::parse_from_rfc3339("2026-10-22T10:20:30+08:00").unwrap();
        for (user_id, id, hostname) in [
            (owner.id, machine_id, "owner-node"),
            (other.id, machine_id, "other-node"),
            (owner.id, legacy_id, "legacy-node"),
        ] {
            db.upsert_device_heartbeat(DeviceHeartbeatRecord {
                user_id,
                machine_id: id,
                hostname: hostname.to_owned(),
                easytier_version: "2.7.0".to_owned(),
                device_os: "{}".to_owned(),
                client_url: "tcp://192.0.2.99:1000".to_owned(),
            })
            .await
            .unwrap();
        }
        for (user_id, ip, country) in [
            (owner.id, "2001:db8::1", "Country A"),
            (other.id, "192.0.2.2", "Country B"),
        ] {
            assert!(
                db.update_device_connection_observation(
                    (user_id, machine_id),
                    &DeviceConnectionObservation {
                        connection_ip: ip.to_owned(),
                        ip_country: Some(country.to_owned()),
                        ip_region: Some("Region".to_owned()),
                        ip_city: Some("City".to_owned()),
                        ip_location_status: IpLocationStatus::Resolved,
                        ip_observed_at: observed_at,
                    },
                )
                .await
                .unwrap()
            );
        }

        let response = central_machine_list(db, owner).await;
        let machines = response["machines"].as_array().unwrap();
        assert_eq!(machines.len(), 2);
        let snapshot = machines
            .iter()
            .find(|item| item["info"]["hostname"] == "owner-node")
            .unwrap();
        assert_eq!(snapshot["online"], false);
        assert_eq!(snapshot["connection_ip"], "2001:db8::1");
        assert_eq!(snapshot["ip_country"], "Country A");
        assert_eq!(snapshot["ip_location_status"], "resolved");
        assert_eq!(snapshot["location"]["country"], "Country A");
        assert_eq!(snapshot["location"]["city"], "City");
        let returned_time =
            chrono::DateTime::parse_from_rfc3339(snapshot["ip_observed_at"].as_str().unwrap())
                .unwrap();
        assert_eq!(returned_time, observed_at);
        assert_eq!(snapshot["client_url"], "tcp://192.0.2.99:1000");
        // pbjson omits the default empty token; offline metadata must not
        // expose the account's config-server credential.
        assert!(snapshot["info"].get("user_token").is_none());
        let legacy = machines
            .iter()
            .find(|item| item["info"]["hostname"] == "legacy-node")
            .unwrap();
        assert!(legacy["connection_ip"].is_null());
        assert!(legacy["location"].is_null());
        assert!(legacy["ip_observed_at"].is_null());
        assert!(legacy["site_location"].is_null());
        assert!(legacy["device_model"].is_null());
        assert_eq!(legacy["ip_location_status"], "unknown");
    }

    #[tokio::test]
    async fn machine_list_preserves_partial_location_without_fabricating_a_country() {
        use crate::db::{Db, DeviceConnectionObservation, DeviceHeartbeatRecord, IpLocationStatus};

        let db = Db::memory_db().await;
        let user = db.auto_create_user("ip-list-partial").await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id: user.id,
            machine_id,
            hostname: "partial-node".into(),
            easytier_version: "2.7.0".into(),
            device_os: "{}".into(),
            client_url: String::new(),
        })
        .await
        .unwrap();
        db.update_device_connection_observation(
            (user.id, machine_id),
            &DeviceConnectionObservation {
                connection_ip: "192.0.2.1".into(),
                ip_country: None,
                ip_region: Some("Region only".into()),
                ip_city: None,
                ip_location_status: IpLocationStatus::Resolved,
                ip_observed_at: chrono::Utc::now().fixed_offset(),
            },
        )
        .await
        .unwrap();
        let response = central_machine_list(db, user).await;
        let item = &response["machines"][0];
        assert_eq!(item["ip_region"], "Region only");
        assert_eq!(item["ip_location_status"], "resolved");
        assert!(item["ip_country"].is_null());
        assert!(item["location"].is_null());
    }

    #[test]
    fn machine_list_can_deserialize_responses_without_ip_snapshot_fields() {
        let item: ListMachineItem = serde_json::from_value(serde_json::json!({
            "client_url": null, "info": null, "location": null,
        }))
        .unwrap();
        assert!(item.connection_ip.is_none());
        assert!(item.ip_observed_at.is_none());
        assert!(item.site_location.is_none());
        assert!(item.device_model.is_none());
        assert_eq!(item.ip_location_status, "unknown");
    }

    #[tokio::test]
    async fn internal_runtime_only_write_reconciles_the_already_applied_revision() {
        use std::{future::Future, time::Duration};

        use easytier::{
            common::config::NetworkConfigExt as _, instance::factory::native_instance_manager,
            web_client::WebClient,
        };
        use easytier_core::{
            connectivity::protocol::raw::TunnelDialer,
            socket::SocketListener,
            tunnel::{
                Tunnel,
                ring::{RING_TUNNEL_CAP, RingTunnel, create_ring_socket_pair},
            },
        };
        use tower::ServiceExt as _;

        use crate::{
            client_manager::{ClientManager, HeartbeatPolicy},
            db::Db,
            webhook::{
                ManagedNetworkConfig, ValidateTokenRequest, ValidateTokenResponse, WebhookConfig,
                WebhookHandler,
            },
        };

        // Exercise the actual REST, Web RPC, and reconcile paths without an
        // operating-system listener or a test-only ClientManager interface.
        #[derive(Debug)]
        struct Listener(tokio::sync::mpsc::UnboundedReceiver<Box<dyn Tunnel>>);
        #[async_trait::async_trait]
        impl SocketListener for Listener {
            type Accepted = Box<dyn Tunnel>;
            async fn listen(&mut self) -> anyhow::Result<()> {
                Ok(())
            }
            async fn accept(&mut self) -> anyhow::Result<Self::Accepted> {
                self.0
                    .recv()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("test listener closed"))
            }
            fn local_url(&self) -> url::Url {
                "ring://server".parse().unwrap()
            }
        }
        struct Dialer(tokio::sync::mpsc::UnboundedSender<Box<dyn Tunnel>>);
        #[async_trait::async_trait]
        impl TunnelDialer for Dialer {
            async fn connect(&self) -> anyhow::Result<Box<dyn Tunnel>> {
                let (server, client) = create_ring_socket_pair(RING_TUNNEL_CAP);
                let server = Box::new(RingTunnel::new(
                    server,
                    Some(easytier::proto::common::TunnelInfo {
                        tunnel_type: "ring".to_owned(),
                        local_addr: Some("ring://server".parse::<url::Url>().unwrap().into()),
                        remote_addr: Some(
                            format!("ring://{}", uuid::Uuid::new_v4())
                                .parse::<url::Url>()
                                .unwrap()
                                .into(),
                        ),
                        ..Default::default()
                    }),
                ));
                self.0
                    .send(server)
                    .map_err(|_| anyhow::anyhow!("test listener closed"))?;
                Ok(Box::new(RingTunnel::new(client, None)))
            }
            fn remote_url(&self) -> url::Url {
                "ring://server".parse().unwrap()
            }
        }
        #[derive(Debug, Default)]
        struct Webhook(std::sync::Mutex<Option<String>>);
        #[async_trait::async_trait]
        impl WebhookHandler for Webhook {
            async fn validate_token(
                &self,
                request: &ValidateTokenRequest,
            ) -> anyhow::Result<ValidateTokenResponse> {
                *self.0.lock().unwrap() = request.applied_config_revision.clone();
                Ok(ValidateTokenResponse {
                    valid: true,
                    pre_approved: true,
                    binding_version: 0,
                    config_revision: "revision-1".to_owned(),
                })
            }
        }
        async fn wait_until<F, Fut>(mut condition: F)
        where
            F: FnMut() -> Fut,
            Fut: Future<Output = bool>,
        {
            tokio::time::timeout(Duration::from_secs(20), async {
                while !condition().await {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("runtime did not converge");
        }

        let db = Db::memory_db().await;
        let db_user = db.auto_create_user("rest-revision").await.unwrap();
        let user_id = db_user.id;
        let machine_id = uuid::Uuid::new_v4();
        let instance_id = uuid::Uuid::new_v4();
        let webhook = Arc::new(Webhook::default());
        let mut manager = ClientManager::new(
            db.clone(),
            None,
            HeartbeatPolicy::from_millis(1_000, 10_000).unwrap(),
            Arc::new(crate::FeatureFlags::default()),
            Arc::new(
                WebhookConfig::new(None, None, None, None, None).with_handler(webhook.clone()),
            ),
        );
        let (connections, listener) = tokio::sync::mpsc::unbounded_channel();
        manager.add_listener(Listener(listener)).await.unwrap();
        let manager = Arc::new(manager);
        let core = Arc::new(native_instance_manager());
        let client = WebClient::new(
            Dialer(connections),
            "rest-revision",
            machine_id,
            "test-device",
            false,
            core.clone(),
            None,
        );
        let desired = serde_json::json!({
            "instance_id": instance_id.to_string(),
            "network_name": "managed-network", "network_secret": "secret",
            "networking_method": "Standalone", "no_tun": true,
            "disable_ipv6": true, "multi_thread": false,
        });
        manager
            .reconcile_managed_network_configs(
                user_id,
                machine_id,
                vec![ManagedNetworkConfig {
                    instance_id: instance_id.to_string(),
                    network_config: desired.clone(),
                }],
                Some("revision-1".to_owned()),
                None,
            )
            .await
            .unwrap();
        wait_until(|| async { webhook.0.lock().unwrap().as_deref() == Some("revision-1") }).await;

        // External Console authentication does not populate central devices.
        // Both public views must still expose the live, webhook-managed Core.
        assert!(db.list_devices(user_id).await.unwrap().is_empty());
        let service = Arc::new(CentralNetworkService::new(db.clone(), manager.clone()));
        let session_store = tower_sessions_sqlx_store::SqliteStore::new(db.inner());
        session_store.migrate().await.unwrap();
        let session_layer = axum_login::tower_sessions::SessionManagerLayer::new(session_store);
        let auth_layer = axum_login::AuthManagerLayerBuilder::new(
            super::super::users::Backend::new(db.clone()),
            session_layer,
        )
        .build();
        let probe_manager = manager.clone();
        let response = Router::new()
            .route(
                "/views",
                get(move |mut auth: AuthSession| async move {
                    auth.user = Some(super::super::users::User {
                        db_user,
                        tokens: vec![],
                    });
                    let config = super::super::ConsoleInfoConfig {
                        config_server_protocol: "tcp".into(),
                        config_server_port: 22020,
                        webhook_auth: true,
                    };
                    let machines = NetworkApi::handle_list_machines(
                        auth.clone(),
                        State(probe_manager.clone()),
                        Extension(config.clone()),
                        Extension(service.clone()),
                    )
                    .await
                    .unwrap()
                    .0;
                    let summary = super::super::RestfulServer::handle_get_summary(
                        auth,
                        State(probe_manager),
                        Extension(config),
                        Extension(service),
                    )
                    .await
                    .unwrap()
                    .0;
                    assert_eq!(summary.device_count, 1);
                    assert_eq!(machines.machines.len(), 1);
                    assert!(machines.machines[0].online);
                    assert!(machines.machines[0].connection_ip.is_none());
                    assert!(machines.machines[0].ip_observed_at.is_none());
                    assert!(machines.machines[0].site_location.is_none());
                    assert!(machines.machines[0].device_model.is_none());
                    assert_eq!(machines.machines[0].ip_location_status, "unknown");
                    assert_eq!(
                        machines.machines[0].info.as_ref().unwrap().machine_id,
                        Some(machine_id.into())
                    );
                    StatusCode::OK
                }),
            )
            .layer(auth_layer)
            .oneshot(
                axum::http::Request::get("/views")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let mut drifted = desired;
        drifted["network_name"] = serde_json::json!("runtime-only-change");
        let response = NetworkApi::build_route_internal()
            .with_state(manager.clone())
            .oneshot(
                axum::http::Request::post(format!(
                    "/api/internal/users/{user_id}/machines/{machine_id}/networks"
                ))
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({"config": drifted, "save": false}).to_string(),
                ))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        wait_until(|| async {
            core.config(instance_id)
                .and_then(|config| NetworkConfig::new_from_config(&config).ok())
                .is_some_and(|config| config.network_name.as_deref() == Some("managed-network"))
        })
        .await;
        assert_eq!(
            db.get_managed_config_revision((user_id, machine_id))
                .await
                .unwrap()
                .as_deref(),
            Some("revision-1")
        );
        drop(client);
        manager
            .disconnect_session_by_machine_id(user_id, &machine_id)
            .await;
        core.delete_network_instances([instance_id]).await.unwrap();
    }

    #[test]
    fn blocked_device_response_keeps_the_public_id_field() {
        let machine_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().fixed_offset();
        let item = BlockedDeviceItem::from(crate::db::entity::blocked_devices::Model {
            user_id: 7,
            machine_id: machine_id.clone(),
            hostname: "blocked-device".to_owned(),
            blocked_time: now,
            attempt_count: 3,
            last_attempt_time: Some(now),
        });

        let value = serde_json::to_value(item).unwrap();
        assert_eq!(value["id"], machine_id);
        assert!(value.get("machine_id").is_none());
    }

    #[test]
    fn revision_conflict_response_exposes_machine_readable_current_revision() {
        let error = crate::client_manager::ManagedConfigError::RevisionConflict {
            expected: Some("rev-1".to_string()),
            current: Some("rev-2".to_string()),
        };

        let (status, Json(body)) = NetworkApi::convert_managed_config_error(error.into());

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            body.code.as_deref(),
            Some("managed_config_revision_conflict")
        );
        assert_eq!(body.current_config_revision.as_deref(), Some("rev-2"));
    }

    #[test]
    fn ownership_conflict_is_distinct_from_revision_conflict() {
        let error = crate::client_manager::ManagedConfigError::OwnershipConflict {
            instance_id: uuid::Uuid::new_v4(),
        };

        let (status, Json(body)) = NetworkApi::convert_managed_config_error(error.into());

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            body.code.as_deref(),
            Some("managed_config_ownership_conflict")
        );
        assert_eq!(body.current_config_revision, None);
    }
}
