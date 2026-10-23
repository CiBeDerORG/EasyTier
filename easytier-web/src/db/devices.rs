use sea_orm::{
    ColumnTrait as _, DbErr, EntityTrait, QueryFilter as _, QueryOrder as _, Set,
    sea_query::OnConflict,
};
use sqlx::types::chrono;
use uuid::Uuid;

use super::{Db, UserIdInDb, central_intent, entity, sqlx_db_error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceHeartbeatRecord {
    pub user_id: UserIdInDb,
    pub machine_id: Uuid,
    pub hostname: String,
    pub easytier_version: String,
    pub device_os: String,
    pub client_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpLocationStatus {
    Unknown,
    Resolved,
    NonPublic,
    NotFound,
    Unavailable,
}

impl IpLocationStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Resolved => "resolved",
            Self::NonPublic => "non_public",
            Self::NotFound => "not_found",
            Self::Unavailable => "unavailable",
        }
    }
}

/// One server-observed source snapshot. The caller must authenticate the
/// connection and serialize writes by the current session owner before saving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceConnectionObservation {
    pub connection_ip: String,
    pub ip_country: Option<String>,
    pub ip_region: Option<String>,
    pub ip_city: Option<String>,
    pub ip_location_status: IpLocationStatus,
    pub ip_observed_at: chrono::DateTime<chrono::FixedOffset>,
}

impl Db {
    /// A registered node with any manual children must remain intact. This
    /// check is also made before central-network shutdown while holding the
    /// same mutation lock used by manual asset creation.
    pub(crate) async fn node_has_assets(
        &self,
        user_id: UserIdInDb,
        machine_id: Uuid,
    ) -> Result<bool, DbErr> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM asset_gateway_groups WHERE owner_user_id = ? AND machine_id = ? \
             UNION ALL SELECT 1 FROM asset_devices WHERE owner_user_id = ? AND machine_id = ?)",
        )
        .bind(user_id)
        .bind(machine_id.to_string())
        .bind(user_id)
        .bind(machine_id.to_string())
        .fetch_one(&self.db)
        .await
        .map_err(sqlx_db_error)
    }

    /// Save both user-maintained attributes atomically without registering a
    /// missing node or modifying the connection/heartbeat/configuration data.
    pub(crate) async fn set_device_attributes(
        &self,
        (user_id, machine_id): (UserIdInDb, Uuid),
        site_location: Option<&str>,
        device_model: Option<&str>,
    ) -> Result<bool, DbErr> {
        let result = sqlx::query(
            "UPDATE devices SET site_location = ?, device_model = ? \
             WHERE user_id = ? AND machine_id = ?",
        )
        .bind(site_location)
        .bind(device_model)
        .bind(user_id)
        .bind(machine_id.to_string())
        .execute(&self.db)
        .await
        .map_err(sqlx_db_error)?;
        Ok(result.rows_affected() > 0)
    }

    /// Update the complete snapshot without registering a missing node or
    /// changing its heartbeat/configuration metadata. The block check is part
    /// of the same SQLite statement, so a completed ban cannot race a read.
    pub(crate) async fn update_device_connection_observation(
        &self,
        (user_id, machine_id): (UserIdInDb, Uuid),
        observation: &DeviceConnectionObservation,
    ) -> Result<bool, DbErr> {
        let result = sqlx::query(
            r#"
            UPDATE devices
            SET connection_ip = ?, ip_country = ?, ip_region = ?, ip_city = ?,
                ip_location_status = ?, ip_observed_at = ?
            WHERE user_id = ? AND machine_id = ?
                AND NOT EXISTS (
                    SELECT 1 FROM blocked_devices
                    WHERE blocked_devices.user_id = devices.user_id
                        AND blocked_devices.machine_id = devices.machine_id
                )
            "#,
        )
        .bind(&observation.connection_ip)
        .bind(&observation.ip_country)
        .bind(&observation.ip_region)
        .bind(&observation.ip_city)
        .bind(observation.ip_location_status.as_str())
        .bind(observation.ip_observed_at)
        .bind(user_id)
        .bind(machine_id.to_string())
        .execute(&self.db)
        .await
        .map_err(sqlx_db_error)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn upsert_device_heartbeat(
        &self,
        record: DeviceHeartbeatRecord,
    ) -> Result<(), DbErr> {
        use entity::devices as d;

        let now = chrono::Local::now().fixed_offset();
        d::Entity::insert(d::ActiveModel {
            user_id: Set(record.user_id),
            machine_id: Set(record.machine_id.to_string()),
            hostname: Set(record.hostname),
            easytier_version: Set(record.easytier_version),
            device_os: Set(record.device_os),
            client_url: Set(record.client_url),
            first_seen_time: Set(now),
            last_seen_time: Set(now),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::columns([d::Column::UserId, d::Column::MachineId])
                .update_columns([
                    d::Column::Hostname,
                    d::Column::EasytierVersion,
                    d::Column::DeviceOs,
                    d::Column::ClientUrl,
                    d::Column::LastSeenTime,
                ])
                .to_owned(),
        )
        .exec(self.orm_db())
        .await?;
        Ok(())
    }

    pub async fn list_devices(
        &self,
        user_id: UserIdInDb,
    ) -> Result<Vec<entity::devices::Model>, DbErr> {
        use entity::devices as d;

        d::Entity::find()
            .filter(d::Column::UserId.eq(user_id))
            .order_by_desc(d::Column::LastSeenTime)
            .all(self.orm_db())
            .await
    }

    pub async fn get_device(
        &self,
        (user_id, machine_id): (UserIdInDb, Uuid),
    ) -> Result<Option<entity::devices::Model>, DbErr> {
        use entity::devices as d;

        d::Entity::find()
            .filter(d::Column::UserId.eq(user_id))
            .filter(d::Column::MachineId.eq(machine_id.to_string()))
            .one(self.orm_db())
            .await
    }

    pub async fn set_device_alias(
        &self,
        (user_id, machine_id): (UserIdInDb, Uuid),
        alias: String,
    ) -> Result<bool, DbErr> {
        let result = sqlx::query(
            r#"
            UPDATE devices
            SET alias = ?
            WHERE user_id = ? AND machine_id = ?
            "#,
        )
        .bind(alias)
        .bind(user_id)
        .bind(machine_id.to_string())
        .execute(&self.db)
        .await
        .map_err(sqlx_db_error)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn is_device_blocked(
        &self,
        (user_id, machine_id): (UserIdInDb, Uuid),
    ) -> Result<bool, DbErr> {
        use entity::blocked_devices as b;

        Ok(b::Entity::find()
            .filter(b::Column::UserId.eq(user_id))
            .filter(b::Column::MachineId.eq(machine_id.to_string()))
            .one(self.orm_db())
            .await?
            .is_some())
    }

    pub async fn list_blocked_devices(
        &self,
        user_id: UserIdInDb,
    ) -> Result<Vec<entity::blocked_devices::Model>, DbErr> {
        use entity::blocked_devices as b;

        b::Entity::find()
            .filter(b::Column::UserId.eq(user_id))
            .order_by_desc(b::Column::LastAttemptTime)
            .order_by_desc(b::Column::BlockedTime)
            .all(self.orm_db())
            .await
    }

    pub async fn unblock_device(
        &self,
        (user_id, machine_id): (UserIdInDb, Uuid),
    ) -> Result<bool, DbErr> {
        use entity::blocked_devices as b;

        let result = b::Entity::delete_many()
            .filter(b::Column::UserId.eq(user_id))
            .filter(b::Column::MachineId.eq(machine_id.to_string()))
            .exec(self.orm_db())
            .await?;
        Ok(result.rows_affected > 0)
    }

    pub async fn record_blocked_attempt(
        &self,
        (user_id, machine_id): (UserIdInDb, Uuid),
        hostname: &str,
    ) -> Result<bool, DbErr> {
        let result = sqlx::query(
            r#"
            UPDATE blocked_devices
            SET hostname = ?,
                attempt_count = attempt_count + 1,
                last_attempt_time = ?
            WHERE user_id = ? AND machine_id = ?
            "#,
        )
        .bind(hostname)
        .bind(chrono::Local::now().fixed_offset())
        .bind(user_id)
        .bind(machine_id.to_string())
        .execute(&self.db)
        .await
        .map_err(sqlx_db_error)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_device_with_optional_block(
        &self,
        (user_id, machine_id): (UserIdInDb, Uuid),
        block: bool,
    ) -> Result<bool, DbErr> {
        let machine_id_value = machine_id.to_string();
        let mut transaction = self
            .db
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(sqlx_db_error)?;
        let hostname = sqlx::query_scalar::<_, String>(
            r#"
            SELECT hostname
            FROM devices
            WHERE user_id = ? AND machine_id = ?
            "#,
        )
        .bind(user_id)
        .bind(&machine_id_value)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(sqlx_db_error)?;
        let Some(hostname) = hostname else {
            transaction.commit().await.map_err(sqlx_db_error)?;
            return Ok(false);
        };

        let has_assets = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM asset_gateway_groups WHERE owner_user_id = ? AND machine_id = ? \
             UNION ALL SELECT 1 FROM asset_devices WHERE owner_user_id = ? AND machine_id = ?)",
        )
        .bind(user_id)
        .bind(&machine_id_value)
        .bind(user_id)
        .bind(&machine_id_value)
        .fetch_one(&mut *transaction)
        .await
        .map_err(sqlx_db_error)?;
        if has_assets {
            transaction.rollback().await.map_err(sqlx_db_error)?;
            return Err(DbErr::Custom("node_has_assets: remove its gateway groups and devices first".into()));
        }

        if block {
            sqlx::query(
                r#"
                INSERT INTO blocked_devices (
                    user_id, machine_id, hostname, blocked_time,
                    attempt_count, last_attempt_time
                ) VALUES (?, ?, ?, ?, 0, NULL)
                ON CONFLICT(user_id, machine_id) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(&machine_id_value)
            .bind(hostname)
            .bind(chrono::Local::now().fixed_offset())
            .execute(&mut *transaction)
            .await
            .map_err(sqlx_db_error)?;
        }

        if let Err(error) = central_intent::remove_device_from_central_intents(
            &mut transaction,
            user_id,
            machine_id,
        )
        .await
        {
            transaction.rollback().await.map_err(sqlx_db_error)?;
            return Err(DbErr::Custom(format!(
                "remove device from central network intents: {error}"
            )));
        }

        let result = sqlx::query(
            r#"
            DELETE FROM devices
            WHERE user_id = ? AND machine_id = ?
            "#,
        )
        .bind(user_id)
        .bind(&machine_id_value)
        .execute(&mut *transaction)
        .await
        .map_err(sqlx_db_error)?;
        sqlx::query(
            r#"
            DELETE FROM user_running_network_configs
            WHERE user_id = ? AND device_id = ?
            "#,
        )
        .bind(user_id)
        .bind(&machine_id_value)
        .execute(&mut *transaction)
        .await
        .map_err(sqlx_db_error)?;
        sqlx::query(
            r#"
            DELETE FROM managed_config_revisions
            WHERE user_id = ? AND device_id = ?
            "#,
        )
        .bind(user_id)
        .bind(&machine_id_value)
        .execute(&mut *transaction)
        .await
        .map_err(sqlx_db_error)?;
        transaction.commit().await.map_err(sqlx_db_error)?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use easytier::common::config::{ConfigSource, NetworkConfig};
    use easytier_core::management::remote_client::{ListNetworkProps, Storage};
    use sea_orm::{EntityTrait, Set};

    use super::{
        Db, DeviceConnectionObservation, DeviceHeartbeatRecord, IpLocationStatus,
        entity::blocked_devices,
    };

    async fn register_observation_device(db: &Db, user_id: i32, machine_id: uuid::Uuid) {
        db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id,
            machine_id,
            hostname: "connection-test".to_string(),
            easytier_version: "test".to_string(),
            device_os: "{}".to_string(),
            client_url: "tcp://192.0.2.1:1001".to_string(),
        })
        .await
        .unwrap();
    }

    fn connection_observation() -> DeviceConnectionObservation {
        DeviceConnectionObservation {
            connection_ip: "192.0.2.1".to_string(),
            ip_country: Some("example-country".to_string()),
            ip_region: Some("example-region".to_string()),
            ip_city: Some("example-city".to_string()),
            ip_location_status: IpLocationStatus::Resolved,
            ip_observed_at: chrono::DateTime::parse_from_rfc3339("2026-10-22T10:00:00Z").unwrap(),
        }
    }

    #[tokio::test]
    async fn manual_attributes_are_tenant_scoped_and_preserve_other_metadata() {
        let db = Db::memory_db().await;
        let owner = db.auto_create_user("attributes-owner").await.unwrap().id;
        let other = db.auto_create_user("attributes-other").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        for user_id in [owner, other] {
            register_observation_device(&db, user_id, machine_id).await;
        }
        db.set_device_alias((owner, machine_id), "existing alias".into())
            .await
            .unwrap();
        db.update_device_connection_observation((owner, machine_id), &connection_observation())
            .await
            .unwrap();
        let before = db.get_device((owner, machine_id)).await.unwrap().unwrap();
        let other_before = db.get_device((other, machine_id)).await.unwrap().unwrap();
        assert_eq!(
            (before.site_location.clone(), before.device_model.clone()),
            (None, None)
        );
        assert!(
            db.set_device_attributes((owner, machine_id), Some("家中机柜"), Some("OpenWrt R4S"))
                .await
                .unwrap()
        );
        let mut expected = before.clone();
        expected.site_location = Some("家中机柜".into());
        expected.device_model = Some("OpenWrt R4S".into());
        assert_eq!(
            db.get_device((owner, machine_id)).await.unwrap().unwrap(),
            expected
        );
        assert_eq!(
            db.get_device((other, machine_id)).await.unwrap().unwrap(),
            other_before
        );

        // A reconnect updates both heartbeat metadata and its source snapshot;
        // neither writer is allowed to clear the user-maintained attributes.
        register_observation_device(&db, owner, machine_id).await;
        let next = DeviceConnectionObservation {
            connection_ip: "192.0.2.9".into(),
            ..connection_observation()
        };
        db.update_device_connection_observation((owner, machine_id), &next)
            .await
            .unwrap();
        let reconnected = db.get_device((owner, machine_id)).await.unwrap().unwrap();
        assert_eq!(reconnected.site_location, expected.site_location);
        assert_eq!(reconnected.device_model, expected.device_model);
        assert_eq!(reconnected.alias, before.alias);
        assert_eq!(reconnected.connection_ip, Some(next.connection_ip));

        assert!(
            db.set_device_attributes((owner, machine_id), None, None)
                .await
                .unwrap()
        );
        let cleared = db.get_device((owner, machine_id)).await.unwrap().unwrap();
        let mut expected = reconnected;
        expected.site_location = None;
        expected.device_model = None;
        assert_eq!(cleared, expected);
    }

    #[tokio::test]
    async fn manual_attributes_do_not_register_missing_nodes_or_partially_save() {
        let db = Db::memory_db().await;
        let owner = db.auto_create_user("attributes-atomic").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        assert!(
            !db.set_device_attributes((owner, machine_id), Some("Office"), Some("Router"))
                .await
                .unwrap()
        );
        assert!(db.get_device((owner, machine_id)).await.unwrap().is_none());
        register_observation_device(&db, owner, machine_id).await;
        db.set_device_attributes((owner, machine_id), Some("Before"), Some("Original"))
            .await
            .unwrap();
        let before = db.get_device((owner, machine_id)).await.unwrap().unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_manual_attributes BEFORE UPDATE OF device_model ON devices \
             BEGIN SELECT RAISE(ABORT, 'injected attribute storage failure'); END",
        )
        .execute(&db.db)
        .await
        .unwrap();
        assert!(
            db.set_device_attributes((owner, machine_id), Some("After"), Some("Changed"))
                .await
                .is_err()
        );
        assert_eq!(
            db.get_device((owner, machine_id)).await.unwrap().unwrap(),
            before
        );
    }

    #[tokio::test]
    async fn connection_observation_is_tenant_scoped_and_survives_heartbeat() {
        let db = Db::memory_db().await;
        let user_a = db.auto_create_user("connection-owner-a").await.unwrap().id;
        let user_b = db.auto_create_user("connection-owner-b").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        for user_id in [user_a, user_b] {
            register_observation_device(&db, user_id, machine_id).await;
        }
        let before_b = db.get_device((user_b, machine_id)).await.unwrap().unwrap();
        assert_eq!(
            before_b.ip_location_status,
            IpLocationStatus::Unknown.as_str()
        );
        assert_eq!(before_b.connection_ip, None);
        let observation = connection_observation();
        assert!(
            db.update_device_connection_observation((user_a, machine_id), &observation)
                .await
                .unwrap()
        );
        register_observation_device(&db, user_a, machine_id).await;
        let after_a = db.get_device((user_a, machine_id)).await.unwrap().unwrap();
        assert_eq!(after_a.connection_ip, Some(observation.connection_ip));
        assert_eq!(after_a.ip_country, observation.ip_country);
        assert_eq!(after_a.ip_region, observation.ip_region);
        assert_eq!(after_a.ip_city, observation.ip_city);
        assert_eq!(after_a.ip_location_status, "resolved");
        assert_eq!(after_a.ip_observed_at, Some(observation.ip_observed_at));
        assert_eq!(
            db.get_device((user_b, machine_id)).await.unwrap().unwrap(),
            before_b
        );
    }

    #[tokio::test]
    async fn changed_ip_with_failed_lookup_clears_previous_location() {
        let db = Db::memory_db().await;
        let user_id = db.auto_create_user("connection-clear").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        register_observation_device(&db, user_id, machine_id).await;
        let before = db.get_device((user_id, machine_id)).await.unwrap().unwrap();
        db.update_device_connection_observation((user_id, machine_id), &connection_observation())
            .await
            .unwrap();
        let failed = DeviceConnectionObservation {
            connection_ip: "2001:db8::2".to_string(),
            ip_country: None,
            ip_region: None,
            ip_city: None,
            ip_location_status: IpLocationStatus::Unavailable,
            ip_observed_at: chrono::DateTime::parse_from_rfc3339("2026-10-22T11:00:00Z").unwrap(),
        };
        assert!(
            db.update_device_connection_observation((user_id, machine_id), &failed)
                .await
                .unwrap()
        );
        let after = db.get_device((user_id, machine_id)).await.unwrap().unwrap();
        assert_eq!(after.connection_ip.as_deref(), Some("2001:db8::2"));
        assert_eq!(
            (after.ip_country, after.ip_region, after.ip_city),
            (None, None, None)
        );
        assert_eq!(after.ip_location_status, "unavailable");
        assert_eq!(after.ip_observed_at, Some(failed.ip_observed_at));
        assert_eq!(after.first_seen_time, before.first_seen_time);
        assert_eq!(after.last_seen_time, before.last_seen_time);
        assert_eq!(after.client_url, before.client_url);
    }

    #[tokio::test]
    async fn failed_observation_write_preserves_the_whole_previous_snapshot() {
        let db = Db::memory_db().await;
        let user_id = db.auto_create_user("connection-rollback").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        register_observation_device(&db, user_id, machine_id).await;
        db.update_device_connection_observation((user_id, machine_id), &connection_observation())
            .await
            .unwrap();
        let before = db.get_device((user_id, machine_id)).await.unwrap().unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_connection_snapshot BEFORE UPDATE OF ip_city ON devices \
             BEGIN SELECT RAISE(ABORT, 'injected snapshot storage failure'); END",
        )
        .execute(&db.db)
        .await
        .unwrap();
        let next = DeviceConnectionObservation {
            connection_ip: "192.0.2.2".to_string(),
            ip_country: None,
            ip_region: None,
            ip_city: None,
            ip_location_status: IpLocationStatus::NotFound,
            ip_observed_at: chrono::DateTime::parse_from_rfc3339("2026-10-22T11:00:00Z").unwrap(),
        };
        assert!(
            db.update_device_connection_observation((user_id, machine_id), &next)
                .await
                .is_err()
        );
        assert_eq!(
            db.get_device((user_id, machine_id)).await.unwrap().unwrap(),
            before
        );
    }

    #[tokio::test]
    async fn connection_observation_does_not_register_missing_or_blocked_devices() {
        let db = Db::memory_db().await;
        let user_id = db.auto_create_user("connection-deleted").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        let observation = connection_observation();
        assert!(
            !db.update_device_connection_observation((user_id, machine_id), &observation)
                .await
                .unwrap()
        );
        assert!(
            db.get_device((user_id, machine_id))
                .await
                .unwrap()
                .is_none()
        );

        register_observation_device(&db, user_id, machine_id).await;
        insert_blocked_device(&db, (user_id, machine_id), "connection-test").await;
        let blocked = db.get_device((user_id, machine_id)).await.unwrap().unwrap();
        assert!(
            !db.update_device_connection_observation((user_id, machine_id), &observation)
                .await
                .unwrap()
        );
        assert_eq!(
            db.get_device((user_id, machine_id)).await.unwrap().unwrap(),
            blocked
        );
        db.delete_device_with_optional_block((user_id, machine_id), false)
            .await
            .unwrap();
        assert!(
            !db.update_device_connection_observation((user_id, machine_id), &observation)
                .await
                .unwrap()
        );
        assert!(
            db.get_device((user_id, machine_id))
                .await
                .unwrap()
                .is_none()
        );
    }

    async fn insert_blocked_device(
        db: &Db,
        (user_id, machine_id): (i32, uuid::Uuid),
        hostname: &str,
    ) {
        blocked_devices::Entity::insert(blocked_devices::ActiveModel {
            user_id: Set(user_id),
            machine_id: Set(machine_id.to_string()),
            hostname: Set(hostname.to_string()),
            blocked_time: Set(chrono::Local::now().fixed_offset()),
            attempt_count: Set(0),
            last_attempt_time: Set(None),
        })
        .exec(db.orm_db())
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn registered_device_identity_is_scoped_by_tenant() {
        let db = Db::memory_db().await;
        let user_a = db.auto_create_user("device-owner-a").await.unwrap().id;
        let user_b = db.auto_create_user("device-owner-b").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();

        db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id: user_a,
            machine_id,
            hostname: "host-a".to_string(),
            easytier_version: "a".to_string(),
            device_os: "{}".to_string(),
            client_url: "tcp://127.0.0.1:1001".to_string(),
        })
        .await
        .unwrap();
        db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id: user_b,
            machine_id,
            hostname: "host-b".to_string(),
            easytier_version: "b".to_string(),
            device_os: "{}".to_string(),
            client_url: "tcp://127.0.0.1:1002".to_string(),
        })
        .await
        .unwrap();

        assert_eq!(
            db.get_device((user_a, machine_id))
                .await
                .unwrap()
                .unwrap()
                .hostname,
            "host-a"
        );
        assert_eq!(
            db.get_device((user_b, machine_id))
                .await
                .unwrap()
                .unwrap()
                .hostname,
            "host-b"
        );
        assert_eq!(db.list_devices(user_a).await.unwrap().len(), 1);
        assert_eq!(db.list_devices(user_b).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn heartbeat_upsert_preserves_alias_and_first_seen() {
        let db = Db::memory_db().await;
        let user_id = db.auto_create_user("device-upsert").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();

        db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id,
            machine_id,
            hostname: "before".to_string(),
            easytier_version: "1".to_string(),
            device_os: r#"{"os":"before"}"#.to_string(),
            client_url: "tcp://127.0.0.1:1001".to_string(),
        })
        .await
        .unwrap();
        assert!(
            db.set_device_alias((user_id, machine_id), "console-name".to_string())
                .await
                .unwrap()
        );
        let before = db.get_device((user_id, machine_id)).await.unwrap().unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id,
            machine_id,
            hostname: "after".to_string(),
            easytier_version: "2".to_string(),
            device_os: r#"{"os":"after"}"#.to_string(),
            client_url: "tcp://127.0.0.1:1002".to_string(),
        })
        .await
        .unwrap();

        let after = db.get_device((user_id, machine_id)).await.unwrap().unwrap();
        assert_eq!(after.alias, "console-name");
        assert_eq!(after.first_seen_time, before.first_seen_time);
        assert!(after.last_seen_time > before.last_seen_time);
        assert_eq!(after.hostname, "after");
        assert_eq!(after.easytier_version, "2");
        assert_eq!(after.device_os, r#"{"os":"after"}"#);
        assert_eq!(after.client_url, "tcp://127.0.0.1:1002");
    }

    #[tokio::test]
    async fn blocklist_is_scoped_by_tenant() {
        let db = Db::memory_db().await;
        let user_a = db.auto_create_user("blocked-owner-a").await.unwrap().id;
        let user_b = db.auto_create_user("blocked-owner-b").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();

        insert_blocked_device(&db, (user_a, machine_id), "host-a").await;

        assert!(db.is_device_blocked((user_a, machine_id)).await.unwrap());
        assert!(!db.is_device_blocked((user_b, machine_id)).await.unwrap());
        assert!(!db.unblock_device((user_b, machine_id)).await.unwrap());
        assert!(db.is_device_blocked((user_a, machine_id)).await.unwrap());
        let blocked_a = db.list_blocked_devices(user_a).await.unwrap();
        assert_eq!(blocked_a.len(), 1);
        assert_eq!(blocked_a[0].hostname, "host-a");
        assert!(db.list_blocked_devices(user_b).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn blocked_attempt_increments_are_atomic() {
        const ATTEMPTS: usize = 32;

        let db = Db::memory_db().await;
        let user_a = db.auto_create_user("attempt-owner-a").await.unwrap().id;
        let user_b = db.auto_create_user("attempt-owner-b").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        insert_blocked_device(&db, (user_a, machine_id), "host-a").await;
        insert_blocked_device(&db, (user_b, machine_id), "host-b").await;

        let mut tasks = Vec::with_capacity(ATTEMPTS);
        for _ in 0..ATTEMPTS {
            let db = db.clone();
            tasks.push(tokio::spawn(async move {
                db.record_blocked_attempt((user_a, machine_id), "host-a-new")
                    .await
                    .unwrap()
            }));
        }
        for task in tasks {
            assert!(task.await.unwrap());
        }

        let blocked_a = db.list_blocked_devices(user_a).await.unwrap();
        let blocked_b = db.list_blocked_devices(user_b).await.unwrap();
        assert_eq!(blocked_a[0].attempt_count, ATTEMPTS as i32);
        assert_eq!(blocked_a[0].hostname, "host-a-new");
        assert!(blocked_a[0].last_attempt_time.is_some());
        assert_eq!(blocked_b[0].attempt_count, 0);
        assert_eq!(blocked_b[0].hostname, "host-b");
    }

    #[tokio::test]
    async fn deleting_device_preserves_or_creates_tenant_block() {
        let db = Db::memory_db().await;
        let user_a = db.auto_create_user("delete-owner-a").await.unwrap().id;
        let user_b = db.auto_create_user("delete-owner-b").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        for (user_id, hostname) in [(user_a, "host-a"), (user_b, "host-b")] {
            db.upsert_device_heartbeat(DeviceHeartbeatRecord {
                user_id,
                machine_id,
                hostname: hostname.to_string(),
                easytier_version: "1".to_string(),
                device_os: "{}".to_string(),
                client_url: format!("tcp://127.0.0.1:{user_id}"),
            })
            .await
            .unwrap();
            db.insert_or_update_user_network_config(
                (user_id, machine_id),
                uuid::Uuid::new_v4(),
                NetworkConfig::default(),
                ConfigSource::Web,
            )
            .await
            .unwrap();
            db.set_managed_config_revision((user_id, machine_id), "rev-1")
                .await
                .unwrap();
        }

        insert_blocked_device(&db, (user_a, machine_id), "blocked-host-a").await;
        assert!(
            db.delete_device_with_optional_block((user_a, machine_id), false)
                .await
                .unwrap()
        );
        assert!(db.get_device((user_a, machine_id)).await.unwrap().is_none());
        assert!(db.is_device_blocked((user_a, machine_id)).await.unwrap());
        assert!(db.get_device((user_b, machine_id)).await.unwrap().is_some());
        assert!(
            db.list_network_configs((user_a, machine_id), ListNetworkProps::All)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.get_managed_config_revision((user_a, machine_id))
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            db.list_network_configs((user_b, machine_id), ListNetworkProps::All)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            db.get_managed_config_revision((user_b, machine_id))
                .await
                .unwrap()
                .as_deref(),
            Some("rev-1")
        );

        assert!(
            db.delete_device_with_optional_block((user_b, machine_id), true)
                .await
                .unwrap()
        );
        assert!(db.get_device((user_b, machine_id)).await.unwrap().is_none());
        let blocked_b = db.list_blocked_devices(user_b).await.unwrap();
        assert_eq!(blocked_b.len(), 1);
        assert_eq!(blocked_b[0].hostname, "host-b");
    }
}
