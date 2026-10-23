//! Request-scoped runtime evidence for service publication.
//! Callers authorize the account before entering this module. Configuration
//! writes remain in the asset transaction; this module never publishes by RPC.

use std::{collections::{BTreeMap, BTreeSet}, sync::Arc, time::Duration};

use easytier::{
    common::config::NetworkConfig,
    proto::{
        api::manage::{CollectNetworkInfoRequest, ConfigSource, ListNetworkInstanceMetaRequest, NetworkMeta},
        rpc_types::controller::BaseController,
    },
};
use easytier_core::management::ConfigFilePermission;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{central_network::service::CentralNetworkService, db::assets::{AssetError, PublicationRuntimeProof}};

use super::{AppStateInner, HttpResult, asset_error, database_error};

struct RunningInstance {
    id: Uuid,
    hostname: Option<String>,
    metadata: Option<NetworkMeta>,
}

struct SavedInstance {
    config: Option<NetworkConfig>,
    source: String,
    disabled: bool,
}

fn publication_error(code: &str, field: &str, message: &str) -> super::HttpError {
    let mut error = AssetError::conflict(code, message);
    error.field_errors.insert(field.to_owned(), message.to_owned());
    asset_error(error)
}

fn parse_id(value: &str, field: &str) -> HttpResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|_| asset_error(AssetError::field(field, "Choose a valid identifier.")))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

async fn ensure_public_server(service: &CentralNetworkService, owner: i32, machine: Uuid) -> HttpResult<()> {
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM devices d \
         JOIN users u ON u.id = d.user_id \
         JOIN node_asset_metadata m ON m.owner_user_id = d.user_id AND m.machine_id = d.machine_id \
         WHERE d.user_id = ? AND d.machine_id = ? AND u.deleted_at IS NULL AND m.is_public_server = 1)",
    )
        .bind(owner).bind(machine.to_string()).fetch_one(&service.db.inner()).await.map_err(database_error)?;
    if !available {
        return Err(publication_error("publication_server_unavailable", "publication.machine_id",
            "Choose a public server registered to this active account."));
    }
    Ok(())
}

async fn running_instances(
    manager: &AppStateInner,
    owner: i32,
    machine: Uuid,
    selected: Option<Uuid>,
) -> HttpResult<Vec<RunningInstance>> {
    let session = manager.get_session_by_machine_id(owner, &machine)
        .ok_or_else(|| publication_error("publication_server_offline", "publication.machine_id",
            "The public server is offline. Reconnect it before publishing a service."))?;
    // The machine route and Session map are separate snapshots. A reused
    // transport URL must not lend another account's runtime to this request.
    if !session.get_token().await.is_some_and(|token| token.user_id == owner && token.machine_id == machine) {
        return Err(publication_error("publication_session_changed", "publication.machine_id",
            "The public server's session changed. Refresh and retry."));
    }
    let rpc = session.scoped_rpc_client();
    // Keep both reads on the captured Session, and own the RPC client in the
    // future: the boxed service is Send but need not be Sync.
    let observed = tokio::time::timeout(Duration::from_secs(2), async move {
        let response = rpc.collect_network_info(BaseController::default(), CollectNetworkInfoRequest {
            inst_ids: selected.into_iter().map(Into::into).collect(),
        }).await?;
        let info = response.info.ok_or_else(|| anyhow::anyhow!("runtime observation missing"))?;
        let mut running = BTreeMap::new();
        for (id, info) in info.map {
            let Ok(id) = Uuid::parse_str(&id) else { continue; };
            if !info.running || selected.is_some_and(|selected| selected != id) { continue; }
            let hostname = info.my_node_info.and_then(|node| nonempty(Some(&node.hostname)).map(str::to_owned));
            running.insert(id, RunningInstance { id, hostname, metadata: None });
        }
        if !running.is_empty() {
            let response = rpc.list_network_instance_meta(BaseController::default(), ListNetworkInstanceMetaRequest {
                inst_ids: running.keys().copied().map(Into::into).collect(),
            }).await?;
            for metadata in response.metas {
                let Some(id) = metadata.inst_id.map(Uuid::from) else { continue; };
                if let Some(instance) = running.get_mut(&id) { instance.metadata = Some(metadata); }
            }
        }
        Ok::<_, anyhow::Error>(running.into_values().collect::<Vec<_>>())
    }).await
        .map_err(|_| publication_error("publication_runtime_timeout", "publication.instance_id",
            "Reading the public server's running networks timed out. Refresh and retry."))?
        .map_err(|_| publication_error("publication_runtime_unavailable", "publication.instance_id",
            "The running networks could not be read. Refresh and retry."))?;
    if !manager.get_session_by_machine_id(owner, &machine)
        .is_some_and(|current| Arc::ptr_eq(&current, &session))
        || !session.get_token().await.is_some_and(|token| token.user_id == owner && token.machine_id == machine)
    {
        return Err(publication_error("publication_session_changed", "publication.machine_id",
            "The public server reconnected during this request. Refresh and retry."));
    }
    Ok(observed)
}

fn unavailable_reason(
    instance_id: Uuid,
    stored: Option<&SavedInstance>,
    metadata: Option<&NetworkMeta>,
    central_owned: bool,
) -> Option<(&'static str, &'static str)> {
    let config = stored.and_then(|stored| stored.config.as_ref());
    if central_owned {
        Some(("centrally_managed", "This network is centrally managed. Change its network intent instead."))
    } else if stored.is_none() {
        Some(("config_not_saved", "Save this running network configuration in the Web console before publishing a service."))
    } else if stored.is_some_and(|stored| !matches!(stored.source.as_str(), "user" | "web")) {
        Some(("saved_config_not_web", "This saved configuration cannot be managed by the Web console."))
    } else if config.is_none() {
        Some(("saved_config_invalid", "The saved network configuration is invalid. Correct and save it before publishing."))
    } else if config.and_then(|config| config.instance_id.as_deref())
        .and_then(|value| Uuid::parse_str(value).ok()) != Some(instance_id)
    {
        Some(("instance_id_mismatch", "The saved configuration does not identify this network instance. Correct and save it before publishing."))
    } else if stored.is_some_and(|stored| stored.disabled) {
        Some(("saved_config_stopped", "This network is saved as stopped. Start and save it before publishing a service."))
    } else if metadata.is_none() {
        Some(("runtime_metadata_missing", "The running network's configuration ownership could not be confirmed."))
    } else if metadata.is_some_and(|meta| meta.source != ConfigSource::Web as i32 && meta.source != ConfigSource::User as i32) {
        Some(("runtime_config_not_web", "This running network cannot be managed by the Web console."))
    } else if metadata.is_some_and(|meta| ConfigFilePermission::from(meta.config_permission)
        .has_flag(ConfigFilePermission::READ_ONLY))
    {
        Some(("runtime_config_read_only", "The running network configuration is read-only."))
    } else {
        // A writable User network is adopted only when the service transaction
        // commits. Listing candidates and cancelling the dialog remain read-only.
        None
    }
}

async fn candidates(
    manager: &AppStateInner,
    service: &CentralNetworkService,
    owner: i32,
    machine: Uuid,
    selected: Option<Uuid>,
) -> HttpResult<Vec<Value>> {
    ensure_public_server(service, owner, machine).await?;
    let running = running_instances(manager, owner, machine, selected).await?;
    let rows = sqlx::query(
        "SELECT network_instance_id, network_config, source, disabled FROM user_running_network_configs \
         WHERE user_id = ? AND device_id = ?",
    )
        .bind(owner).bind(machine.to_string()).fetch_all(&service.db.inner()).await.map_err(database_error)?;
    let mut saved = BTreeMap::new();
    for row in rows {
        let id: String = row.try_get("network_instance_id").map_err(database_error)?;
        let text: String = row.try_get("network_config").map_err(database_error)?;
        // Never return configuration text or parser errors: these can contain credentials.
        saved.insert(id, SavedInstance {
            config: serde_json::from_str(&text).ok(),
            source: row.try_get("source").map_err(database_error)?,
            disabled: row.try_get("disabled").map_err(database_error)?,
        });
    }
    let central = service.db.central_owned_runtime_configs(owner, machine).await.map_err(|_| {
        asset_error(AssetError::new(500, "asset_database_error", "Network ownership could not be loaded."))
    })?;
    let mut items = Vec::with_capacity(running.len());
    for instance in running {
        let id = instance.id.to_string();
        let stored = saved.get(&id);
        let config = stored.and_then(|stored| stored.config.as_ref());
        let metadata = instance.metadata.as_ref();
        let display_name = nonempty(instance.hostname.as_deref())
            .or_else(|| config.and_then(|config| nonempty(config.hostname.as_deref())))
            .unwrap_or(&id);
        let network_name = metadata.map(|meta| meta.network_name.as_str())
            .or_else(|| config.and_then(|config| config.network_name.as_deref())).unwrap_or_default();
        let central_owned = central.iter().any(|central| central.instance_id == instance.id
            || central.network_name == network_name
            || config.and_then(|config| config.network_name.as_deref()) == Some(central.network_name.as_str()));
        let unavailable = unavailable_reason(instance.id, stored, metadata, central_owned);
        items.push(json!({
            "instance_id":id,
            "display_name":display_name,
            "network_name":network_name,
            "selectable":unavailable.is_none(),
            "unavailable_code":unavailable.map(|(code, _)| code),
            "unavailable_reason":unavailable.map(|(_, reason)| reason),
        }));
    }
    Ok(items)
}

pub(super) async fn list_instances(
    manager: &AppStateInner,
    service: &CentralNetworkService,
    owner: i32,
    machine: &str,
) -> HttpResult<Value> {
    let machine = parse_id(machine, "publication.machine_id")?;
    let items = candidates(manager, service, owner, machine, None).await?;
    Ok(json!({"total":items.len(),"items":items}))
}

/// One-request proof only. The transaction must still recheck account, public
/// server, stored configuration identity/source and centrally managed ownership.
pub(super) async fn prove_instance(
    manager: &AppStateInner,
    service: &CentralNetworkService,
    owner: i32,
    machine: &str,
    instance: &str,
) -> HttpResult<PublicationRuntimeProof> {
    let machine = parse_id(machine, "publication.machine_id")?;
    let instance = parse_id(instance, "publication.instance_id")?;
    let instance_id = instance.to_string();
    let candidate = candidates(manager, service, owner, machine, Some(instance)).await?
        .into_iter().find(|candidate| candidate["instance_id"].as_str() == Some(instance_id.as_str()))
        .ok_or_else(|| publication_error("publication_instance_not_running", "publication.instance_id",
            "This network instance is no longer running. Refresh and choose a running network."))?;
    if candidate["selectable"] != true {
        return Err(publication_error("publication_instance_unavailable", "publication.instance_id",
            candidate["unavailable_reason"].as_str().unwrap_or("This network is unavailable for service publication.")));
    }
    Ok(PublicationRuntimeProof {
        owner_user_id: owner,
        machine_id: machine.to_string(),
        instance_id,
    })
}

/// After the transaction commits, fence cached runtime work and wake the
/// existing reconciler. Offline nodes retain desired changes for reconnection.
pub(super) async fn notify_targets(manager: &AppStateInner, owner: i32, machines: &[String]) {
    let machines: BTreeSet<Uuid> = machines.iter().filter_map(|machine| Uuid::parse_str(machine).ok()).collect();
    for machine in machines {
        manager.invalidate_applied_config_revision(owner, machine).await;
    }
}

#[cfg(test)]
#[path = "publication_tests.rs"]
mod tests;
