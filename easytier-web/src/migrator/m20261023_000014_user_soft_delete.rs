use sea_orm_migration::{prelude::*, sea_orm::Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Keep the user identity and unique username reserved. Deleting the
        // users row would cascade into registered nodes and network intent.
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE users ADD COLUMN deleted_at TEXT")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let deleted = db
            .query_one(Statement::from_string(
                db.get_database_backend(),
                "SELECT COUNT(*) AS count FROM users WHERE deleted_at IS NOT NULL",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("Cannot inspect deleted users".into()))?
            .try_get::<i64>("", "count")?;
        if deleted != 0 {
            return Err(DbErr::Custom(
                "Cannot discard account deletion markers; restore a matching backup instead".into(),
            ));
        }
        db.execute_unprepared("ALTER TABLE users DROP COLUMN deleted_at")
            .await?;
        Ok(())
    }
}
