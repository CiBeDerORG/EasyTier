use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE automation_runner_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                endpoint TEXT NOT NULL,
                token_cipher TEXT NOT NULL,
                revision INTEGER NOT NULL,
                updated_by INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            )",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Restore the pre-upgrade backup; do not discard saved executor credentials during rollback".into(),
        ))
    }
}
