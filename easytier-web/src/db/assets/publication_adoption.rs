//! Adopt a writable User network only inside an explicitly authorized service
//! save transaction. Cleanup and inherited address changes never adopt networks.
use super::*;

pub(super) async fn ensure_web_config(
    connection: &mut SqliteConnection,
    owner: i32,
    machine: &str,
    instance: &str,
    config: &mut StoredConfig,
    proof: Option<&PublicationRuntimeProof>,
) -> AssetResult<()> {
    if config.source != "user" {
        return mutable_config(connection, owner, machine, instance, config).await;
    }
    if !proof.is_some_and(|proof| proof.owner_user_id == owner
        && proof.machine_id == machine && proof.instance_id == instance)
    {
        return Err(publication_conflict("publication_runtime_required", "instance_id",
            "Choose a running, writable User or Web network on the selected public server and retry."));
    }
    if config.disabled {
        return Err(publication_conflict("publication_network_disabled", "instance_id",
            "This network is disabled. Start it before binding a service."));
    }
    // Validate the would-be Web configuration first. This rechecks the active
    // owner, instance identity and central ownership inside the transaction.
    let adopted = StoredConfig {
        id: config.id,
        created: config.created.clone(),
        source: "web".to_owned(),
        disabled: config.disabled,
        value: config.value.clone(),
    };
    mutable_config(connection, owner, machine, instance, &adopted).await?;
    let updated = sqlx::query(
        "UPDATE user_running_network_configs SET source = 'web', update_time = ? \
         WHERE id = ? AND user_id = ? AND device_id = ? AND network_instance_id = ? \
         AND create_time = ? AND source = 'user' AND disabled = 0",
    )
        .bind(chrono::Local::now().fixed_offset()).bind(config.id).bind(owner)
        .bind(machine).bind(instance).bind(&config.created)
        .execute(&mut *connection).await?;
    if updated.rows_affected() != 1 {
        return Err(publication_conflict("publication_config_changed", "instance_id",
            "The network configuration changed. Refresh and retry."));
    }
    config.source = "web".to_owned();
    // The caller writes mappings and invalidates managed revisions in this
    // same transaction. A port conflict or any later failure rolls adoption back.
    Ok(())
}

#[cfg(test)]
#[path = "publication_adoption_tests.rs"]
mod tests;
