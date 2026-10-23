use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261024_000011_network_space"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE network_map_configs (
                user_id INTEGER NOT NULL,
                network_id TEXT NOT NULL,
                revision INTEGER NOT NULL CHECK (revision > 0),
                config_json TEXT NOT NULL,
                PRIMARY KEY (user_id, network_id),
                FOREIGN KEY (user_id, network_id) REFERENCES networks(user_id, id)
                    ON DELETE CASCADE ON UPDATE CASCADE
            );
            CREATE TABLE network_services (
                user_id INTEGER NOT NULL,
                id TEXT NOT NULL,
                network_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                lan_device_id TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                config_json TEXT NOT NULL,
                PRIMARY KEY (user_id, id),
                FOREIGN KEY (user_id, network_id) REFERENCES networks(user_id, id)
                    ON DELETE CASCADE ON UPDATE CASCADE,
                FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, machine_id)
                    ON DELETE CASCADE ON UPDATE CASCADE
            );
            CREATE INDEX idx_network_services_network
                ON network_services(user_id, network_id, device_id);
            "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE network_services; DROP TABLE network_map_configs;")
            .await?;
        Ok(())
    }
}
