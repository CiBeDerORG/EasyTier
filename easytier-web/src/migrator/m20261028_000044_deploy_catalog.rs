use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            CREATE TABLE deploy_catalog_state (
                singleton INTEGER NOT NULL PRIMARY KEY CHECK(singleton = 1),
                git_revision TEXT NOT NULL,
                fetched_at TEXT NOT NULL,
                app_count INTEGER NOT NULL CHECK(app_count > 0)
            );
            CREATE TABLE deploy_catalog_apps (
                app_id TEXT NOT NULL PRIMARY KEY,
                git_revision TEXT NOT NULL,
                name TEXT NOT NULL,
                summary TEXT NOT NULL,
                category TEXT NOT NULL,
                compose_project TEXT NOT NULL,
                manifest_path TEXT NOT NULL,
                manifest_sha256 TEXT NOT NULL,
                detail_json TEXT NOT NULL
            );
            CREATE INDEX idx_deploy_catalog_apps_category
                ON deploy_catalog_apps(category, name, app_id);
            "#,
        ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "DROP TABLE IF EXISTS deploy_catalog_apps; DROP TABLE IF EXISTS deploy_catalog_state;"
        ).await?;
        Ok(())
    }
}
