use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Do NOT cascade with automation_scripts: a timeout followed by deletion
        // must never turn the original request ID into an executable new create.
        manager.get_connection().execute_unprepared(r#"
CREATE TABLE automation_script_mutations (
    owner_user_id INTEGER NOT NULL,
    request_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    script_id TEXT NOT NULL,
    response_json TEXT NOT NULL CHECK(json_valid(response_json)),
    created_at TEXT NOT NULL,
    PRIMARY KEY(owner_user_id, request_id)
);
CREATE INDEX idx_automation_script_mutations_script
    ON automation_script_mutations(owner_user_id, script_id);
        "#).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Formal script request receipts are durable idempotency tombstones and must not be dropped.".into()
        ))
    }
}
