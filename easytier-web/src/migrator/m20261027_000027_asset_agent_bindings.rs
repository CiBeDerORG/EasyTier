use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            -- Stable controller identity, never tied to a user session.
            CREATE TABLE asset_agent_controller (
                singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                controller_id TEXT NOT NULL
            );
            CREATE TABLE asset_agent_bindings (
                asset_device_id TEXT PRIMARY KEY NOT NULL
                    REFERENCES asset_devices(id) ON DELETE CASCADE,
                owner_user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                machine_id TEXT NOT NULL,
                agent_id TEXT NOT NULL UNIQUE,
                spki_sha256 TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                token_cipher TEXT NOT NULL,
                paired_at TEXT NOT NULL,
                verified_at TEXT NOT NULL,
                FOREIGN KEY(asset_device_id,owner_user_id,machine_id)
                    REFERENCES asset_devices(id,owner_user_id,machine_id) ON DELETE CASCADE
            );
            CREATE INDEX asset_agent_bindings_scope
                ON asset_agent_bindings(owner_user_id,machine_id);
        "#,
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent bindings contain live grants; automatic downgrade is prohibited".into(),
        ))
    }
}
