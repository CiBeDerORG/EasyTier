//! `SeaORM` Entity for tenant-scoped registered devices.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "devices")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i32,
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub machine_id: String,
    #[sea_orm(column_type = "Text")]
    pub hostname: String,
    #[sea_orm(column_type = "Text", default_value = "")]
    pub alias: String,
    #[sea_orm(column_type = "Text")]
    pub easytier_version: String,
    #[sea_orm(column_type = "Text")]
    pub device_os: String,
    #[sea_orm(column_type = "Text")]
    pub client_url: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub connection_ip: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub ip_country: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub ip_region: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub ip_city: Option<String>,
    #[sea_orm(column_type = "Text", default_value = "unknown")]
    pub ip_location_status: String,
    pub ip_observed_at: Option<DateTimeWithTimeZone>,
    #[sea_orm(column_type = "Text", nullable)]
    pub site_location: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub device_model: Option<String>,
    pub first_seen_time: DateTimeWithTimeZone,
    pub last_seen_time: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
