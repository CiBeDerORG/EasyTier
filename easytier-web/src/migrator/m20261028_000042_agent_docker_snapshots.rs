use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            CREATE TABLE asset_agent_docker_snapshots (
                asset_device_id TEXT PRIMARY KEY NOT NULL
                    REFERENCES asset_devices(id) ON DELETE CASCADE,
                agent_id TEXT NOT NULL,
                snapshot_json TEXT NOT NULL,
                observed_at TEXT NOT NULL
            );
            "#,
        ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "DROP TABLE IF EXISTS asset_agent_docker_snapshots;"
        ).await?;
        Ok(())
    }
}
