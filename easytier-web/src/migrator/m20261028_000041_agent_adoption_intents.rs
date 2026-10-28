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
                CREATE TABLE asset_agent_adoption_intents (
                    asset_device_id TEXT NOT NULL
                        REFERENCES asset_devices(id) ON DELETE CASCADE,
                    owner_user_id INTEGER NOT NULL,
                    agent_id TEXT NOT NULL,
                    resource_id TEXT NOT NULL,
                    application_id TEXT NOT NULL,
                    container_ids_json TEXT NOT NULL,
                    confirmed_at TEXT NOT NULL,
                    PRIMARY KEY (asset_device_id, resource_id),
                    UNIQUE (asset_device_id, application_id)
                );
                CREATE INDEX idx_asset_agent_adoption_intents_agent
                    ON asset_agent_adoption_intents(agent_id);
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS asset_agent_adoption_intents;")
            .await?;
        Ok(())
    }
}
