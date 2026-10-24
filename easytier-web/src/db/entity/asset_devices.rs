//! Persistent node asset data. Foreign keys and constraints are defined by the migration.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "asset_devices")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub id: String,
    pub owner_user_id: i32,
    #[sea_orm(column_type = "Text")]
    pub machine_id: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub gateway_group_id: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub device_type_id: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub model: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub overlay_ip: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub lan_ip: Option<String>,
    pub is_gateway_self: bool,
    #[sea_orm(column_type = "Text", nullable)]
    pub remarks: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub network_instance_id: Option<String>,
    pub config_record_id: Option<i32>,
    #[sea_orm(column_type = "Text", nullable)]
    pub config_create_time: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub config_fingerprint: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub binding_snapshot_json: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub address_mode: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub rule_key: Option<String>,
    pub host_offset: Option<i64>,
    pub discovery_json: Option<String>,
    pub display_name_manual: bool,
    pub endpoint_revision: i64,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

