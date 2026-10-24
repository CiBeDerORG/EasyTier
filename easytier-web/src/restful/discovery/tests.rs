use super::*;
use crate::db::discovery::tests::Fixture;
use uuid::Uuid;
fn ep(ip: &str, port: i64) -> Value {
    json!({"address":ip,"port":port,"service":"_adb-tls-connect._tcp","protocol":"IPv4"})
}

impl Fixture {
    async fn http(
        &self,
        actor: &str,
        method: axum::http::Method,
        suffix: &str,
        body: Option<Value>,
    ) -> (axum::http::StatusCode, Value) {
        use crate::{
            central_network::service::CentralNetworkService,
            client_manager::{ClientManager, HeartbeatPolicy},
            restful::{
                ConsoleInfoConfig,
                users::{AuthSession, Backend, User},
            },
        };
        use axum::{Extension, body::Body};
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        use tower::ServiceExt;
        let account = match crate::db::entity::users::Entity::find()
            .filter(crate::db::entity::users::Column::Username.eq(actor))
            .one(self.db.orm_db())
            .await
            .unwrap()
        {
            Some(v) => v,
            None => self.db.auto_create_user(actor).await.unwrap(),
        };
        let sessions = tower_sessions_sqlx_store::SqliteStore::new(self.db.inner());
        sessions.migrate().await.unwrap();
        let auth = axum_login::AuthManagerLayerBuilder::new(
            Backend::new(self.db.clone()),
            axum_login::tower_sessions::SessionManagerLayer::new(sessions),
        )
        .build();
        let manager = std::sync::Arc::new(ClientManager::new(
            self.db.clone(),
            None,
            HeartbeatPolicy::default(),
            std::sync::Arc::new(crate::FeatureFlags::default()),
            std::sync::Arc::new(crate::webhook::WebhookConfig::new(
                None, None, None, None, None,
            )),
        ));
        let service =
            std::sync::Arc::new(CentralNetworkService::new(self.db.clone(), manager.clone()));
        let runtime = self.runtime.clone();
        let app = crate::restful::discovery::router()
            .layer(axum::middleware::from_fn(
                move |mut request: axum::extract::Request, next: axum::middleware::Next| {
                    let account = account.clone();
                    async move {
                        request
                            .extensions_mut()
                            .get_mut::<AuthSession>()
                            .unwrap()
                            .user = Some(User {
                            db_user: account,
                            tokens: vec![],
                        });
                        next.run(request).await
                    }
                },
            ))
            .layer(Extension(ConsoleInfoConfig {
                config_server_protocol: "tcp".into(),
                config_server_port: 22020,
                webhook_auth: false,
            }))
            .layer(Extension(service))
            .layer(Extension(runtime))
            .layer(auth)
            .with_state(manager);
        let request = axum::http::Request::builder()
            .method(method)
            .uri(format!(
                "/api/v1/assets/discovery/gateways/{}/{}?owner_user_id={}",
                self.g.id, suffix, self.g.owner
            ))
            .header("Content-Type", "application/json")
            .body(
                body.map(|v| Body::from(v.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let raw = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&raw).unwrap())
    }
}

#[tokio::test]
async fn http_adb_scans_once_imports_only_missing_ips_and_replay_does_no_rpc() {
    use axum::http::{Method, StatusCode};
    let f = Fixture::new().await;
    let mock = crate::discovery::luci_client::tests::Mock::start();
    std::fs::write(mock.root.join("mode.json"),json!({"password":"  exact 密码  ",
   "easy-discovery.adb":{"schema_version":1,"has_snapshot":true,"status":"ok","source":"avahi","devices":[ep("192.168.1.20",37001),ep("192.168.1.21",37002)],"errors":[]},
   "easy-discovery.devices":{"schema_version":1,"has_snapshot":true,"status":"ok","source":"network-aggregate","devices":[{"ipv4":["192.168.1.20"],"hostname":"phone"},{"ipv4":["192.168.1.99"],"hostname":"unrelated"}],"errors":[]}
 }).to_string()).unwrap();
    let (status,_)=f.http("ods-owner",Method::PUT,"access",Some(json!({"endpoint_url":mock.url,"ca_pem":mock.ca,"secret_action":"replace","secret":"  exact 密码  ","expected_revision":0}))).await;
    assert_eq!(status, StatusCode::OK);
    let request = json!({"request_id":Uuid::new_v4()});
    let (status, _) = f
        .http("outsider", Method::POST, "adb/scan", Some(request.clone()))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, result) = f
        .http("ods-owner", Method::POST, "adb/scan", Some(request.clone()))
        .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let rows = result["result"]["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("{result}"));
    assert_eq!(rows[0]["state"], "saved", "{result}");
    assert_eq!(rows[1]["state"], "skipped");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM asset_devices WHERE is_gateway_self=0")
            .fetch_one(&f.db.inner())
            .await
            .unwrap(),
        1
    );
    let count = mock.requests().len();
    let (_, replay) = f
        .http("ods-owner", Method::POST, "adb/scan", Some(request))
        .await;
    assert_eq!(replay["operation_id"], result["operation_id"]);
    assert_eq!(mock.requests().len(), count);
    let (_, history) = f.http("ods-owner", Method::GET, "operations", None).await;
    assert_eq!(history["items"].as_array().unwrap().len(), 2);
    assert_eq!(mock.requests().len(), count);
    let scans: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|r| r["params"][2] == "scan")
        .collect();
    assert_eq!(scans.len(), 2);
    assert_eq!(scans[0]["params"][1], "easy-discovery.adb");
    assert_eq!(scans[1]["params"][1], "easy-discovery.devices");
}
