use sea_orm_migration::prelude::*;

mod m20241029_000001_init;
mod m20260403_000002_scope_network_config_unique;
mod m20260421_000003_add_network_config_source;
mod m20260514_000004_rename_web_config_source;
mod m20260619_000005_managed_config_revisions;
mod m20260920_000007_device_registry;
mod m20260920_000009_central_network_intent;

mod m20261001_000010_member_ip_allocation;
mod m20261022_000011_node_attributes;
mod m20261023_000012_node_assets;
mod m20261023_000013_asset_instance_bindings;
mod m20261023_000014_user_soft_delete;
mod m20261023_000015_service_publications;
mod m20261023_000016_node_geo_locations;
mod m20261024_000011_network_space;
mod m20261024_000011_deploy_agents;
mod m20261024_000017_asset_service_navigation;
mod m20261024_000018_automation_center;
mod m20261024_000019_openwrt_discovery;
mod m20261024_000020_adb_service_events;
mod m20261024_000021_automation_settings;
mod m20261025_000022_public_node_ip;
mod m20261025_000023_adb_unlock_secrets;
mod m20261025_000024_device_type_semantic_kinds;
mod m20261027_000025_automation_drafts;
mod m20261027_000026_automation_draft_script_link;
mod m20261027_000027_asset_agent_bindings;
mod m20261028_000028_automation_run_concurrency;
mod m20261028_000029_automation_script_mutations;
mod m20261028_000040_agent_host_snapshots;
mod m20261028_000041_agent_adoption_intents;
mod m20261028_000042_agent_docker_snapshots;
mod m20261028_000043_agent_docker_absences;
mod m20261028_000044_deploy_catalog;
mod m20261029_000045_deploy_catalog_packages;
mod m20261029_000046_automation_adb_favorites;
mod m20261029_000047_automation_adb_preset_visibility;

#[cfg(test)]
mod merge_upgrade_tests;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20241029_000001_init::Migration),
            Box::new(m20260403_000002_scope_network_config_unique::Migration),
            Box::new(m20260421_000003_add_network_config_source::Migration),
            Box::new(m20260514_000004_rename_web_config_source::Migration),
            Box::new(m20260619_000005_managed_config_revisions::Migration),
            Box::new(m20260920_000007_device_registry::Migration),
            Box::new(m20260920_000009_central_network_intent::Migration),
            Box::new(m20261001_000010_member_ip_allocation::Migration),
            Box::new(m20261022_000011_node_attributes::Migration),
            Box::new(m20261023_000012_node_assets::Migration),
            Box::new(m20261023_000013_asset_instance_bindings::Migration),
            Box::new(m20261023_000014_user_soft_delete::Migration),
            Box::new(m20261023_000015_service_publications::Migration),
            Box::new(m20261023_000016_node_geo_locations::Migration),
            Box::new(m20261024_000011_network_space::Migration),
            Box::new(m20261024_000011_deploy_agents::Migration),
            Box::new(m20261024_000017_asset_service_navigation::Migration),
            Box::new(m20261024_000018_automation_center::Migration),
            Box::new(m20261024_000019_openwrt_discovery::Migration),
            Box::new(m20261024_000020_adb_service_events::Migration),
            Box::new(m20261024_000021_automation_settings::Migration),
            Box::new(m20261025_000022_public_node_ip::Migration),
            Box::new(m20261025_000023_adb_unlock_secrets::Migration),
            Box::new(m20261025_000024_device_type_semantic_kinds::Migration),
            Box::new(m20261027_000025_automation_drafts::Migration),
            Box::new(m20261027_000026_automation_draft_script_link::Migration),
            Box::new(m20261027_000027_asset_agent_bindings::Migration),
            Box::new(m20261028_000028_automation_run_concurrency::Migration),
            Box::new(m20261028_000029_automation_script_mutations::Migration),
            Box::new(m20261028_000040_agent_host_snapshots::Migration),
            Box::new(m20261028_000041_agent_adoption_intents::Migration),
            Box::new(m20261028_000042_agent_docker_snapshots::Migration),
            Box::new(m20261028_000043_agent_docker_absences::Migration),
            Box::new(m20261028_000044_deploy_catalog::Migration),
            Box::new(m20261029_000045_deploy_catalog_packages::Migration),
            Box::new(m20261029_000046_automation_adb_favorites::Migration),
            Box::new(m20261029_000047_automation_adb_preset_visibility::Migration),
        ]
    }
}
