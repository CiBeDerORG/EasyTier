//! Persistent node asset data. Foreign keys and constraints are defined by the migration.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "asset_operations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub actor_user_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub owner_user_id: i32,
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub operation: String,
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub request_id: String,
    #[sea_orm(column_type = "Text")]
    pub payload_hash: String,
    #[sea_orm(column_type = "Text")]
    pub resource_kind: String,
    #[sea_orm(column_type = "Text")]
    pub resource_id: String,
    #[sea_orm(column_type = "Text")]
    pub response_json: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

