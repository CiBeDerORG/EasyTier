use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
CREATE TABLE automation_drafts (
    id TEXT PRIMARY KEY NOT NULL,
    owner_user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_user_id INTEGER NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    steps_json TEXT NOT NULL CHECK (json_valid(steps_json) AND json_type(steps_json) = 'array'),
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_automation_drafts_owner_updated
    ON automation_drafts(owner_user_id, updated_at DESC, id DESC);

-- Retain mutation receipts even after draft deletion. An old create/save
-- request must never be replayed as a fresh mutation or resurrect a draft.
CREATE TABLE automation_draft_mutations (
    owner_user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_user_id INTEGER NOT NULL,
    request_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('create','update','delete')),
    response_json TEXT NOT NULL CHECK (json_valid(response_json)),
    created_at TEXT NOT NULL,
    PRIMARY KEY (owner_user_id, actor_user_id, request_id)
);
CREATE INDEX idx_automation_draft_mutations_draft
    ON automation_draft_mutations(owner_user_id, draft_id);
        "#).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Restore a database backup; rolling back the automation draft migration would delete unsaved user work".into(),
        ))
    }
}
