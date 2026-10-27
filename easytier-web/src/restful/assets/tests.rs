use axum::{body::Body as RequestBody, http::Method};
use easytier::common::config::{ConfigSource, NetworkConfig};
use easytier_core::management::remote_client::Storage as _;
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
            Arc::new(crate::webhook::WebhookConfig::new(
                None, None, None, None, None,
            )),
        ));
        let service = Arc::new(CentralNetworkService::new(db.clone(), manager.clone()));
        Self {
            db,
            manager,
            service,
        }
    }

    async fn account(&self, name: &str) -> Account {
        self.db.auto_create_user(name).await.unwrap()
    }

    async fn make_admin(&self, user_id: i32) {
        sqlx::query("INSERT INTO groups(name) VALUES('admins') ON CONFLICT(name) DO NOTHING")
            .execute(&self.db.inner())
            .await
            .unwrap();
        sqlx::query("INSERT INTO users_groups(user_id,group_id) SELECT ?,id FROM groups WHERE name = 'admins'")
            .bind(user_id).execute(&self.db.inner()).await.unwrap();
    }

    async fn register_node(&self, owner: i32, node: Uuid) {
        self.db
            .upsert_device_heartbeat(DeviceHeartbeatRecord {
                user_id: owner,
                machine_id: node,
                hostname: "asset-router".into(),
                easytier_version: "test".into(),
                device_os: "{}".into(),
                client_url: "tcp://192.0.2.1:1000".into(),
            })
            .await
            .unwrap();
    }

    async fn gateway_instance(&self, owner: i32, node: Uuid) -> Uuid {
        let instance = Uuid::new_v4();
        self.db
            .insert_or_update_user_network_config(
                (owner, node),
                instance,
                NetworkConfig {
                    network_name: Some("Bound gateway".into()),
                    virtual_ipv4: Some("10.90.0.1".into()),
                    dhcp: Some(false),
                    proxy_cidrs: vec!["192.168.1.0/24->10.20.1.0/24".into()],
                    ..Default::default()
                },
                ConfigSource::Web,
            )
            .await
            .unwrap();
        instance
    }

    async fn preview_gateway(
        &self,
        actor: Account,
        owner: i32,
        node: Uuid,
        instance: Uuid,
    ) -> Value {
        let (status, preview) = self
            .request(
                Some(actor),
                false,
                Method::POST,
                &format!(
                    "/api/v1/assets/validation/binding?owner_user_id={owner}&machine_id={node}"
                ),
                Some(json!({"subject":{"resource_kind":"gateway","id":null},
                "operation":"attach","instance_id":instance})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{preview}");
        assert_eq!(preview["can_save"], true);
        preview
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
        )
        .build();
        let app = router()
            .layer(axum::middleware::from_fn(
                move |mut request: axum::extract::Request, next: axum::middleware::Next| {
                    let user = user.clone();
                    async move {
                        request
                            .extensions_mut()
                            .get_mut::<AuthSession>()
                            .unwrap()
                            .user = user.map(|db_user| User {
                            db_user,
                            tokens: vec![],
                        });
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
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, body)
    }
}

fn gateway_payload(instance: Uuid, preview: &Value) -> Value {
    json!({"request_id":Uuid::new_v4(),"name":"Gateway","remarks":null,
        "binding":{"instance_id":instance},"preview_version":preview["preview_version"]})
}

#[tokio::test]
async fn service_catalog_and_navigation_enforce_account_and_webhook_permissions() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("navigation-owner").await;
    let other = fixture.account("navigation-other").await;
    let admin = fixture.account("navigation-admin").await;
    fixture.make_admin(admin.id).await;
    let node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    let device_id = Uuid::new_v4().to_string();
    let service_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO asset_devices(id,owner_user_id,machine_id,name,overlay_ip,created_at,updated_at) VALUES(?,?,?,'NAS','10.0.0.2','2026-10-23','2026-10-23')")
        .bind(&device_id).bind(owner.id).bind(node.to_string()).execute(&fixture.db.inner()).await.unwrap();
    sqlx::query("INSERT INTO asset_services(id,owner_user_id,machine_id,device_id,name,port,access_url,created_at,updated_at) VALUES(?,?,?,?,'Files',443,'https://nas.example.test/','2026-10-23','2026-10-23')")
        .bind(&service_id).bind(owner.id).bind(node.to_string()).bind(&device_id).execute(&fixture.db.inner()).await.unwrap();
    let catalog = format!("/api/v1/assets/services?owner_user_id={}", owner.id);
    let navigation = format!("/api/v1/assets/services/{service_id}/navigation?owner_user_id={}&machine_id={node}", owner.id);
    for (actor, webhook, expected) in [
        (None, false, StatusCode::UNAUTHORIZED),
        (Some(other.clone()), false, StatusCode::FORBIDDEN),
        (Some(owner.clone()), true, StatusCode::FORBIDDEN),
    ] {
        let (status, _) = fixture.request(actor.clone(), webhook, Method::GET, &catalog, None).await;
        assert_eq!(status, expected);
        let (status, _) = fixture.request(actor, webhook, Method::PATCH, &navigation, Some(json!({"show_in_navigation":true}))).await;
        assert_eq!(status, expected);
    }
    let (status, result) = fixture.request(Some(owner.clone()), false, Method::GET, &catalog, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["items"][0]["device_name"], "NAS");
    assert_eq!(result["items"][0]["show_in_navigation"], false);

    let (status, result) = fixture.request(Some(admin.clone()), false, Method::PATCH, &navigation, Some(json!({"show_in_navigation":true}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["show_in_navigation"], true);
    assert_eq!(result["owner_user_id"], owner.id);
    sqlx::query("DELETE FROM users_groups WHERE user_id = ?")
        .bind(admin.id).execute(&fixture.db.inner()).await.unwrap();
    let (status, _) = fixture.request(Some(admin), false, Method::PATCH, &navigation, Some(json!({"show_in_navigation":false}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, result) = fixture.request(Some(owner), false, Method::GET, &catalog, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["items"][0]["show_in_navigation"], true);
}

#[tokio::test]
async fn anonymous_asset_requests_return_json_authentication_error() {
    let fixture = Fixture::new().await;
    let (status, body) = fixture
        .request(None, false, Method::GET, "/api/v1/assets/context", None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "authentication_required");
    assert!(body["field_errors"].is_object());
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );
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
    let before = fixture
        .db
        .get_device((owner.id, node))
        .await
        .unwrap()
        .unwrap();

    let (status, context) = fixture
        .request(
            Some(other.clone()),
            false,
            Method::GET,
            "/api/v1/assets/context",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["can_manage_others"], false);
    let (status, _) = fixture
        .request(
            Some(other.clone()),
            false,
            Method::GET,
            "/api/v1/assets/accounts",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = fixture
        .request(
            Some(other.clone()),
            false,
            Method::GET,
            &format!("/api/v1/assets/nodes?owner_user_id={}", owner.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "asset_access_denied");
    let (status, _) = fixture.request(
        Some(other), false, Method::PATCH,
        &format!("/api/v1/assets/nodes/{node}/metadata?owner_user_id={}", owner.id),
        Some(json!({"is_public_server":true,"site_location":"unauthorized","device_model":"changed"})),
    ).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        fixture
            .db
            .get_device((owner.id, node))
            .await
            .unwrap()
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn administrator_writes_to_target_owner_and_revocation_applies_on_next_request() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("asset-admin-target").await;
    let admin = fixture.account("asset-admin-actor").await;
    fixture.make_admin(admin.id).await;
    let (status, created) = fixture
        .request(
            Some(admin.clone()),
            false,
            Method::POST,
            &format!("/api/v1/assets/device-types?owner_user_id={}", owner.id),
            Some(json!({"request_id":Uuid::new_v4(),"name":"Managed by admin"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["owner_user_id"], owner.id);
    let (status, listed) = fixture
        .request(
            Some(owner.clone()),
            false,
            Method::GET,
            &format!("/api/v1/assets/device-types?owner_user_id={}", owner.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"][0]["id"], created["id"]);
    let (status, own_types) = fixture
        .request(
            Some(admin.clone()),
            false,
            Method::GET,
            &format!("/api/v1/assets/device-types?owner_user_id={}", admin.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(own_types["items"], json!([]));

    sqlx::query("DELETE FROM users_groups WHERE user_id = ? AND group_id IN (SELECT id FROM groups WHERE name = 'admins')")
        .bind(admin.id).execute(&fixture.db.inner()).await.unwrap();
    let (status, body) = fixture
        .request(
            Some(admin),
            false,
            Method::DELETE,
            &format!(
                "/api/v1/assets/device-types/{}?owner_user_id={}",
                created["id"].as_str().unwrap(),
                owner.id
            ),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "asset_access_denied");
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM asset_device_types WHERE owner_user_id = ?")
            .bind(owner.id)
            .fetch_one(&fixture.db.inner())
            .await
            .unwrap();
    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn external_webhook_mode_reports_unavailable_and_does_not_initialize_assets() {
    let fixture = Fixture::new().await;
    let user = fixture.account("asset-webhook").await;
    let (status, context) = fixture
        .request(
            Some(user.clone()),
            true,
            Method::GET,
            "/api/v1/assets/context",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["available"], false);
    let (status, body) = fixture
        .request(
            Some(user.clone()),
            true,
            Method::POST,
            &format!(
                "/api/v1/assets/device-types/initialize?owner_user_id={}",
                user.id
            ),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "asset_mode_unavailable");
    let catalog_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_catalog_state")
        .fetch_one(&fixture.db.inner())
        .await
        .unwrap();
    assert_eq!(catalog_count, 0);
    let (status, _) = fixture
        .request(
            Some(user.clone()),
            true,
            Method::GET,
            &format!("/api/v1/assets/device-types?owner_user_id={}", user.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn deleting_node_with_children_is_rejected_before_blocking_the_node() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("asset-protected-node").await;
    let node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    let before = fixture
        .db
        .get_device((owner.id, node))
        .await
        .unwrap()
        .unwrap();
    let store = AssetsStore::new(fixture.db.clone());
    let instance = fixture.gateway_instance(owner.id, node).await;
    let preview = fixture
        .preview_gateway(owner.clone(), owner.id, node, instance)
        .await;
    let group = store
        .create_group(
            owner.id,
            owner.id,
            &node.to_string(),
            gateway_payload(instance, &preview),
        )
        .await
        .unwrap();
    let url = format!(
        "/api/v1/assets/nodes/{node}?owner_user_id={}&block=true",
        owner.id
    );
    let (status, body) = fixture
        .request(Some(owner.clone()), false, Method::DELETE, &url, None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "asset_node_conflict");
    assert_eq!(
        fixture
            .db
            .get_device((owner.id, node))
            .await
            .unwrap()
            .unwrap(),
        before
    );
    assert!(
        !fixture
            .db
            .is_device_blocked((owner.id, node))
            .await
            .unwrap()
    );
    assert_eq!(
        store
            .list_groups(owner.id, &node.to_string())
            .await
            .unwrap()["items"][0]["id"],
        group["id"]
    );

    store
        .delete_group(owner.id, &node.to_string(), group["id"].as_str().unwrap())
        .await
        .unwrap();
    let (status, _) = fixture
        .request(Some(owner.clone()), false, Method::DELETE, &url, None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        fixture
            .db
            .get_device((owner.id, node))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .db
            .is_device_blocked((owner.id, node))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn ordinary_user_cannot_read_preview_or_write_another_owners_bindings() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("binding-owner").await;
    let other = fixture.account("binding-other").await;
    let admin = fixture.account("binding-unrelated-admin").await;
    fixture.make_admin(admin.id).await;
    let node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    let instance = fixture.gateway_instance(owner.id, node).await;
    let preview = fixture
        .preview_gateway(owner.clone(), owner.id, node, instance)
        .await;
    let scope = format!("owner_user_id={}&machine_id={node}", owner.id);
    let resource_id = Uuid::new_v4();
    let requests = [
        (
            Method::GET,
            format!("/api/v1/assets/network-instances?{scope}&role=gateway"),
            None,
        ),
        (
            Method::POST,
            format!("/api/v1/assets/validation/binding?{scope}"),
            Some(
                json!({"subject":{"resource_kind":"gateway","id":null},"operation":"attach","instance_id":instance}),
            ),
        ),
        (
            Method::POST,
            format!("/api/v1/assets/validation/instance-address?{scope}"),
            Some(
                json!({"subject":{"resource_kind":"device","id":null},"address_spec":{"mode":"direct_instance","instance_id":instance}}),
            ),
        ),
        (
            Method::POST,
            format!("/api/v1/assets/gateway-groups?{scope}"),
            Some(gateway_payload(instance, &preview)),
        ),
        (
            Method::PUT,
            format!("/api/v1/assets/gateway-groups/{resource_id}/binding?{scope}"),
            Some(
                json!({"request_id":Uuid::new_v4(),"operation":"refresh","instance_id":instance,"preview_version":preview["preview_version"]}),
            ),
        ),
        (
            Method::PUT,
            format!("/api/v1/assets/devices/{resource_id}/binding?{scope}"),
            Some(json!({})),
        ),
        (
            Method::PUT,
            format!("/api/v1/assets/devices/{resource_id}/address?{scope}"),
            Some(json!({})),
        ),
    ];
    for (method, path, payload) in requests {
        let (status, body) = fixture
            .request(Some(other.clone()), false, method, &path, payload)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
        assert_eq!(body["code"], "asset_access_denied", "{path}");
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_gateway_groups")
        .fetch_one(&fixture.db.inner())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn administrator_binding_creation_uses_target_scope_and_revocation_blocks_next_request() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("binding-admin-target").await;
    let admin = fixture.account("binding-admin-actor").await;
    fixture.make_admin(admin.id).await;
    let node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    fixture.register_node(admin.id, node).await;
    let instance = fixture.gateway_instance(owner.id, node).await;
    let actor_instance = fixture.gateway_instance(admin.id, node).await;
    let scope = format!("owner_user_id={}&machine_id={node}", owner.id);
    let candidates_path = format!("/api/v1/assets/network-instances?{scope}&role=gateway");
    let (status, candidates) = fixture
        .request(
            Some(admin.clone()),
            false,
            Method::GET,
            &candidates_path,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(candidates["items"].as_array().unwrap().len(), 1);
    assert_eq!(candidates["items"][0]["instance_id"], instance.to_string());
    assert_ne!(
        candidates["items"][0]["instance_id"],
        actor_instance.to_string()
    );
    assert_eq!(candidates["items"][0]["online"], false);
    assert_eq!(candidates["items"][0]["selectable"], true);
    let preview = fixture
        .preview_gateway(admin.clone(), owner.id, node, instance)
        .await;
    let create_path = format!("/api/v1/assets/gateway-groups?{scope}");
    let (status, created) = fixture
        .request(
            Some(admin.clone()),
            false,
            Method::POST,
            &create_path,
            Some(gateway_payload(instance, &preview)),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["owner_user_id"], owner.id);
    let group_id = created["id"].as_str().unwrap();
    let saved: (i32, String, String) = sqlx::query_as(
        "SELECT owner_user_id,machine_id,network_instance_id FROM asset_gateway_groups WHERE id = ?")
        .bind(group_id).fetch_one(&fixture.db.inner()).await.unwrap();
    assert_eq!(saved, (owner.id, node.to_string(), instance.to_string()));
    let snapshot_before: String =
        sqlx::query_scalar("SELECT binding_snapshot_json FROM asset_gateway_groups WHERE id = ?")
            .bind(group_id)
            .fetch_one(&fixture.db.inner())
            .await
            .unwrap();
    let refresh = json!({"subject":{"resource_kind":"gateway","id":group_id},"operation":"refresh","instance_id":instance});
    let preview_path = format!("/api/v1/assets/validation/binding?{scope}");
    let (status, preview) = fixture
        .request(
            Some(admin.clone()),
            false,
            Method::POST,
            &preview_path,
            Some(refresh.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["can_save"], true);

    sqlx::query("DELETE FROM users_groups WHERE user_id = ? AND group_id IN (SELECT id FROM groups WHERE name = 'admins')")
        .bind(admin.id).execute(&fixture.db.inner()).await.unwrap();
    for (method, path, payload) in [
        (Method::GET, candidates_path, None),
        (Method::POST, preview_path, Some(refresh)),
        (
            Method::PUT,
            format!("/api/v1/assets/gateway-groups/{group_id}/binding?{scope}"),
            Some(
                json!({"request_id":Uuid::new_v4(),"operation":"refresh","instance_id":instance,"preview_version":preview["preview_version"]}),
            ),
        ),
    ] {
        let (status, body) = fixture
            .request(Some(admin.clone()), false, method, &path, payload)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
        assert_eq!(body["code"], "asset_access_denied");
    }
    let snapshot_after: String =
        sqlx::query_scalar("SELECT binding_snapshot_json FROM asset_gateway_groups WHERE id = ?")
            .bind(group_id)
            .fetch_one(&fixture.db.inner())
            .await
            .unwrap();
    assert_eq!(snapshot_after, snapshot_before);
    let actor_groups: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM asset_gateway_groups WHERE owner_user_id = ?")
            .bind(admin.id)
            .fetch_one(&fixture.db.inner())
            .await
            .unwrap();
    assert_eq!(actor_groups, 0);
}

#[tokio::test]
async fn binding_candidates_and_previews_do_not_cross_node_scope() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("binding-node-scope").await;
    let node = Uuid::new_v4();
    let other_node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    fixture.register_node(owner.id, other_node).await;
    let instance = fixture.gateway_instance(owner.id, other_node).await;
    let scope = format!("owner_user_id={}&machine_id={node}", owner.id);
    let (status, body) = fixture
        .request(
            Some(owner.clone()),
            false,
            Method::GET,
            &format!("/api/v1/assets/network-instances?{scope}&role=gateway"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"], json!([]));
    let (status, body) = fixture.request(Some(owner.clone()), false, Method::POST,
        &format!("/api/v1/assets/validation/binding?{scope}"),
        Some(json!({"subject":{"resource_kind":"gateway","id":null},"operation":"attach","instance_id":instance}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "binding_instance_missing");
    let preview = fixture
        .preview_gateway(owner.clone(), owner.id, other_node, instance)
        .await;
    let (status, body) = fixture
        .request(
            Some(owner),
            false,
            Method::POST,
            &format!("/api/v1/assets/gateway-groups?{scope}"),
            Some(gateway_payload(instance, &preview)),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "binding_instance_missing");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_gateway_groups")
        .fetch_one(&fixture.db.inner())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn external_webhook_mode_rejects_binding_candidates_previews_and_writes() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("binding-webhook").await;
    let node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    let instance = fixture.gateway_instance(owner.id, node).await;
    let preview = fixture
        .preview_gateway(owner.clone(), owner.id, node, instance)
        .await;
    let scope = format!("owner_user_id={}&machine_id={node}", owner.id);
    let resource_id = Uuid::new_v4();
    for (method, path, payload) in [
        (
            Method::GET,
            format!("/api/v1/assets/network-instances?{scope}&role=gateway"),
            None,
        ),
        (
            Method::POST,
            format!("/api/v1/assets/validation/binding?{scope}"),
            Some(
                json!({"subject":{"resource_kind":"gateway","id":null},"operation":"attach","instance_id":instance}),
            ),
        ),
        (
            Method::POST,
            format!("/api/v1/assets/validation/instance-address?{scope}"),
            Some(json!({})),
        ),
        (
            Method::POST,
            format!("/api/v1/assets/gateway-groups?{scope}"),
            Some(gateway_payload(instance, &preview)),
        ),
        (
            Method::PUT,
            format!("/api/v1/assets/gateway-groups/{resource_id}/binding?{scope}"),
            Some(json!({})),
        ),
        (
            Method::PUT,
            format!("/api/v1/assets/devices/{resource_id}/binding?{scope}"),
            Some(json!({})),
        ),
        (
            Method::PUT,
            format!("/api/v1/assets/devices/{resource_id}/address?{scope}"),
            Some(json!({})),
        ),
    ] {
        let (status, body) = fixture
            .request(Some(owner.clone()), true, method, &path, payload)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
        assert_eq!(body["code"], "asset_mode_unavailable");
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_gateway_groups")
        .fetch_one(&fixture.db.inner())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn public_node_ip_is_manual_scoped_and_cleared_when_role_is_disabled() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("node-ip-owner").await;
    let node = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    let path = format!("/api/v1/assets/nodes/{node}/metadata?owner_user_id={}", owner.id);
    let input = |role: bool, ip: Option<&str>| {
        let mut payload = json!({"is_public_server": role, "site_location": null, "device_model": null});
        if let Some(ip) = ip { payload["public_ip"] = json!(ip); }
        payload
    };
    let (status, first) = fixture.request(Some(owner.clone()), false, Method::PATCH, &path,
        Some(input(true, Some("8.8.8.8")))).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["public_ip"], "8.8.8.8");
    let (status, listed) = fixture.request(Some(owner.clone()), false, Method::GET,
        &format!("/api/v1/assets/nodes?owner_user_id={}", owner.id), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"][0]["public_ip"], "8.8.8.8");
    // Older metadata clients may omit the new field without erasing it.
    let (status, kept) = fixture.request(Some(owner.clone()), false, Method::PATCH, &path,
        Some(input(true, None))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(kept["public_ip"], "8.8.8.8");
    for invalid in ["127.0.0.1", "192.168.1.2", "203.0.113.10", "8.8.8.8:443", "http://8.8.8.8"] {
        let (status, failure) = fixture.request(Some(owner.clone()), false, Method::PATCH, &path,
            Some(input(true, Some(invalid)))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{invalid}: {failure}");
        assert!(failure["field_errors"]["public_ip"].is_string(), "{invalid}: {failure}");
    }
    let (status, removed) = fixture.request(Some(owner.clone()), false, Method::PATCH, &path,
        Some(input(false, None))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(removed["is_public_server"], false);
    assert!(removed["public_ip"].is_null());
}

#[tokio::test]
async fn server_agent_entry_reuses_asset_and_enforces_scope_without_remote_probe() {
    let fixture = Fixture::new().await;
    let owner = fixture.account("agent-entry-owner").await;
    let stranger = fixture.account("agent-entry-stranger").await;
    let admin = fixture.account("agent-entry-admin").await;
    fixture.make_admin(admin.id).await;
    let node = Uuid::new_v4();
    let device = Uuid::new_v4();
    let type_id = Uuid::new_v4();
    fixture.register_node(owner.id, node).await;
    sqlx::query("INSERT INTO asset_device_types(id,owner_user_id,name,semantic_kind,semantic_kinds,created_at,updated_at) VALUES(?,?,?,'other','[\"phone\",\"server\"]','2026-10-27','2026-10-27')")
        .bind(type_id.to_string()).bind(owner.id).bind("Linux server")
        .execute(&fixture.db.inner()).await.unwrap();
    sqlx::query("INSERT INTO asset_devices(id,owner_user_id,machine_id,device_type_id,name,overlay_ip,lan_ip,created_at,updated_at) VALUES(?,?,?,?,?,'10.80.0.8','192.168.0.8','2026-10-27','2026-10-27')")
        .bind(device.to_string()).bind(owner.id).bind(node.to_string())
        .bind(type_id.to_string()).bind("NAS")
        .execute(&fixture.db.inner()).await.unwrap();
    let url = format!(
        "/api/v1/assets/devices/{device}/agent-entry?owner_user_id={}&machine_id={node}",
        owner.id
    );
    let (status, value) = fixture.request(Some(owner.clone()), false, Method::GET, &url, None).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["owner_user_id"], owner.id);
    assert_eq!(value["machine_id"], node.to_string());
    assert_eq!(value["asset_device_id"], device.to_string());
    assert_eq!(value["overlay_ip"], "10.80.0.8");
    assert_eq!(value["agent_status"], "not_checked");
    assert_eq!(value["pairing_available"], false);
    assert_eq!(value["source"], "existing_asset_device");

    let (status, _) = fixture.request(Some(stranger), false, Method::GET, &url, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = fixture.request(Some(admin), false, Method::GET, &url, None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = fixture.request(None, false, Method::GET, &url, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = fixture.request(Some(owner.clone()), true, Method::GET, &url, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    sqlx::query("UPDATE asset_device_types SET semantic_kinds='[\"phone\"]' WHERE id=?")
        .bind(type_id.to_string()).execute(&fixture.db.inner()).await.unwrap();
    let (status, value) = fixture.request(Some(owner.clone()), false, Method::GET, &url, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{value}");
    assert_eq!(value["code"], "agent_requires_server_semantic");
    let unknown_url = url.replace(&device.to_string(), &Uuid::new_v4().to_string());
    let (status, _) = fixture.request(Some(owner), false, Method::GET, &unknown_url, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
