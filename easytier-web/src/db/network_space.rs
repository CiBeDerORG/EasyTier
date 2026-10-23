//! User-owned presentation metadata. Network connectivity remains runtime data;
//! configured LAN relationships deliberately carry no inferred health state.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sqlx::Sqlite;
use uuid::Uuid;

use super::{Db, UserIdInDb};

#[derive(Debug, thiserror::Error)]
pub(crate) enum SpaceError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("database error: {0}")]
    Database(String),
}

impl From<sqlx::Error> for SpaceError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value.to_string())
    }
}

impl From<serde_json::Error> for SpaceError {
    fn from(value: serde_json::Error) -> Self {
        Self::Database(value.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MapNodeRole {
    Node,
    Gateway,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MapLayout {
    Topology,
    Home,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MapNodeConfig {
    pub device_id: Uuid,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub label: String,
    pub role: MapNodeRole,
    pub layout: MapLayout,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LanDeviceKind {
    Server,
    Desktop,
    Camera,
    Router,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MapRoom {
    Living,
    Study,
    Bedroom,
    Entry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MapLanDevice {
    pub id: String,
    pub gateway_device_id: Uuid,
    pub name: String,
    pub address: String,
    pub kind: LanDeviceKind,
    pub room: MapRoom,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NetworkMapConfig {
    pub revision: i64,
    pub nodes: Vec<MapNodeConfig>,
    pub devices: Vec<MapLanDevice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceIcon {
    Home,
    Photos,
    Media,
    Chart,
    Server,
    Globe,
    Code,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceInput {
    pub network_id: Uuid,
    pub device_id: Uuid,
    pub lan_device_id: Option<String>,
    pub name: String,
    pub description: String,
    pub url: String,
    pub icon: ServiceIcon,
    pub group: String,
    pub show_in_navigation: bool,
    pub open_in_new_tab: bool,
    pub sort_order: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ServiceEntry {
    pub id: Uuid,
    #[serde(flatten)]
    pub config: ServiceInput,
}

fn text(value: &mut String, field: &str, max: usize, required: bool) -> Result<(), SpaceError> {
    *value = value.trim().to_owned();
    if (required && value.is_empty())
        || value.chars().count() > max
        || value.chars().any(char::is_control)
    {
        return Err(SpaceError::Invalid(format!("invalid {field}")));
    }
    Ok(())
}

impl NetworkMapConfig {
    fn validate(&mut self, members: &HashSet<Uuid>) -> Result<(), SpaceError> {
        if !(0..9_007_199_254_740_991).contains(&self.revision)
            || self.nodes.len() > 2_000
            || self.devices.len() > 5_000
        {
            return Err(SpaceError::Invalid(
                "map configuration is too large or has an invalid revision".into(),
            ));
        }
        let mut ids = HashSet::new();
        let mut gateways = HashSet::new();
        for node in &mut self.nodes {
            if !members.contains(&node.device_id) || !ids.insert(node.device_id) {
                return Err(SpaceError::Invalid(
                    "map nodes must be unique members of this network".into(),
                ));
            }
            match (node.latitude, node.longitude) {
                (None, None) => {}
                (Some(lat), Some(lon))
                    if lat.is_finite()
                        && lon.is_finite()
                        && (-90.0..=90.0).contains(&lat)
                        && (-180.0..=180.0).contains(&lon) => {}
                _ => {
                    return Err(SpaceError::Invalid(
                        "latitude and longitude must be a valid pair".into(),
                    ));
                }
            }
            text(&mut node.label, "node label", 100, false)?;
            if node.role == MapNodeRole::Gateway {
                gateways.insert(node.device_id);
            }
        }
        let mut lan_ids = HashSet::new();
        for device in &mut self.devices {
            text(&mut device.id, "LAN device id", 64, true)?;
            if !device
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
                || !lan_ids.insert(device.id.clone())
                || !gateways.contains(&device.gateway_device_id)
            {
                return Err(SpaceError::Invalid(
                    "LAN devices require unique ids and a configured gateway in this network"
                        .into(),
                ));
            }
            text(&mut device.name, "LAN device name", 100, true)?;
            text(&mut device.address, "LAN device address", 255, true)?;
            if device.address.chars().any(char::is_whitespace) {
                return Err(SpaceError::Invalid(
                    "LAN device address cannot contain whitespace".into(),
                ));
            }
        }
        Ok(())
    }

    fn retain_members(&mut self, members: &HashSet<Uuid>) -> bool {
        let before = (self.nodes.len(), self.devices.len());
        self.nodes.retain(|node| members.contains(&node.device_id));
        let gateways: HashSet<_> = self
            .nodes
            .iter()
            .filter(|node| node.role == MapNodeRole::Gateway)
            .map(|node| node.device_id)
            .collect();
        self.devices
            .retain(|device| gateways.contains(&device.gateway_device_id));
        before != (self.nodes.len(), self.devices.len())
    }
}

impl ServiceInput {
    fn normalize(&mut self) -> Result<(), SpaceError> {
        text(&mut self.name, "service name", 100, true)?;
        text(&mut self.description, "service description", 512, false)?;
        text(&mut self.group, "service group", 64, false)?;
        text(&mut self.url, "service URL", 2048, true)?;
        let parsed = url::Url::parse(&self.url).map_err(|_| {
            SpaceError::Invalid("service URL must be an absolute HTTP or HTTPS URL".into())
        })?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(SpaceError::Invalid(
                "service URL must use HTTP or HTTPS without embedded credentials".into(),
            ));
        }
        self.url = parsed.to_string();
        if self.sort_order.unsigned_abs() > 1_000_000 {
            return Err(SpaceError::Invalid(
                "service sort order is out of range".into(),
            ));
        }
        if let Some(id) = &mut self.lan_device_id {
            text(id, "LAN device id", 64, true)?;
        }
        Ok(())
    }
}

async fn require_network(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    user_id: UserIdInDb,
    network_id: Uuid,
) -> Result<(), SpaceError> {
    let found: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM networks WHERE user_id = ? AND id = ?)")
            .bind(user_id)
            .bind(network_id.to_string())
            .fetch_one(&mut **tx)
            .await?;
    if !found {
        return Err(SpaceError::NotFound("network not found".into()));
    }
    Ok(())
}

async fn member_ids(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    user_id: UserIdInDb,
    network_id: Uuid,
) -> Result<HashSet<Uuid>, SpaceError> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT device_id FROM network_members WHERE user_id = ? AND network_id = ?",
    )
    .bind(user_id)
    .bind(network_id.to_string())
    .fetch_all(&mut **tx)
    .await?;
    ids.into_iter()
        .map(|id| Uuid::parse_str(&id).map_err(|error| SpaceError::Database(error.to_string())))
        .collect()
}

async fn read_map(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    user_id: UserIdInDb,
    network_id: Uuid,
) -> Result<NetworkMapConfig, SpaceError> {
    let row: Option<(i64, String)> = sqlx::query_as("SELECT revision, config_json FROM network_map_configs WHERE user_id = ? AND network_id = ?")
        .bind(user_id).bind(network_id.to_string()).fetch_optional(&mut **tx).await?;
    let Some((revision, json)) = row else {
        return Ok(NetworkMapConfig::default());
    };
    let mut config: NetworkMapConfig = serde_json::from_str(&json)?;
    config.revision = revision;
    Ok(config)
}

async fn validate_service_target(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    user_id: UserIdInDb,
    input: &ServiceInput,
) -> Result<(), SpaceError> {
    require_network(tx, user_id, input.network_id).await?;
    if !member_ids(tx, user_id, input.network_id)
        .await?
        .contains(&input.device_id)
    {
        return Err(SpaceError::Invalid(
            "service device must be a member of this network".into(),
        ));
    }
    if let Some(id) = &input.lan_device_id {
        let map = read_map(tx, user_id, input.network_id).await?;
        if !map
            .devices
            .iter()
            .any(|device| &device.id == id && device.gateway_device_id == input.device_id)
        {
            return Err(SpaceError::Invalid(
                "service LAN device must belong to the selected gateway".into(),
            ));
        }
    }
    Ok(())
}

impl Db {
    pub(crate) async fn get_network_map(
        &self,
        user_id: UserIdInDb,
        network_id: Uuid,
    ) -> Result<NetworkMapConfig, SpaceError> {
        let mut tx = self.db.begin().await?;
        require_network(&mut tx, user_id, network_id).await?;
        let mut config = read_map(&mut tx, user_id, network_id).await?;
        config.retain_members(&member_ids(&mut tx, user_id, network_id).await?);
        tx.commit().await?;
        Ok(config)
    }

    pub(crate) async fn save_network_map(
        &self,
        user_id: UserIdInDb,
        network_id: Uuid,
        mut config: NetworkMapConfig,
    ) -> Result<NetworkMapConfig, SpaceError> {
        let mut tx = self.db.begin_with("BEGIN IMMEDIATE").await?;
        require_network(&mut tx, user_id, network_id).await?;
        config.validate(&member_ids(&mut tx, user_id, network_id).await?)?;
        let current = read_map(&mut tx, user_id, network_id).await?;
        if current.revision != config.revision {
            return Err(SpaceError::Conflict(
                "map has changed; reload before saving".into(),
            ));
        }
        // A device's service must not silently move to another gateway or
        // disappear when its configured LAN relationship is edited.
        let references: Vec<(String, String)> = sqlx::query_as("SELECT lan_device_id, device_id FROM network_services WHERE user_id = ? AND network_id = ? AND lan_device_id IS NOT NULL")
            .bind(user_id).bind(network_id.to_string()).fetch_all(&mut *tx).await?;
        for (id, gateway) in references {
            if !config
                .devices
                .iter()
                .any(|device| device.id == id && device.gateway_device_id.to_string() == gateway)
            {
                return Err(SpaceError::Conflict("remove or reassign the associated services before removing or moving a LAN device".into()));
            }
        }
        let expected_revision = config.revision;
        config.revision += 1;
        let result = sqlx::query(
            "INSERT INTO network_map_configs (user_id, network_id, revision, config_json) VALUES (?, ?, ?, ?) ON CONFLICT(user_id, network_id) DO UPDATE SET revision = excluded.revision, config_json = excluded.config_json WHERE network_map_configs.revision = ?"
        ).bind(user_id).bind(network_id.to_string()).bind(config.revision)
            .bind(serde_json::to_string(&config)?).bind(expected_revision).execute(&mut *tx).await?;
        if result.rows_affected() != 1 {
            return Err(SpaceError::Conflict(
                "map has changed; reload before saving".into(),
            ));
        }
        tx.commit().await?;
        Ok(config)
    }

    pub(crate) async fn list_network_services(
        &self,
        user_id: UserIdInDb,
        network_id: Option<Uuid>,
    ) -> Result<Vec<ServiceEntry>, SpaceError> {
        let mut tx = self.db.begin().await?;
        if let Some(id) = network_id {
            require_network(&mut tx, user_id, id).await?;
        }
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT s.id, s.config_json FROM network_services s JOIN network_members m ON m.user_id = s.user_id AND m.network_id = s.network_id AND m.device_id = s.device_id WHERE s.user_id = ? AND (? IS NULL OR s.network_id = ?) ORDER BY s.sort_order, s.id"
        ).bind(user_id).bind(network_id.map(|id| id.to_string())).bind(network_id.map(|id| id.to_string())).fetch_all(&mut *tx).await?;
        let entries = rows
            .into_iter()
            .map(|(id, json)| {
                Ok(ServiceEntry {
                    id: Uuid::parse_str(&id)
                        .map_err(|error| SpaceError::Database(error.to_string()))?,
                    config: serde_json::from_str(&json)?,
                })
            })
            .collect::<Result<Vec<_>, SpaceError>>()?;
        tx.commit().await?;
        Ok(entries)
    }

    pub(crate) async fn write_network_service(
        &self,
        user_id: UserIdInDb,
        id: Option<Uuid>,
        mut input: ServiceInput,
    ) -> Result<ServiceEntry, SpaceError> {
        input.normalize()?;
        let mut tx = self.db.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(id) = id {
            let found: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM network_services WHERE user_id = ? AND id = ?)",
            )
            .bind(user_id)
            .bind(id.to_string())
            .fetch_one(&mut *tx)
            .await?;
            if !found {
                return Err(SpaceError::NotFound("service not found".into()));
            }
        } else {
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM network_services WHERE user_id = ?")
                    .bind(user_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if count >= 2_000 {
                return Err(SpaceError::Invalid("service limit reached".into()));
            }
        }
        validate_service_target(&mut tx, user_id, &input).await?;
        let id = id.unwrap_or_else(Uuid::new_v4);
        sqlx::query(
            "INSERT INTO network_services (user_id, id, network_id, device_id, lan_device_id, sort_order, config_json) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(user_id, id) DO UPDATE SET network_id = excluded.network_id, device_id = excluded.device_id, lan_device_id = excluded.lan_device_id, sort_order = excluded.sort_order, config_json = excluded.config_json"
        ).bind(user_id).bind(id.to_string()).bind(input.network_id.to_string()).bind(input.device_id.to_string())
            .bind(&input.lan_device_id).bind(input.sort_order).bind(serde_json::to_string(&input)?)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(ServiceEntry { id, config: input })
    }

    pub(crate) async fn delete_network_service(
        &self,
        user_id: UserIdInDb,
        id: Uuid,
    ) -> Result<(), SpaceError> {
        let result = sqlx::query("DELETE FROM network_services WHERE user_id = ? AND id = ?")
            .bind(user_id)
            .bind(id.to_string())
            .execute(&self.db)
            .await?;
        if result.rows_affected() == 0 {
            return Err(SpaceError::NotFound("service not found".into()));
        }
        Ok(())
    }
}

/// Central intent writes replace the member table. Run cleanup only after the
/// final members are present, so unrelated settings changes preserve metadata.
pub(super) async fn prune_network_space(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    user_id: UserIdInDb,
    network_id: Uuid,
) -> Result<(), sea_orm::DbErr> {
    async fn prune(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        user_id: UserIdInDb,
        network_id: Uuid,
    ) -> Result<(), SpaceError> {
        sqlx::query("DELETE FROM network_services WHERE user_id = ? AND network_id = ? AND NOT EXISTS (SELECT 1 FROM network_members m WHERE m.user_id = network_services.user_id AND m.network_id = network_services.network_id AND m.device_id = network_services.device_id)")
            .bind(user_id).bind(network_id.to_string()).execute(&mut **tx).await?;
        let mut config = read_map(tx, user_id, network_id).await?;
        if config.retain_members(&member_ids(tx, user_id, network_id).await?) {
            config.revision += 1;
            sqlx::query("UPDATE network_map_configs SET revision = ?, config_json = ? WHERE user_id = ? AND network_id = ?")
                .bind(config.revision).bind(serde_json::to_string(&config)?).bind(user_id).bind(network_id.to_string()).execute(&mut **tx).await?;
        }
        Ok(())
    }
    prune(tx, user_id, network_id)
        .await
        .map_err(|error| sea_orm::DbErr::Custom(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> (Db, i32, Uuid, Uuid, Uuid) {
        let db = Db::memory_db().await;
        let user_id = db.auto_create_user("space-owner").await.unwrap().id;
        let network_id = Uuid::new_v4();
        let gateway = Uuid::new_v4();
        let peer = Uuid::new_v4();
        sqlx::query("INSERT INTO networks (user_id, id, display_name, network_name, network_secret, networking_method, create_time, update_time) VALUES (?, ?, 'Home', 'home', 'test-secret', 'Standalone', 'now', 'now')")
            .bind(user_id).bind(network_id.to_string()).execute(&db.db).await.unwrap();
        for device_id in [gateway, peer] {
            db.upsert_device_heartbeat(crate::db::DeviceHeartbeatRecord {
                user_id,
                machine_id: device_id,
                hostname: "node".into(),
                easytier_version: "test".into(),
                device_os: "{}".into(),
                client_url: "tcp://127.0.0.1:11010".into(),
            })
            .await
            .unwrap();
            sqlx::query("INSERT INTO network_members (user_id, network_id, id, device_id, create_time, update_time) VALUES (?, ?, ?, ?, 'now', 'now')")
                .bind(user_id).bind(network_id.to_string()).bind(Uuid::new_v4().to_string()).bind(device_id.to_string())
                .execute(&db.db).await.unwrap();
        }
        (db, user_id, network_id, gateway, peer)
    }

    fn map(gateway: Uuid, peer: Uuid) -> NetworkMapConfig {
        NetworkMapConfig {
            revision: 0,
            nodes: vec![
                MapNodeConfig {
                    device_id: gateway,
                    latitude: Some(31.23),
                    longitude: Some(121.47),
                    label: " Home ".into(),
                    role: MapNodeRole::Gateway,
                    layout: MapLayout::Home,
                },
                MapNodeConfig {
                    device_id: peer,
                    latitude: None,
                    longitude: None,
                    label: "Laptop".into(),
                    role: MapNodeRole::Node,
                    layout: MapLayout::Topology,
                },
            ],
            devices: vec![MapLanDevice {
                id: "nas".into(),
                gateway_device_id: gateway,
                name: "NAS".into(),
                address: "192.168.31.20".into(),
                kind: LanDeviceKind::Server,
                room: MapRoom::Study,
            }],
        }
    }

    fn service(network_id: Uuid, device_id: Uuid) -> ServiceInput {
        ServiceInput {
            network_id,
            device_id,
            lan_device_id: None,
            name: " Home Assistant ".into(),
            description: "Automation".into(),
            url: "http://10.144.0.1:8123".into(),
            icon: ServiceIcon::Home,
            group: "Home".into(),
            show_in_navigation: true,
            open_in_new_tab: true,
            sort_order: 0,
        }
    }

    #[tokio::test]
    async fn map_round_trip_rejects_stale_revisions_and_other_tenants() {
        let (db, user, network, gateway, peer) = setup().await;
        assert_eq!(
            db.get_network_map(user, network).await.unwrap(),
            NetworkMapConfig::default()
        );
        let saved = db
            .save_network_map(user, network, map(gateway, peer))
            .await
            .unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.nodes[0].label, "Home");
        assert_eq!(db.get_network_map(user, network).await.unwrap(), saved);
        assert!(matches!(
            db.save_network_map(user, network, map(gateway, peer)).await,
            Err(SpaceError::Conflict(_))
        ));
        assert_eq!(
            db.save_network_map(user, network, saved)
                .await
                .unwrap()
                .revision,
            2
        );
        let other = db.auto_create_user("other-map-owner").await.unwrap().id;
        assert!(matches!(
            db.get_network_map(other, network).await,
            Err(SpaceError::NotFound(_))
        ));
        assert!(matches!(
            db.save_network_map(other, network, map(gateway, peer))
                .await,
            Err(SpaceError::NotFound(_))
        ));
    }

    #[test]
    fn map_validation_requires_real_members_coordinates_and_gateway_relationships() {
        let gateway = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let members = HashSet::from([gateway, peer]);
        let mut valid = map(gateway, peer);
        valid.validate(&members).unwrap();
        for coordinates in [
            (Some(91.0), Some(0.0)),
            (Some(0.0), Some(181.0)),
            (None, Some(1.0)),
            (Some(f64::NAN), Some(0.0)),
        ] {
            let mut invalid = valid.clone();
            invalid.nodes[0].latitude = coordinates.0;
            invalid.nodes[0].longitude = coordinates.1;
            assert!(invalid.validate(&members).is_err());
        }
        let mut duplicate = valid.clone();
        duplicate.nodes.push(duplicate.nodes[0].clone());
        assert!(duplicate.validate(&members).is_err());
        let mut foreign = valid.clone();
        foreign.nodes[0].device_id = Uuid::new_v4();
        assert!(foreign.validate(&members).is_err());
        let mut wrong_parent = valid.clone();
        wrong_parent.devices[0].gateway_device_id = peer;
        assert!(wrong_parent.validate(&members).is_err());
        let mut changed_role = valid;
        changed_role.nodes[0].role = MapNodeRole::Node;
        assert!(changed_role.validate(&members).is_err());
    }

    #[test]
    fn service_urls_are_safe_browser_targets_without_server_side_fetching() {
        let mut input = service(Uuid::new_v4(), Uuid::new_v4());
        for url in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///etc/passwd",
            "ftp://example.com",
            "//example.com",
            "https://user:password@example.com",
            "https://user@example.com",
            "http://",
        ] {
            input.url = url.into();
            assert!(input.normalize().is_err(), "accepted {url}");
        }
        for url in [
            "http://192.168.1.2:8123",
            "https://service.example/path?q=hello#settings",
            "http://[::1]:8080",
        ] {
            input.url = url.into();
            input.normalize().unwrap();
        }
    }

    #[tokio::test]
    async fn services_are_owner_scoped_and_navigation_does_not_delete_them() {
        let (db, user, network, gateway, _) = setup().await;
        let mut input = service(network, gateway);
        let created = db
            .write_network_service(user, None, input.clone())
            .await
            .unwrap();
        assert_eq!(created.config.name, "Home Assistant");
        assert_eq!(created.config.url, "http://10.144.0.1:8123/");
        input.show_in_navigation = false;
        let updated = db
            .write_network_service(user, Some(created.id), input.clone())
            .await
            .unwrap();
        assert!(!updated.config.show_in_navigation);
        assert_eq!(
            db.list_network_services(user, Some(network)).await.unwrap(),
            vec![updated]
        );
        let other = db.auto_create_user("other-service-owner").await.unwrap().id;
        assert!(
            db.list_network_services(other, None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            db.list_network_services(other, Some(network)).await,
            Err(SpaceError::NotFound(_))
        ));
        assert!(matches!(
            db.write_network_service(other, Some(created.id), input.clone())
                .await,
            Err(SpaceError::NotFound(_))
        ));
        assert!(matches!(
            db.delete_network_service(other, created.id).await,
            Err(SpaceError::NotFound(_))
        ));
        input.device_id = Uuid::new_v4();
        assert!(matches!(
            db.write_network_service(user, None, input).await,
            Err(SpaceError::Invalid(_))
        ));
        db.delete_network_service(user, created.id).await.unwrap();
        assert!(
            db.list_network_services(user, None)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn referenced_lan_devices_cannot_disappear_or_move_without_reassigning_services() {
        let (db, user, network, gateway, peer) = setup().await;
        let saved = db
            .save_network_map(user, network, map(gateway, peer))
            .await
            .unwrap();
        let mut input = service(network, gateway);
        input.lan_device_id = Some("nas".into());
        let created = db
            .write_network_service(user, None, input.clone())
            .await
            .unwrap();
        input.device_id = peer;
        assert!(matches!(
            db.write_network_service(user, None, input).await,
            Err(SpaceError::Invalid(_))
        ));
        let mut removed = saved.clone();
        removed.devices.clear();
        assert!(matches!(
            db.save_network_map(user, network, removed.clone()).await,
            Err(SpaceError::Conflict(_))
        ));
        assert_eq!(db.get_network_map(user, network).await.unwrap(), saved);
        db.delete_network_service(user, created.id).await.unwrap();
        db.save_network_map(user, network, removed).await.unwrap();
    }

    #[tokio::test]
    async fn replacing_members_preserves_metadata_until_a_member_is_actually_removed() {
        let (db, user, network, gateway, peer) = setup().await;
        let saved = db
            .save_network_map(user, network, map(gateway, peer))
            .await
            .unwrap();
        let entry = db
            .write_network_service(user, None, service(network, gateway))
            .await
            .unwrap();
        let mut intent = db
            .load_central_network_intent(user, network)
            .await
            .unwrap()
            .unwrap();
        intent.display_name = "Renamed".into();
        db.save_central_network_intent(intent.clone())
            .await
            .unwrap();
        assert_eq!(db.get_network_map(user, network).await.unwrap(), saved);
        assert_eq!(
            db.list_network_services(user, None).await.unwrap(),
            vec![entry]
        );
        intent.remove_device(gateway);
        db.save_central_network_intent(intent).await.unwrap();
        assert!(
            db.list_network_services(user, None)
                .await
                .unwrap()
                .is_empty()
        );
        let pruned = db.get_network_map(user, network).await.unwrap();
        assert_eq!(pruned.revision, 2);
        assert_eq!(pruned.nodes.len(), 1);
        assert_eq!(pruned.nodes[0].device_id, peer);
        assert!(pruned.devices.is_empty());
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM network_services")
            .fetch_one(&db.db)
            .await
            .unwrap();
        assert_eq!(
            rows, 0,
            "removed membership must not resurrect a service on rejoin"
        );
    }

    #[tokio::test]
    async fn deleting_network_cascades_map_and_service_records() {
        let (db, user, network, gateway, peer) = setup().await;
        db.save_network_map(user, network, map(gateway, peer))
            .await
            .unwrap();
        db.write_network_service(user, None, service(network, gateway))
            .await
            .unwrap();
        sqlx::query("DELETE FROM networks WHERE user_id = ? AND id = ?")
            .bind(user)
            .bind(network.to_string())
            .execute(&db.db)
            .await
            .unwrap();
        for table in ["network_services", "network_map_configs"] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&db.db)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
    }
}
