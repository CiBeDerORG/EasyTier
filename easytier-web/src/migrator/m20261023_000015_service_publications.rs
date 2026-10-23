use sea_orm_migration::{prelude::*, sea_orm::Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            CREATE TABLE asset_service_publications (
                service_id TEXT NOT NULL PRIMARY KEY,
                owner_user_id INTEGER NOT NULL,
                machine_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                public_machine_id TEXT NOT NULL,
                instance_id TEXT NOT NULL,
                config_record_id INTEGER NOT NULL,
                config_create_time TEXT NOT NULL,
                bind_ip TEXT NOT NULL,
                bind_port INTEGER NOT NULL CHECK (bind_port BETWEEN 1 AND 65535),
                dst_ip TEXT NOT NULL,
                dst_port INTEGER NOT NULL CHECK (dst_port BETWEEN 1 AND 65535),
                server_name TEXT NOT NULL,
                network_name TEXT NOT NULL,
                rules_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (service_id, owner_user_id, machine_id, device_id)
                    REFERENCES asset_services(id, owner_user_id, machine_id, device_id)
                    ON DELETE CASCADE,
                FOREIGN KEY (owner_user_id, public_machine_id)
                    REFERENCES devices(user_id, machine_id) ON DELETE RESTRICT
            );
            CREATE INDEX idx_asset_service_publications_source
                ON asset_service_publications(owner_user_id, machine_id, device_id);
            CREATE INDEX idx_asset_service_publications_target
                ON asset_service_publications(owner_user_id, public_machine_id, instance_id);
            "#,
        ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let count = db.query_one(Statement::from_string(
            db.get_database_backend(),
            "SELECT COUNT(*) AS count FROM asset_service_publications",
        )).await?.ok_or_else(|| DbErr::Custom("Cannot inspect service publications".into()))?
            .try_get::<i64>("", "count")?;
        if count != 0 {
            return Err(DbErr::Custom("Unpublish services before removing publication ownership records".into()));
        }
        db.execute_unprepared("DROP TABLE asset_service_publications").await?;
        Ok(())
    }
}
