use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            CREATE TABLE node_geo_locations (
                owner_user_id INTEGER NOT NULL,
                machine_id TEXT NOT NULL,
                country TEXT CHECK (country IS NULL OR length(country) BETWEEN 1 AND 128),
                region TEXT CHECK (region IS NULL OR length(region) BETWEEN 1 AND 128),
                city TEXT CHECK (city IS NULL OR length(city) BETWEEN 1 AND 128),
                district TEXT CHECK (district IS NULL OR length(district) BETWEEN 1 AND 128),
                latitude REAL,
                longitude REAL,
                coordinate_system TEXT NOT NULL DEFAULT 'WGS84' CHECK (coordinate_system = 'WGS84'),
                PRIMARY KEY (owner_user_id, machine_id),
                FOREIGN KEY (owner_user_id, machine_id)
                    REFERENCES devices(user_id, machine_id) ON DELETE CASCADE,
                CHECK ((latitude IS NULL AND longitude IS NULL) OR
                    (latitude IS NOT NULL AND longitude IS NOT NULL AND
                     latitude BETWEEN -90 AND 90 AND longitude BETWEEN -180 AND 180))
            );
            "#,
        ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("DROP TABLE node_geo_locations").await?;
        Ok(())
    }
}
