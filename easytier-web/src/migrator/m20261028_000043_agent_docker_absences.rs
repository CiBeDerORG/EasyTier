use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            CREATE TABLE asset_agent_docker_absences (
                asset_device_id TEXT NOT NULL
                    REFERENCES asset_devices(id) ON DELETE CASCADE,
                agent_id TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                last_resource_json TEXT NOT NULL,
                last_seen_at TEXT NOT NULL,
                first_missing_at TEXT NOT NULL,
                last_missing_at TEXT NOT NULL,
                consecutive_complete_absences INTEGER NOT NULL
                    CHECK (consecutive_complete_absences >= 1),
                PRIMARY KEY (asset_device_id, agent_id, resource_id)
            );
            CREATE INDEX idx_agent_docker_absences_agent
                ON asset_agent_docker_absences(asset_device_id, agent_id);
            "#,
        ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "DROP TABLE IF EXISTS asset_agent_docker_absences;"
        ).await?;
        Ok(())
    }
}
