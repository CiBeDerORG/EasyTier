use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add binding metadata without guessing relationships for existing
        // assets or changing their addresses, identities, or services. The
        // configuration record identity is a snapshot, not a foreign key:
        // deleting an EasyTier instance must not delete or block its assets.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE asset_gateway_groups ADD COLUMN network_instance_id TEXT;
                ALTER TABLE asset_gateway_groups ADD COLUMN config_record_id INTEGER;
                ALTER TABLE asset_gateway_groups ADD COLUMN config_create_time TEXT;
                ALTER TABLE asset_gateway_groups ADD COLUMN config_fingerprint TEXT;
                ALTER TABLE asset_gateway_groups ADD COLUMN binding_snapshot_json TEXT;

                CREATE UNIQUE INDEX idx_asset_gateway_groups_instance
                    ON asset_gateway_groups(owner_user_id, machine_id, network_instance_id)
                    WHERE network_instance_id IS NOT NULL;

                ALTER TABLE asset_devices ADD COLUMN network_instance_id TEXT;
                ALTER TABLE asset_devices ADD COLUMN config_record_id INTEGER;
                ALTER TABLE asset_devices ADD COLUMN config_create_time TEXT;
                ALTER TABLE asset_devices ADD COLUMN config_fingerprint TEXT;
                ALTER TABLE asset_devices ADD COLUMN binding_snapshot_json TEXT;
                ALTER TABLE asset_devices ADD COLUMN address_mode TEXT NOT NULL
                    DEFAULT 'legacy_manual' CHECK (address_mode IN (
                        'legacy_manual', 'direct_instance', 'gateway_rule', 'gateway_self'
                    ));
                ALTER TABLE asset_devices ADD COLUMN rule_key TEXT;
                ALTER TABLE asset_devices ADD COLUMN host_offset INTEGER
                    CHECK (host_offset IS NULL OR host_offset BETWEEN 0 AND 4294967295);
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Explicit schema rollback discards binding provenance only. Existing
        // assets, their current addresses/subnets, and services are retained;
        // restoring a backup is required to recover the exact pre-upgrade data
        // or bindings written after upgrading.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                DROP INDEX idx_asset_gateway_groups_instance;

                ALTER TABLE asset_devices DROP COLUMN host_offset;
                ALTER TABLE asset_devices DROP COLUMN rule_key;
                ALTER TABLE asset_devices DROP COLUMN address_mode;
                ALTER TABLE asset_devices DROP COLUMN binding_snapshot_json;
                ALTER TABLE asset_devices DROP COLUMN config_fingerprint;
                ALTER TABLE asset_devices DROP COLUMN config_create_time;
                ALTER TABLE asset_devices DROP COLUMN config_record_id;
                ALTER TABLE asset_devices DROP COLUMN network_instance_id;

                ALTER TABLE asset_gateway_groups DROP COLUMN binding_snapshot_json;
                ALTER TABLE asset_gateway_groups DROP COLUMN config_fingerprint;
                ALTER TABLE asset_gateway_groups DROP COLUMN config_create_time;
                ALTER TABLE asset_gateway_groups DROP COLUMN config_record_id;
                ALTER TABLE asset_gateway_groups DROP COLUMN network_instance_id;
                "#,
            )
            .await?;
        Ok(())
    }
}
