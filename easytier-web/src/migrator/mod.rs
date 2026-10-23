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
        ]
    }
}
