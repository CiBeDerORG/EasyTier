//! Optional, request-scoped runtime observations for asset registration.
//! The saved configuration remains authoritative for identity and subnet rules.

use std::{net::Ipv4Addr, time::Duration};

use easytier_core::management::remote_client::RemoteClientManager as _;
use uuid::Uuid;

use crate::db::{Db, assets::{RuntimeObservation, RuntimeObservations}};

use super::AppStateInner;

/// Call only after checking asset scope permissions, before taking a mutation
/// lock or opening a write transaction. A failed optional observation must not
/// prevent reading saved configurations or maintaining existing asset metadata.
pub(super) async fn observe(
    manager: &AppStateInner,
    db: &Db,
    owner: i32,
    node: &str,
) -> RuntimeObservations {
    let mut observations = RuntimeObservations::new();
    let Ok(machine_id) = Uuid::parse_str(node) else {
        return observations;
    };
    if machine_id.is_nil() {
        return observations;
    }

    let Ok(instance_ids) = sqlx::query_scalar::<_, String>(
        "SELECT network_instance_id FROM user_running_network_configs \
         WHERE user_id = ? AND device_id = ?",
    )
    .bind(owner)
    .bind(node)
    .fetch_all(&db.inner())
    .await else {
        // The authoritative catalog reports database failures separately.
        return observations;
    };

    let online = manager.get_session_by_machine_id(owner, &machine_id).is_some();
    let mut requested_ids = Vec::new();
    for instance_id in instance_ids {
        if let Ok(id) = Uuid::parse_str(&instance_id) {
            requested_ids.push(id);
            observations.insert(id.to_string(), RuntimeObservation {
                ipv4: None,
                // An active node session alone does not prove this instance is running.
                online: if online { None } else { Some(false) },
            });
        }
    }
    if !online || requested_ids.is_empty() {
        return observations;
    }

    // One bounded read per explicit request; never a background probe or loop.
    let response = tokio::time::timeout(
        Duration::from_secs(2),
        manager.handle_collect_network_info((owner, machine_id), Some(requested_ids)),
    ).await;
    let Ok(Ok(response)) = response else {
        return observations;
    };
    let Some(info) = response.info else {
        return observations;
    };
    for (instance_id, info) in info.map {
        let Ok(id) = Uuid::parse_str(&instance_id) else { continue; };
        let Some(observation) = observations.get_mut(&id.to_string()) else { continue; };
        observation.online = Some(info.running);
        if !info.running {
            continue;
        }
        observation.ipv4 = info.my_node_info
            .and_then(|node| node.virtual_ipv4)
            .and_then(|address| address.address)
            .map(Ipv4Addr::from)
            .filter(|address| !address.is_unspecified())
            .map(|address| address.to_string());
    }
    observations
}
