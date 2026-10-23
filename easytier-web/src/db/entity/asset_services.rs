//! Persistent node asset data. Foreign keys and constraints are defined by the migration.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "asset_services")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub id: String,
    pub owner_user_id: i32,
    #[sea_orm(column_type = "Text")]
    pub machine_id: String,
    #[sea_orm(column_type = "Text")]
    pub device_id: String,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    pub port: i32,
    #[sea_orm(column_type = "Text", nullable)]
    pub remarks: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub access_url: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub icon: Option<String>,
    pub show_in_navigation: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

