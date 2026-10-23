use axum::{body::Body as RequestBody, http::Method};
use tower::ServiceExt as _;

use super::*;
use crate::{
    client_manager::{ClientManager, HeartbeatPolicy},
    db::{DeviceHeartbeatRecord, entity::users::Model as Account},
    restful::users::{Backend, User},
};

struct Fixture {
    db: Db,
    manager: Arc<ClientManager>,
    service: Arc<CentralNetworkService>,
}

impl Fixture {
    async fn new() -> Self {
        let db = Db::memory_db().await;
        let manager = Arc::new(ClientManager::new(
            db.clone(),
            None,
            HeartbeatPolicy::default(),
            Arc::new(crate::FeatureFlags::default()),
            Arc::new(crate::webhook::WebhookConfig::new(None, None, None, None, None)),
        ));
        let service = Arc::new(CentralNetworkService::new(db.clone(), manager.clone()));
        Self { db, manager, service }
    }

    async fn account(&self, name: &str) -> Account {
        self.db.auto_create_user(name).await.unwrap()
    }

    async fn make_admin(&self, user_id: i32) {
        sqlx::query("INSERT INTO groups(name) VALUES('admins') ON CONFLICT(name) DO NOTHING")
            .execute(&self.db.inner()).await.unwrap();
        sqlx::query("INSERT INTO users_groups(user_id,group_id) SELECT ?,id FROM groups WHERE name = 'admins'")
            .bind(user_id).execute(&self.db.inner()).await.unwrap();
    }

    async fn register_node(&self, owner: i32, node: Uuid) {
        self.db.upsert_device_heartbeat(DeviceHeartbeatRecord {
            user_id: owner,
            machine_id: node,
            hostname: "asset-router".into(),
            easytier_version: "test".into(),
            device_os: "{}".into(),
            client_url: "tcp://192.0.2.1:1000".into(),
        }).await.unwrap();
    }

    async fn request(
        &self,
        user: Option<Account>,
        webhook_auth: bool,
        method: Method,
        path: &str,
        payload: Option<Value>,
    ) -> (StatusCode, Value) {
        let session_store = tower_sessions_sqlx_store::SqliteStore::new(self.db.inner());
        session_store.migrate().await.unwrap();
        let auth_layer = axum_login::AuthManagerLayerBuilder::new(
            Backend::new(self.db.clone()),
            axum_login::tower_sessions::SessionManagerLayer::new(session_store),
        ).build();
        let app = router()
            .layer(axum::middleware::from_fn(
                move |mut request: axum::extract::Request, next: axum::middleware::Next| {
                    let user = user.clone();
                    async move {
                        request.extensions_mut().get_mut::<AuthSession>().unwrap().user =
                            user.map(|db_user| User { db_user, tokens: vec![] });
                        next.run(request).await
                    }
                },
            ))
            .layer(Extension(ConsoleInfoConfig {
                config_server_protocol: "tcp".into(),
                config_server_port: 22020,
                webhook_auth,
            }))
            .layer(Extension(self.service.clone()))
            .layer(auth_layer)
            .with_state(self.manager.clone());
        let mut request = axum::http::Request::builder().method(method).uri(path);
        let body = if let Some(payload) = payload {
            request = request.header(axum::http::header::CONTENT_TYPE, "application/json");
            RequestBody::from(payload.to_string())
        } else {
            RequestBody::empty()
        };
        let response = app.oneshot(request.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let body = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
        (status, body)
    }
}

#[tokio::test]
async fn anonymous_asset_requests_return_json_authentication_error() {
    let fixture = Fixture::new().await;
    let (status, body) = fixture.request(None, false, Method::GET, "/api/v1/assets/context", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "authentication_required");
    assert!(body["field_errors"].is_object());
    assert!(body["message"].as_str().is_some_and(|message| !message.is_empty()));
}

#[tokio::test]
async fn ordinary_user_does_not_inherit_another_users_admin_membership() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("asset-owner").await;
    let other = fixture.account("asset-other").await;
    let admin = fixture.account("asset-admin").await;
    fixture.make_admin(admin.id).await;
    let node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    let before = fixture.db.get_device((owner.id, node)).await.unwrap().unwrap();

    let (status, context) = fixture.request(Some(other.clone()), false, Method::GET, "/api/v1/assets/context", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["can_manage_others"], false);
    let (status, _) = fixture.request(Some(other.clone()), false, Method::GET, "/api/v1/assets/accounts", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = fixture.request(
        Some(other.clone()), false, Method::GET,
        &format!("/api/v1/assets/nodes?owner_user_id={}", owner.id), None,
    ).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "asset_access_denied");
    let (status, _) = fixture.request(
        Some(other), false, Method::PATCH,
        &format!("/api/v1/assets/nodes/{node}/metadata?owner_user_id={}", owner.id),
        Some(json!({"is_public_server":true,"site_location":"unauthorized","device_model":"changed"})),
    ).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(fixture.db.get_device((owner.id, node)).await.unwrap().unwrap(), before);
}

#[tokio::test]
async fn administrator_writes_to_target_owner_and_revocation_applies_on_next_request() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("asset-admin-target").await;
    let admin = fixture.account("asset-admin-actor").await;
    fixture.make_admin(admin.id).await;
    let (status, created) = fixture.request(
        Some(admin.clone()), false, Method::POST,
        &format!("/api/v1/assets/device-types?owner_user_id={}", owner.id),
        Some(json!({"request_id":Uuid::new_v4(),"name":"Managed by admin"})),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["owner_user_id"], owner.id);
    let (status, listed) = fixture.request(
        Some(owner.clone()), false, Method::GET,
        &format!("/api/v1/assets/device-types?owner_user_id={}", owner.id), None,
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"][0]["id"], created["id"]);
    let (status, own_types) = fixture.request(
        Some(admin.clone()), false, Method::GET,
        &format!("/api/v1/assets/device-types?owner_user_id={}", admin.id), None,
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(own_types["items"], json!([]));

    sqlx::query("DELETE FROM users_groups WHERE user_id = ? AND group_id IN (SELECT id FROM groups WHERE name = 'admins')")
        .bind(admin.id).execute(&fixture.db.inner()).await.unwrap();
    let (status, body) = fixture.request(
        Some(admin), false, Method::DELETE,
        &format!("/api/v1/assets/device-types/{}?owner_user_id={}", created["id"].as_str().unwrap(), owner.id), None,
    ).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "asset_access_denied");
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_device_types WHERE owner_user_id = ?")
        .bind(owner.id).fetch_one(&fixture.db.inner()).await.unwrap();
    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn external_webhook_mode_reports_unavailable_and_does_not_initialize_assets() {
    let fixture = Fixture::new().await;
    let user = fixture.account("asset-webhook").await;
    let (status, context) = fixture.request(Some(user.clone()), true, Method::GET, "/api/v1/assets/context", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["available"], false);
    let (status, body) = fixture.request(
        Some(user.clone()), true, Method::POST,
        &format!("/api/v1/assets/device-types/initialize?owner_user_id={}", user.id), Some(json!({})),
    ).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "asset_mode_unavailable");
    let catalog_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_catalog_state")
        .fetch_one(&fixture.db.inner()).await.unwrap();
    assert_eq!(catalog_count, 0);
    let (status, _) = fixture.request(
        Some(user.clone()), true, Method::GET,
        &format!("/api/v1/assets/device-types?owner_user_id={}", user.id), None,
    ).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn deleting_node_with_children_is_rejected_before_blocking_the_node() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("asset-protected-node").await;
    let node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    let before = fixture.db.get_device((owner.id, node)).await.unwrap().unwrap();
    let store = AssetsStore::new(fixture.db.clone());
    let group = store.create_group(owner.id, owner.id, &node.to_string(), json!({
        "request_id":Uuid::new_v4(),"name":"Gateway","subnets":["192.168.1.0/24"],"remarks":null,
    })).await.unwrap();
    let url = format!("/api/v1/assets/nodes/{node}?owner_user_id={}&block=true", owner.id);
    let (status, body) = fixture.request(Some(owner.clone()), false, Method::DELETE, &url, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "asset_node_conflict");
    assert_eq!(fixture.db.get_device((owner.id, node)).await.unwrap().unwrap(), before);
    assert!(!fixture.db.is_device_blocked((owner.id, node)).await.unwrap());
    assert_eq!(store.list_groups(owner.id, &node.to_string()).await.unwrap()["items"][0]["id"], group["id"]);

    store.delete_group(owner.id, &node.to_string(), group["id"].as_str().unwrap()).await.unwrap();
    let (status, _) = fixture.request(Some(owner.clone()), false, Method::DELETE, &url, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(fixture.db.get_device((owner.id, node)).await.unwrap().is_none());
    assert!(fixture.db.is_device_blocked((owner.id, node)).await.unwrap());
}
