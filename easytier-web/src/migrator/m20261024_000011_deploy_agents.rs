use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE deploy_agents (
                id TEXT PRIMARY KEY NOT NULL,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                alias TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                ca_pem TEXT NOT NULL,
                credential TEXT NOT NULL,
                agent_id TEXT,
                identity_json TEXT,
                services_json TEXT,
                catalog_json TEXT,
                last_error TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX deploy_agents_owner ON deploy_agents(user_id);
            CREATE TABLE deploy_tasks (
                task_id TEXT NOT NULL,
                connection_id TEXT NOT NULL REFERENCES deploy_agents(id) ON DELETE CASCADE,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                request_cipher TEXT NOT NULL,
                result_json TEXT NOT NULL,
                logs TEXT NOT NULL DEFAULT '',
                log_offset INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(connection_id, task_id)
            );
        "#,
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE deploy_tasks; DROP TABLE deploy_agents;")
            .await?;
        Ok(())
    }
}
