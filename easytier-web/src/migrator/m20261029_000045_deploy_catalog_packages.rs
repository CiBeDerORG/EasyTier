use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            CREATE TABLE deploy_catalog_packages (
                app_id TEXT NOT NULL,
                git_revision TEXT NOT NULL,
                manifest_sha256 TEXT NOT NULL,
                package_sha256 TEXT NOT NULL,
                package_size INTEGER NOT NULL CHECK(package_size > 0),
                file_count INTEGER NOT NULL CHECK(file_count > 0),
                built_at TEXT NOT NULL,
                PRIMARY KEY(app_id, git_revision)
            );
            CREATE INDEX idx_deploy_catalog_packages_revision
                ON deploy_catalog_packages(git_revision, app_id);
            "#,
        ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "DROP TABLE IF EXISTS deploy_catalog_packages;"
        ).await?;
        Ok(())
    }
}
