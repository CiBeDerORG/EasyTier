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
                ALTER TABLE devices ADD COLUMN connection_ip TEXT;
                ALTER TABLE devices ADD COLUMN ip_country TEXT;
                ALTER TABLE devices ADD COLUMN ip_region TEXT;
                ALTER TABLE devices ADD COLUMN ip_city TEXT;
                ALTER TABLE devices ADD COLUMN ip_location_status TEXT NOT NULL
                    DEFAULT 'unknown' CHECK(ip_location_status IN (
                        'unknown', 'resolved', 'non_public', 'not_found', 'unavailable'
                    ));
                ALTER TABLE devices ADD COLUMN ip_observed_at TEXT;
                ALTER TABLE devices ADD COLUMN site_location TEXT;
                ALTER TABLE devices ADD COLUMN device_model TEXT;
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Explicit schema rollback discards the connection snapshots and manual
        // attributes, preserving device identities and their existing metadata.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE devices DROP COLUMN device_model;
                ALTER TABLE devices DROP COLUMN site_location;
                ALTER TABLE devices DROP COLUMN ip_observed_at;
                ALTER TABLE devices DROP COLUMN ip_location_status;
                ALTER TABLE devices DROP COLUMN ip_city;
                ALTER TABLE devices DROP COLUMN ip_region;
                ALTER TABLE devices DROP COLUMN ip_country;
                ALTER TABLE devices DROP COLUMN connection_ip;
                "#,
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrator::Migrator;
    use sea_orm::{Database, Statement};

    #[tokio::test]
    async fn node_attributes_migration_preserves_legacy_devices() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let manager = SchemaManager::new(&db);
        for migration in Migrator::migrations()
            .into_iter()
            .take_while(|migration| migration.name() != Migration.name())
        {
            migration.up(&manager).await.unwrap();
        }
        db.execute_unprepared(
            r#"
            INSERT INTO users(username, password)
                VALUES ('legacy-node-owner', 'unusable-test-hash');
            INSERT INTO devices(user_id, machine_id, hostname, alias, client_url,
                first_seen_time, last_seen_time)
                SELECT id, 'legacy-machine', 'legacy-host', 'legacy-alias',
                    'tcp://192.0.2.1:1001', '2026-10-01T00:00:00Z', '2026-10-20T00:00:00Z'
                FROM users WHERE username = 'legacy-node-owner';
            "#,
        )
        .await
        .unwrap();
        let user_id = db
            .query_one(Statement::from_string(
                db.get_database_backend(),
                "SELECT id FROM users WHERE username = 'legacy-node-owner'",
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i32>("", "id")
            .unwrap();

        Migration.up(&manager).await.unwrap();
        let row = db
            .query_one(Statement::from_string(
                db.get_database_backend(),
                "SELECT * FROM devices WHERE machine_id = 'legacy-machine'",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i32>("", "user_id").unwrap(), user_id);
        assert_eq!(
            row.try_get::<String>("", "hostname").unwrap(),
            "legacy-host"
        );
        assert_eq!(row.try_get::<String>("", "alias").unwrap(), "legacy-alias");
        assert_eq!(
            row.try_get::<String>("", "client_url").unwrap(),
            "tcp://192.0.2.1:1001"
        );
        assert_eq!(
            row.try_get::<String>("", "first_seen_time").unwrap(),
            "2026-10-01T00:00:00Z"
        );
        assert_eq!(
            row.try_get::<String>("", "last_seen_time").unwrap(),
            "2026-10-20T00:00:00Z"
        );
        assert_eq!(
            row.try_get::<String>("", "ip_location_status").unwrap(),
            "unknown"
        );
        for column in [
            "connection_ip",
            "ip_country",
            "ip_region",
            "ip_city",
            "ip_observed_at",
            "site_location",
            "device_model",
        ] {
            assert_eq!(row.try_get::<Option<String>>("", column).unwrap(), None);
        }

        db.execute_unprepared(
            "UPDATE devices SET connection_ip = '192.0.2.1', ip_country = 'example', \
             ip_location_status = 'resolved', ip_observed_at = '2026-10-22T00:00:00Z', \
             site_location = 'Office', device_model = 'OpenWrt router'",
        )
        .await
        .unwrap();
        Migration.down(&manager).await.unwrap();
        // This workspace enables the SQLite driver on SeaORM rather than the
        // optional migration schema-discovery feature. Inspect SQLite directly.
        let columns = db
            .query_all(Statement::from_string(
                db.get_database_backend(),
                "PRAGMA table_info(devices)",
            ))
            .await
            .unwrap();
        for column in [
            "connection_ip",
            "ip_country",
            "ip_region",
            "ip_city",
            "ip_location_status",
            "ip_observed_at",
            "site_location",
            "device_model",
        ] {
            assert!(
                columns
                    .iter()
                    .all(|row| row.try_get::<String>("", "name").unwrap() != column)
            );
        }
        let index = db
            .query_one(Statement::from_string(
                db.get_database_backend(),
                "SELECT name FROM sqlite_master WHERE type = 'index' \
                 AND tbl_name = 'devices' AND name = 'idx_devices_user_last_seen'",
            ))
            .await
            .unwrap();
        assert!(index.is_some());
        Migration.up(&manager).await.unwrap();
        let restored = db
            .query_one(Statement::from_string(
                db.get_database_backend(),
                "SELECT alias, ip_location_status, site_location, device_model \
                 FROM devices WHERE machine_id = 'legacy-machine'",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            restored.try_get::<String>("", "alias").unwrap(),
            "legacy-alias"
        );
        assert_eq!(
            restored
                .try_get::<String>("", "ip_location_status")
                .unwrap(),
            "unknown"
        );
        for column in ["site_location", "device_model"] {
            assert_eq!(
                restored.try_get::<Option<String>>("", column).unwrap(),
                None
            );
        }
    }
}
