use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            ALTER TABLE asset_device_types
                ADD COLUMN semantic_kinds TEXT NOT NULL DEFAULT '[]'
                CHECK (json_valid(semantic_kinds) AND json_type(semantic_kinds) = 'array');
            UPDATE asset_device_types
                SET semantic_kinds = CASE
                    WHEN semantic_kind IS NULL THEN '[]'
                    ELSE json_array(semantic_kind)
                END;
            "#,
        ).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Multi-select semantics cannot be downgraded without losing server and combined semantic kinds".into(),
        ))
    }
}
