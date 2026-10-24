use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "ALTER TABLE node_asset_metadata ADD COLUMN public_ip TEXT CHECK (public_ip IS NULL OR length(public_ip) BETWEEN 1 AND 45)"
        ).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Restore the pre-upgrade backup rather than discarding manually configured public server addresses".into(),
        ))
    }
}
