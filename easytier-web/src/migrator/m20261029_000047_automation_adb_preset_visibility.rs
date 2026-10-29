use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
CREATE TABLE automation_adb_hidden_presets (
    owner_user_id INTEGER NOT NULL,
    preset_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(owner_user_id, preset_id)
);
CREATE INDEX idx_automation_adb_hidden_presets_owner
    ON automation_adb_hidden_presets(owner_user_id, created_at DESC);
        "#).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "DROP TABLE IF EXISTS automation_adb_hidden_presets"
        ).await?;
        Ok(())
    }
}
