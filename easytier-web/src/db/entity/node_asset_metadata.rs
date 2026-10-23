//! Persistent node asset data. Foreign keys and constraints are defined by the migration.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "node_asset_metadata")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub owner_user_id: i32,
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub machine_id: String,
    pub is_public_server: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

