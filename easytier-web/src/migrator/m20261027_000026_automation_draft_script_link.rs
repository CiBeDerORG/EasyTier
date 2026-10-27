use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager:&SchemaManager)->Result<(),DbErr> {
        manager.get_connection().execute_unprepared(r#"
ALTER TABLE automation_drafts ADD COLUMN script_id TEXT;
ALTER TABLE automation_drafts ADD COLUMN script_revision INTEGER
    CHECK (script_revision IS NULL OR script_revision > 0);
        "#).await?;
        Ok(())
    }
    async fn down(&self,_manager:&SchemaManager)->Result<(),DbErr>{
        Err(DbErr::Custom("A draft's source script revision is required for safe recovery; do not downgrade without a database backup".into()))
    }
}
