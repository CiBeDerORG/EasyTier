//! User-maintained node placement. This is independent of observed IP geography.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use super::{Db, UserIdInDb, sqlx_db_error};

fn wgs84() -> String { "WGS84".to_owned() }

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeGeoLocation {
    pub(crate) country: Option<String>,
    pub(crate) region: Option<String>,
    pub(crate) city: Option<String>,
    pub(crate) district: Option<String>,
    pub(crate) latitude: Option<f64>,
    pub(crate) longitude: Option<f64>,
    #[serde(default = "wgs84")]
    pub(crate) coordinate_system: String,
}

#[derive(Debug)]
pub(crate) struct NodeGeoValidationError {
    pub(crate) field: String,
    pub(crate) message: String,
}

impl NodeGeoValidationError {
    fn field(field: &str, message: &str) -> Self {
        Self { field: format!("geo_location.{field}"), message: message.to_owned() }
    }
}

fn administrative_name(value: &Value, field: &str) -> Result<Option<String>, NodeGeoValidationError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            if value.chars().any(char::is_control) {
                return Err(NodeGeoValidationError::field(field, "Control characters are not allowed."));
            }
            let value = value.trim();
            if value.chars().count() > 128 {
                return Err(NodeGeoValidationError::field(field, "Use at most 128 characters."));
            }
            Ok((!value.is_empty()).then(|| value.to_owned()))
        }
        _ => Err(NodeGeoValidationError::field(field, "Provide text or null for this field.")),
    }
}

fn coordinate(value: &Value, field: &str, limit: f64) -> Result<Option<f64>, NodeGeoValidationError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let coordinate = value.as_f64().filter(|coordinate| coordinate.is_finite() && coordinate.abs() <= limit)
                .ok_or_else(|| NodeGeoValidationError::field(field, &format!("Enter a finite number between -{limit} and {limit}.")))?;
            Ok(Some(if coordinate == 0.0 { 0.0 } else { coordinate }))
        }
    }
}

/// A supplied object replaces the complete placement. Null and a completely
/// empty object both clear it; the caller keeps an omitted field separate.
pub(crate) fn parse_geo_location(value: &Value) -> Result<Option<NodeGeoLocation>, NodeGeoValidationError> {
    if value.is_null() { return Ok(None); }
    let object = value.as_object().ok_or_else(|| NodeGeoValidationError {
        field: "geo_location".to_owned(), message: "Provide a location object or null.".to_owned(),
    })?;
    for field in object.keys() {
        if !["country", "region", "city", "district", "latitude", "longitude", "coordinate_system"].contains(&field.as_str()) {
            return Err(NodeGeoValidationError::field(field, "This location field is not supported."));
        }
    }
    if value.get("coordinate_system").is_some_and(|system| system.as_str() != Some("WGS84")) {
        return Err(NodeGeoValidationError::field("coordinate_system", "Coordinates must use WGS84."));
    }
    let location = NodeGeoLocation {
        country: administrative_name(value, "country")?,
        region: administrative_name(value, "region")?,
        city: administrative_name(value, "city")?,
        district: administrative_name(value, "district")?,
        latitude: coordinate(value, "latitude", 90.0)?,
        longitude: coordinate(value, "longitude", 180.0)?,
        coordinate_system: wgs84(),
    };
    if location.latitude.is_some() != location.longitude.is_some() {
        return Err(NodeGeoValidationError::field(
            if location.latitude.is_none() { "latitude" } else { "longitude" },
            "Set both latitude and longitude, or leave both empty.",
        ));
    }
    if location.country.is_none() && location.region.is_none() && location.city.is_none()
        && location.district.is_none() && location.latitude.is_none() {
        return Ok(None);
    }
    Ok(Some(location))
}

/// Serde's ordinary Option treats explicit null like an omitted property.
/// This adapter retains Some(Null), so old clients cannot clear saved placement.
pub(crate) fn deserialize_geo_patch<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where D: serde::Deserializer<'de> {
    <Value as Deserialize>::deserialize(deserializer).map(Some)
}

pub(crate) fn row_location(row: &SqliteRow) -> Result<Option<NodeGeoLocation>, sqlx::Error> {
    if row.try_get::<Option<i32>, _>("geo_owner_user_id")?.is_none() { return Ok(None); }
    Ok(Some(NodeGeoLocation {
        country: row.try_get("geo_country")?,
        region: row.try_get("geo_region")?,
        city: row.try_get("geo_city")?,
        district: row.try_get("geo_district")?,
        latitude: row.try_get("geo_latitude")?,
        longitude: row.try_get("geo_longitude")?,
        coordinate_system: wgs84(),
    }))
}

const GEO_COLUMNS: &str = "owner_user_id AS geo_owner_user_id, machine_id, country AS geo_country, \
    region AS geo_region, city AS geo_city, district AS geo_district, \
    latitude AS geo_latitude, longitude AS geo_longitude";

pub(crate) async fn read_location(
    connection: &mut SqliteConnection,
    owner: UserIdInDb,
    node: &str,
) -> Result<Option<NodeGeoLocation>, sqlx::Error> {
    let sql = format!("SELECT {GEO_COLUMNS} FROM node_geo_locations WHERE owner_user_id = ? AND machine_id = ?");
    let row = sqlx::query(&sql).bind(owner).bind(node).fetch_optional(&mut *connection).await?;
    match row {
        Some(row) => row_location(&row),
        None => Ok(None),
    }
}

pub(crate) async fn write_location(
    connection: &mut SqliteConnection,
    owner: UserIdInDb,
    node: &str,
    location: Option<&NodeGeoLocation>,
) -> Result<(), sqlx::Error> {
    if let Some(location) = location {
        sqlx::query(
            "INSERT INTO node_geo_locations(owner_user_id,machine_id,country,region,city,district,latitude,longitude) \
             VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(owner_user_id,machine_id) DO UPDATE SET \
             country=excluded.country,region=excluded.region,city=excluded.city,district=excluded.district, \
             latitude=excluded.latitude,longitude=excluded.longitude",
        ).bind(owner).bind(node).bind(&location.country).bind(&location.region)
            .bind(&location.city).bind(&location.district).bind(location.latitude).bind(location.longitude)
            .execute(&mut *connection).await?;
    } else {
        sqlx::query("DELETE FROM node_geo_locations WHERE owner_user_id = ? AND machine_id = ?")
            .bind(owner).bind(node).execute(&mut *connection).await?;
    }
    Ok(())
}

impl Db {
    pub(crate) async fn list_node_geo_locations(
        &self,
        owner: UserIdInDb,
    ) -> Result<HashMap<String, NodeGeoLocation>, sea_orm::DbErr> {
        let sql = format!("SELECT {GEO_COLUMNS} FROM node_geo_locations WHERE owner_user_id = ?");
        let rows = sqlx::query(&sql).bind(owner).fetch_all(&self.db).await.map_err(sqlx_db_error)?;
        let mut locations = HashMap::with_capacity(rows.len());
        for row in rows {
            if let Some(location) = row_location(&row).map_err(sqlx_db_error)? {
                locations.insert(row.try_get("machine_id").map_err(sqlx_db_error)?, location);
            }
        }
        Ok(locations)
    }
}
