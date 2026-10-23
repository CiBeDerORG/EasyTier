//! Loopback transport/authentication coverage for persisted connection metadata.

use super::*;
use crate::{
    central_network::device_auth::DeviceAuth,
    client_manager::ClientManager,
    db::{Db, DeviceHeartbeatRecord, IpLocationStatus},
    webhook::WebhookConfig,
};
use easytier::proto::{
    rpc::standalone::{
        runtime_rpc_dialer, runtime_rpc_listener, runtime_udp_tunnel_dialer,
        runtime_udp_tunnel_listener,
    },
    web::{GetFeatureRequest, WebServerServiceClientFactory},
};
use easytier_core::connectivity::protocol::raw::TunnelDialer as _;

async fn wait_until<F, Fut>(mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    tokio::time::timeout(Duration::from_secs(10), async {
        while !condition().await {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("connection metadata did not converge");
}

async fn connect(
    manager: &ClientManager,
    listener_url: url::Url,
) -> (BidirectRpcManager, Arc<Session>) {
    let tunnel = if listener_url.scheme() == "udp" {
        runtime_udp_tunnel_dialer(listener_url)
            .connect()
            .await
            .unwrap()
    } else {
        runtime_rpc_dialer(listener_url).connect().await.unwrap()
    };
    let local_url: url::Url = tunnel.info().unwrap().local_addr.unwrap().into();
    let rpc = BidirectRpcManager::new();
    rpc.run_with_tunnel(tunnel);
    let client = rpc
        .rpc_client()
        .scoped_client::<WebServerServiceClientFactory<BaseController>>(1, 1, String::new());
    tokio::time::timeout(
        Duration::from_secs(5),
        client.get_feature(BaseController::default(), GetFeatureRequest {}),
    )
    .await
    .unwrap()
    .unwrap();
    let session = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(session) = manager
                .client_sessions
                .iter()
                .find(|entry| entry.key().port() == local_url.port())
                .map(|entry| entry.value().clone())
            {
                return session;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    (rpc, session)
}

fn request(token: &str, machine_id: uuid::Uuid) -> HeartbeatRequest {
    HeartbeatRequest {
        machine_id: Some(machine_id.into()),
        user_token: token.to_owned(),
        hostname: "connection-test-node".to_owned(),
        report_time: chrono::Utc::now().to_rfc3339(),
        support_heartbeat_policy: true,
        ..Default::default()
    }
}

async fn heartbeat(rpc: &BidirectRpcManager, token: &str, machine_id: uuid::Uuid) {
    rpc.rpc_client()
        .scoped_client::<WebServerServiceClientFactory<BaseController>>(1, 1, String::new())
        .heartbeat(BaseController::default(), request(token, machine_id))
        .await
        .unwrap();
}

#[tokio::test]
async fn tcp_and_udp_registration_persist_source_once_and_refresh_on_reconnect() {
    for (protocol, host, expected_ip) in [
        ("tcp", "127.0.0.1", "127.0.0.1"),
        ("udp", "127.0.0.1", "127.0.0.1"),
        ("tcp", "[::1]", "::1"),
        ("udp", "[::1]", "::1"),
    ] {
        let db = Db::memory_db().await;
        let user_id = db.auto_create_user("connection-owner").await.unwrap().id;
        let machine_id = uuid::Uuid::new_v4();
        let webhook = WebhookConfig::new(None, None, None, None, None)
            .with_handler(Arc::new(DeviceAuth::new(db.clone(), false)));
        let mut manager = ClientManager::new(
            db.clone(),
            None,
            HeartbeatPolicy::default(),
            Arc::new(FeatureFlags::default()),
            Arc::new(webhook),
        );
        let listener_url = if protocol == "udp" {
            manager
                .add_listener(runtime_udp_tunnel_listener(
                    format!("udp://{host}:0").parse().unwrap(),
                    format!("{host}:0").parse().unwrap(),
                ))
                .await
                .unwrap()
        } else {
            manager
                .add_listener(runtime_rpc_listener(format!("{host}:0").parse().unwrap()))
                .await
                .unwrap()
        };
        let (rpc, session) = connect(&manager, listener_url.clone()).await;

        // Transport handshake alone must not register or persist a node.
        assert!(session.get_token().await.is_none());
        assert!(
            db.get_device((user_id, machine_id))
                .await
                .unwrap()
                .is_none()
        );
        heartbeat(&rpc, "connection-owner", machine_id).await;
        wait_until(|| async {
            db.get_device((user_id, machine_id))
                .await
                .unwrap()
                .is_some_and(|device| device.ip_observed_at.is_some())
        })
        .await;
        let first = db.get_device((user_id, machine_id)).await.unwrap().unwrap();
        assert_eq!(first.connection_ip.as_deref(), Some(expected_ip));
        assert_eq!(first.ip_location_status, "non_public");
        assert_eq!(
            (first.ip_country, first.ip_region, first.ip_city),
            (None, None, None)
        );
        assert_eq!(session.get_token().await.unwrap().user_id, user_id);

        // Trigger the existing same-session validation cadence. Baseline
        // registration metadata changes, while the source snapshot stays put.
        for _ in 0..10 {
            heartbeat(&rpc, "connection-owner", machine_id).await;
        }
        wait_until(|| async {
            db.get_device((user_id, machine_id))
                .await
                .unwrap()
                .unwrap()
                .last_seen_time
                > first.last_seen_time
        })
        .await;
        assert_eq!(
            db.get_device((user_id, machine_id))
                .await
                .unwrap()
                .unwrap()
                .ip_observed_at,
            first.ip_observed_at,
        );

        // An invalid credential on another connection must not alter this row.
        let (invalid_rpc, invalid_session) = connect(&manager, listener_url.clone()).await;
        heartbeat(&invalid_rpc, "unknown-connection-token", machine_id).await;
        wait_until(|| async {
            invalid_session.data.read().await.auth_state == SessionAuthState::Invalid
        })
        .await;
        assert!(invalid_session.get_token().await.is_none());
        assert_eq!(
            db.get_device((user_id, machine_id))
                .await
                .unwrap()
                .unwrap()
                .ip_observed_at,
            first.ip_observed_at,
        );
        invalid_session.stop().await;
        invalid_rpc.stop().await;

        session.stop().await;
        rpc.stop().await;
        let offline = db.get_device((user_id, machine_id)).await.unwrap().unwrap();
        assert_eq!(offline.connection_ip.as_deref(), Some(expected_ip));
        assert_eq!(offline.ip_observed_at, first.ip_observed_at);

        let (reconnected_rpc, reconnected_session) = connect(&manager, listener_url).await;
        heartbeat(&reconnected_rpc, "connection-owner", machine_id).await;
        wait_until(|| async {
            db.get_device((user_id, machine_id))
                .await
                .unwrap()
                .unwrap()
                .ip_observed_at
                > first.ip_observed_at
        })
        .await;
        reconnected_session.stop().await;
        reconnected_rpc.stop().await;
    }
}

async fn registered_session(
    external: bool,
) -> (Storage, Arc<RwLock<SessionData>>, i32, uuid::Uuid) {
    let db = Db::memory_db().await;
    let user_id = db.auto_create_user("observation-owner").await.unwrap().id;
    let machine_id = uuid::Uuid::new_v4();
    db.upsert_device_heartbeat(DeviceHeartbeatRecord {
        user_id,
        machine_id,
        hostname: "node".into(),
        easytier_version: "test".into(),
        device_os: "{}".into(),
        client_url: String::new(),
    })
    .await
    .unwrap();
    let storage = Storage::new(db);
    let webhook = WebhookConfig::new(
        external.then(|| "http://127.0.0.1:1".into()),
        None,
        None,
        None,
        None,
    );
    let mut data = SessionData::new(
        storage.weak_ref(),
        "tcp://127.0.0.1:12345".parse().unwrap(),
        None,
        Arc::new(FeatureFlags::default()),
        Arc::new(webhook),
    );
    data.req = Some(request("observation-owner", machine_id));
    data.connection_observation = Some(DeviceConnectionObservation {
        connection_ip: "127.0.0.1".into(),
        ip_country: None,
        ip_region: None,
        ip_city: None,
        ip_location_status: IpLocationStatus::NonPublic,
        ip_observed_at: chrono::Utc::now().fixed_offset(),
    });
    data.session_epoch = 1;
    // Model the callback as already delivered so the external-mode unit case
    // exercises persistence gating without issuing an HTTP callback.
    data.webhook_connected_binding_version = Some(0);
    (storage, Arc::new(RwLock::new(data)), user_id, machine_id)
}

async fn apply_authenticated(
    storage: &Storage,
    data: &Arc<RwLock<SessionData>>,
    user_id: i32,
    machine_id: uuid::Uuid,
) {
    let (input, epoch) = {
        let data = data.read().await;
        (
            webhook_validation::WebhookValidationInput {
                storage: storage.clone(),
                webhook_config: data.webhook_config.clone(),
                client_url: data.client_url.clone(),
                applied_config_revision: None,
                applied_config_revision_known: false,
                failed_instance_ids: Vec::new(),
                req: data.req.clone().unwrap(),
                machine_id,
            },
            data.webhook_validation_change_epoch,
        )
    };
    webhook_validation::apply_success(
        &Arc::downgrade(data),
        input,
        webhook_validation::WebhookHeartbeatValidation {
            config_revision: String::new(),
            binding_version: 0,
        },
        user_id,
        epoch,
    )
    .await;
}

#[tokio::test]
async fn external_webhook_authentication_does_not_write_local_connection_snapshot() {
    let (storage, data, user_id, machine_id) = registered_session(true).await;
    apply_authenticated(&storage, &data, user_id, machine_id).await;
    assert_eq!(data.read().await.auth_state, SessionAuthState::Authorized);
    let row = storage
        .db()
        .get_device((user_id, machine_id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.connection_ip, None);
    assert_eq!(row.ip_location_status, "unknown");
    assert_eq!(row.ip_observed_at, None);
}

#[tokio::test]
async fn snapshot_database_failure_does_not_reject_authentication_or_repeat_on_revalidation() {
    let (storage, data, user_id, machine_id) = registered_session(false).await;
    sqlx::query(
        "CREATE TRIGGER reject_connection_snapshot BEFORE UPDATE OF connection_ip ON devices \
         BEGIN SELECT RAISE(ABORT, 'injected snapshot failure'); END",
    )
    .execute(&storage.db().inner())
    .await
    .unwrap();
    let mut notifications = data.read().await.heartbeat_waiter();
    apply_authenticated(&storage, &data, user_id, machine_id).await;
    assert_eq!(data.read().await.auth_state, SessionAuthState::Authorized);
    notifications
        .try_recv()
        .expect("config runtime must be notified despite metadata failure");
    assert!(
        storage
            .get_client_url_by_machine_id(user_id, &machine_id)
            .is_some()
    );
    let row = storage
        .db()
        .get_device((user_id, machine_id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.connection_ip, None);
    assert_eq!(row.ip_observed_at, None);

    sqlx::query("DROP TRIGGER reject_connection_snapshot")
        .execute(&storage.db().inner())
        .await
        .unwrap();
    apply_authenticated(&storage, &data, user_id, machine_id).await;
    assert_eq!(
        storage
            .db()
            .get_device((user_id, machine_id))
            .await
            .unwrap()
            .unwrap()
            .connection_ip,
        None,
        "same-session revalidation must not introduce a metadata retry",
    );
}
