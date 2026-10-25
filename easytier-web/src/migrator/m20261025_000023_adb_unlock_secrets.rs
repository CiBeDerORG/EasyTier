use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager:&SchemaManager)->Result<(),DbErr>{
        manager.get_connection().execute_unprepared("CREATE TABLE adb_service_unlock_secrets (service_id TEXT PRIMARY KEY NOT NULL REFERENCES asset_services(id) ON DELETE CASCADE,owner_user_id INTEGER NOT NULL,secret_cipher TEXT NOT NULL,updated_at TEXT NOT NULL);CREATE INDEX idx_adb_unlock_owner ON adb_service_unlock_secrets(owner_user_id);").await?;
        Ok(())
    }
    async fn down(&self, manager:&SchemaManager)->Result<(),DbErr>{
        manager.get_connection().execute_unprepared("DROP TABLE adb_service_unlock_secrets").await?;
        Ok(())
    }
}
