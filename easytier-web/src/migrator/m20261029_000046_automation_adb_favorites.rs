use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
CREATE TABLE automation_adb_favorites (
    id TEXT PRIMARY KEY NOT NULL,
    owner_user_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    command TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(owner_user_id, command)
);
CREATE INDEX idx_automation_adb_favorites_owner
    ON automation_adb_favorites(owner_user_id, updated_at DESC);
        "#).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "DROP TABLE IF EXISTS automation_adb_favorites"
        ).await?;
        Ok(())
    }
}
