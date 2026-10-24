use sea_orm_migration::{SchemaManagerConnection, prelude::*};
use sqlx::Connection;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let SchemaManagerConnection::Connection(db) = manager.get_connection() else {
            return Err(DbErr::Custom("OpenWrt discovery migration requires a dedicated SQLite connection outside a transaction".into()));
        };
        let pool = db.get_sqlite_connection_pool();
        let mut connection = pool.acquire().await.map_err(db_error)?;
        // SQLite cannot drop an inline UNIQUE constraint. Rebuild on this ONE
        // pinned connection, with FK enforcement restored before pool return.
        // Other connections retain their enforcement settings throughout.
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&mut *connection)
            .await
            .map_err(db_error)?;
        let result = async {
            let mut tx = connection.begin_with("BEGIN IMMEDIATE").await?;
            let result = async {
                let schema: String = sqlx::query_scalar("SELECT sql FROM sqlite_schema WHERE type='table' AND name='asset_devices'").fetch_one(&mut *tx).await?;
                let constraint = "UNIQUE (owner_user_id, machine_id, lan_ip),";
                if schema.matches(constraint).count() != 1 {
                    return Err(sqlx::Error::Protocol("Unexpected asset_devices schema; migration stopped without changing records".into()));
                }
                let triggers: Vec<String> = sqlx::query_scalar("SELECT sql FROM sqlite_schema WHERE type='trigger' AND sql IS NOT NULL").fetch_all(&mut *tx).await?;
                let names: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='trigger'").fetch_all(&mut *tx).await?;
                let indexes: Vec<String> = sqlx::query_scalar("SELECT sql FROM sqlite_schema WHERE type='index' AND tbl_name='asset_devices' AND sql IS NOT NULL").fetch_all(&mut *tx).await?;
                for name in names { let sql=format!("DROP TRIGGER \"{}\"",name.replace('"',"\"\"")); sqlx::Executor::execute(&mut *tx, sql.as_str()).await?; }
                let replacement = schema.replacen("asset_devices", "asset_devices_ods", 1).replace(constraint, "");
                sqlx::Executor::execute(&mut *tx, replacement.as_str()).await?;
                sqlx::Executor::execute(&mut *tx, "INSERT INTO asset_devices_ods SELECT * FROM asset_devices; DROP TABLE asset_devices; ALTER TABLE asset_devices_ods RENAME TO asset_devices;").await?;
                for sql in indexes.into_iter().chain(triggers) { sqlx::Executor::execute(&mut *tx, sql.as_str()).await?; }
                sqlx::Executor::execute(&mut *tx, include_str!("openwrt_discovery.sql")).await?;
                if !sqlx::query("PRAGMA foreign_key_check").fetch_all(&mut *tx).await?.is_empty() {
                    return Err(sqlx::Error::Protocol("Foreign key check failed; discovery migration rolled back".into()));
                }
                Ok::<_, sqlx::Error>(())
            }.await;
            match result { Ok(()) => tx.commit().await, Err(error) => { tx.rollback().await?; Err(error) } }
        }.await;
        let restored = sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&mut *connection)
            .await;
        if let Err(error) = restored {
            connection.close().await.map_err(db_error)?;
            return Err(db_error(error));
        }
        result.map_err(db_error)
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom("Restore the pre-upgrade database and credential key backup; group-scoped duplicate addresses cannot be safely collapsed".into()))
    }
}
fn db_error(error: sqlx::Error) -> DbErr {
    DbErr::Custom(error.to_string())
}
