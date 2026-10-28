use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Existing automation_runs.concurrency has CHECK BETWEEN 1 AND 3.
        // Rebuilding it would invalidate live automation_run_devices foreign
        // keys, immutable run results and cancellation tombstones. Preserve
        // the original column for old readers; only values >3 are stored
        // here, and both reads/dispatch use the authoritative override.
        manager.get_connection().execute_unprepared(r#"
CREATE TABLE automation_run_concurrency_overrides (
    run_id TEXT PRIMARY KEY NOT NULL
        REFERENCES automation_runs(id) ON DELETE RESTRICT,
    concurrency INTEGER NOT NULL CHECK(concurrency > 3)
);
        "#).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Batch concurrency overrides are part of immutable run history; unsafe to drop.".into()
        ))
    }
}
