use super::*;
use crate::restful::{
    ConsoleInfoConfig,
    users::{AuthSession, Backend, User},
};
use crate::{
    central_network::service::CentralNetworkService,
    client_manager::{ClientManager, HeartbeatPolicy},
};
use axum::{Router, body::Body, http::Method};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tower::ServiceExt;

fn input(endpoint: &str, revision: i64) -> Value {
    json!({"endpoint":endpoint,"revision":revision,"token_action":"replace","token":"  exact token ! $ ` 中文 with spaces 123456789  "})
}
async fn settings() -> Arc<Settings> {
    easytier::utils::init_crypto_provider();
    let db = Db::memory_db().await;
    let credentials = Arc::new(deploy_agent::Service::new(db.clone()).await.unwrap());
    Arc::new(Settings::new(db, credentials).await.unwrap())
}

async fn http(
    settings: Arc<Settings>,
    username: Option<&str>,
    method: Method,
    path: &str,
    body: Option<Value>,
    origin: Option<&str>,
) -> (StatusCode, Value) {
    let db = settings.db.clone();
    let user = if let Some(name) = username {
        let account = match crate::db::entity::users::Entity::find()
            .filter(crate::db::entity::users::Column::Username.eq(name))
            .one(db.orm_db())
            .await
            .unwrap()
        {
            Some(account) => account,
            None => db.auto_create_user(name).await.unwrap(),
        };
        if name == "settings-admin" {
            sqlx::query("INSERT OR IGNORE INTO users_groups(user_id,group_id) SELECT ?,id FROM groups WHERE name='admins'")
                .bind(account.id).execute(&db.inner()).await.unwrap();
        }
        Some(account)
    } else {
        None
    };
    let sessions = tower_sessions_sqlx_store::SqliteStore::new(db.inner());
    sessions.migrate().await.unwrap();
    let auth = axum_login::AuthManagerLayerBuilder::new(
        Backend::new(db.clone()),
        axum_login::tower_sessions::SessionManagerLayer::new(sessions),
    )
    .build();
    let manager = Arc::new(ClientManager::new(
        db.clone(),
        None,
        HeartbeatPolicy::default(),
        Arc::new(crate::FeatureFlags::default()),
        Arc::new(crate::webhook::WebhookConfig::new(
            None, None, None, None, None,
        )),
    ));
    let service = Arc::new(CentralNetworkService::new(db, manager.clone()));
    let app = super::super::router(settings)
        .layer(axum::middleware::from_fn(
            move |mut req: axum::extract::Request, next: axum::middleware::Next| {
                let user = user.clone();
                async move {
                    req.extensions_mut().get_mut::<AuthSession>().unwrap().user =
                        user.map(|db_user| User {
                            db_user,
                            tokens: vec![],
                        });
                    next.run(req).await
                }
            },
        ))
        .layer(Extension(ConsoleInfoConfig {
            config_server_protocol: "tcp".into(),
            config_server_port: 22020,
            webhook_auth: false,
        }))
        .layer(Extension(service))
        .layer(auth)
        .with_state(manager);
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(format!("/api/v1/automation{path}"))
        .header("Content-Type", "application/json")
        .header("Host", "console.local");
    if let Some(origin) = origin {
        req = req.header("Origin", origin);
    }
    let response = app
        .oneshot(
            req.body(
                body.map(|v| Body::from(v.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    if path.starts_with("/settings") {
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    let status = response.status();
    let value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    (status, value)
}

#[tokio::test]
async fn encrypted_settings_survive_restart_preserve_token_and_reject_stale_updates() {
    let settings = settings().await;
    let request = input("http://127.0.0.1:9", 0);
    let secret = request["token"].as_str().unwrap().to_owned();
    let saved = settings.save(1, request).await.unwrap();
    assert_eq!(saved["source"], "database");
    assert_eq!(saved["revision"], 1);
    assert!(!saved.to_string().contains(&secret));
    assert!(saved.get("token").is_none());
    let cipher: String = sqlx::query_scalar("SELECT token_cipher FROM automation_runner_settings")
        .fetch_one(&settings.db.inner())
        .await
        .unwrap();
    assert!(!cipher.contains(&secret));
    let reloaded = Settings::new(settings.db.clone(), settings.credentials.clone())
        .await
        .unwrap();
    assert_eq!(reloaded.runner().unwrap().token(), secret);
    reloaded
        .save(
            1,
            json!({"endpoint":"http://127.0.0.1:9","token_action":"preserve","revision":1}),
        )
        .await
        .unwrap();
    assert_eq!(reloaded.runner().unwrap().token(), secret);
    assert_eq!(
        reloaded
            .save(1, input("http://127.0.0.1:10", 1))
            .await
            .unwrap_err()
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(reloaded.runner().unwrap().endpoint(), "http://127.0.0.1:9/");
}

#[tokio::test]
async fn only_active_administrators_can_read_save_and_probe_settings() {
    let settings = settings().await;
    for method in [Method::GET, Method::PUT, Method::POST] {
        let path = if method == Method::POST {
            "/settings/check"
        } else {
            "/settings"
        };
        let payload = if method == Method::GET {
            None
        } else {
            Some(input("http://127.0.0.1:9", 0))
        };
        assert_eq!(
            http(
                settings.clone(),
                None,
                method.clone(),
                path,
                payload.clone(),
                None
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            http(
                settings.clone(),
                Some("settings-member"),
                method,
                path,
                payload,
                None
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    let rejected = http(
        settings.clone(),
        Some("settings-admin"),
        Method::PUT,
        "/settings",
        Some(input("http://127.0.0.1:9", 0)),
        Some("https://other.example"),
    )
    .await;
    assert_eq!(rejected.0, StatusCode::FORBIDDEN);
    assert!(!settings.view()["configured"].as_bool().unwrap());
    let saved = http(
        settings.clone(),
        Some("settings-admin"),
        Method::PUT,
        "/settings",
        Some(input("http://127.0.0.1:9", 0)),
        Some("http://console.local"),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK);
    let (_, context) = http(
        settings.clone(),
        Some("settings-admin"),
        Method::GET,
        "/context",
        None,
        None,
    )
    .await;
    assert_eq!(context["runner_configured"], true);
    assert_eq!(context["can_manage_settings"], true);
    let (_, context) = http(
        settings.clone(),
        Some("settings-member"),
        Method::GET,
        "/context",
        None,
        None,
    )
    .await;
    assert_eq!(context["runner_configured"], true);
    assert_eq!(context["can_manage_settings"], false);
    sqlx::query("UPDATE users SET deleted_at='now' WHERE username='settings-admin'")
        .execute(&settings.db.inner())
        .await
        .unwrap();
    assert_eq!(
        http(
            settings,
            Some("settings-admin"),
            Method::GET,
            "/settings",
            None,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn connection_probe_is_explicit_authenticated_and_uses_saved_configuration() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let settings = settings().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let executor = uuid::Uuid::new_v4().to_string();
    let expected = executor.clone();
    let secret = input("", 0)["token"].as_str().unwrap().to_owned();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/internal/v1/health",
                axum::routing::get(move |headers: HeaderMap| {
                    seen.fetch_add(1, Ordering::SeqCst);
                    let valid = headers.get("authorization").unwrap()
                        == format!("AutomationToken {}", STANDARD.encode(secret.as_bytes()))
                            .as_str();
                    let value = json!({"protocol_version":1,"executor_id":expected});
                    async move {
                        (
                            if valid {
                                StatusCode::OK
                            } else {
                                StatusCode::UNAUTHORIZED
                            },
                            Json(value),
                        )
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let saved = http(
        settings.clone(),
        Some("settings-admin"),
        Method::PUT,
        "/settings",
        Some(input(&endpoint, 0)),
        None,
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK);
    http(
        settings.clone(),
        Some("settings-admin"),
        Method::GET,
        "/settings",
        None,
        None,
    )
    .await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "reading and saving must not probe"
    );
    let result = http(
        settings.clone(),
        Some("settings-admin"),
        Method::POST,
        "/settings/check",
        Some(json!({"revision":1})),
        None,
    )
    .await;
    assert_eq!(result.0, StatusCode::OK);
    assert_eq!(result.1["executor_id"], executor);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let stale = http(
        settings,
        Some("settings-admin"),
        Method::POST,
        "/settings/check",
        Some(json!({"revision":0})),
        None,
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn active_resources_and_inflight_dispatch_block_configuration_changes() {
    let settings = settings().await;
    settings
        .save(1, input("http://127.0.0.1:9", 0))
        .await
        .unwrap();
    let dispatch = settings.operations.read().await;
    assert_eq!(
        settings
            .save(1, input("http://127.0.0.1:10", 1))
            .await
            .unwrap_err()
            .0,
        StatusCode::CONFLICT
    );
    drop(dispatch);
    sqlx::query("INSERT INTO automation_sessions(id,actor_user_id,owner_user_id,device_id,target_key,target_fingerprint,target_json,quality,state,created_at,updated_at,expires_at) VALUES('session',1,1,'device','target','fingerprint','{}','wall','active','now','now','later')")
        .execute(&settings.db.inner()).await.unwrap();
    assert_eq!(
        settings
            .save(1, input("http://127.0.0.1:10", 1))
            .await
            .unwrap_err()
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(settings.runner().unwrap().endpoint(), "http://127.0.0.1:9/");
    sqlx::query("UPDATE automation_sessions SET state='closed'")
        .execute(&settings.db.inner())
        .await
        .unwrap();
    settings
        .save(1, input("http://127.0.0.1:10", 1))
        .await
        .unwrap();
    assert_eq!(
        settings.runner().unwrap().endpoint(),
        "http://127.0.0.1:10/"
    );
}

#[tokio::test]
async fn switching_servers_requires_confirmed_cleanup_of_old_targets() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let settings = settings().await;
    let confirm = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let allow = confirm.clone();
    let seen = calls.clone();
    let executor = uuid::Uuid::new_v4().to_string();
    let device = uuid::Uuid::new_v4().to_string();
    let expected_executor = executor.clone();
    let expected_device = device.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener,Router::new().route("/internal/v1/targets/{device}/retire",axum::routing::post(move |Json(body):Json<Value>| {
            seen.fetch_add(1,Ordering::SeqCst);
            assert_eq!(body["expected_executor_id"],expected_executor);
            assert_eq!(body["target"]["device_id"],expected_device);
            let result=json!({"event_id":body["event_id"],"executor_id":expected_executor,"device_id":expected_device,"cleanup_confirmed":allow.load(Ordering::SeqCst)});
            async move {Json(result)}
        }))).await.unwrap();
    });
    settings.save(1, input(&endpoint, 0)).await.unwrap();
    sqlx::query("INSERT INTO discovery_targets(device_id,owner_user_id,executor_id,target_fingerprint,target_json,updated_at) VALUES(?,1,?,'old',?,'now')")
        .bind(&device).bind(&executor).bind(json!({"device_id":device,"target_fingerprint":"old"}).to_string())
        .execute(&settings.db.inner()).await.unwrap();
    let blocked = settings
        .save(1, input("http://127.0.0.1:9", 1))
        .await
        .unwrap_err();
    assert_eq!(blocked.1.0["code"], "runner_cleanup_pending");
    assert_eq!(
        settings.runner().unwrap().endpoint(),
        format!("{endpoint}/")
    );
    assert_eq!(settings.view()["revision"], 1);
    confirm.store(true, Ordering::SeqCst);
    settings
        .save(1, input("http://127.0.0.1:9", 1))
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(settings.runner().unwrap().endpoint(), "http://127.0.0.1:9/");
    let retired: i64 = sqlx::query_scalar("SELECT retired FROM discovery_targets")
        .fetch_one(&settings.db.inner())
        .await
        .unwrap();
    assert_eq!(retired, 1);
    server.abort();
}

#[tokio::test]
async fn invalid_configuration_never_replaces_working_settings() {
    let settings = settings().await;
    settings
        .save(1, input("http://127.0.0.1:9", 0))
        .await
        .unwrap();
    for endpoint in [
        "file:///tmp/executor",
        "https://user:pass@example.com",
        "http://example.com?a=1",
        "http://example.com#fragment",
    ] {
        assert_eq!(
            settings.save(1, input(endpoint, 1)).await.unwrap_err().0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let mut payload = input("http://127.0.0.1:10", 1);
    payload["token"] = json!("short");
    assert_eq!(
        settings.save(1, payload).await.unwrap_err().0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(settings.view()["revision"], 1);
    assert_eq!(settings.runner().unwrap().endpoint(), "http://127.0.0.1:9/");
}

#[test]
fn callbacks_accept_lossless_credentials_and_legacy_bearer_tokens() {
    for token in [
        "legacy-token-1234567890123456789012345",
        "  中文 ! $ ` token with spaces 123456789  ",
    ] {
        let runner = Runner::configured("http://127.0.0.1:9", token.into()).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            format!("AutomationToken {}", STANDARD.encode(token.as_bytes()))
                .parse()
                .unwrap(),
        );
        assert!(runner.authenticate(&headers).is_ok());
        headers.insert(
            "authorization",
            format!("Bearer {}", "wrong-token").parse().unwrap(),
        );
        assert!(runner.authenticate(&headers).is_err());
        if token.is_ascii() {
            headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
            assert!(runner.authenticate(&headers).is_ok());
        }
    }
}

#[tokio::test]
async fn executor_only_database_keeps_key_across_restart_and_rejects_missing_key() {
    easytier::utils::init_crypto_provider();
    let directory = std::env::temp_dir().join(format!("runner-settings-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let db = Db::new(directory.join("settings.db").to_string_lossy())
        .await
        .unwrap();
    let credentials = Arc::new(deploy_agent::Service::new(db.clone()).await.unwrap());
    let settings = Settings::new(db.clone(), credentials).await.unwrap();
    settings
        .save(1, input("http://127.0.0.1:9", 0))
        .await
        .unwrap();
    let credentials = Arc::new(deploy_agent::Service::new(db.clone()).await.unwrap());
    let restarted = Settings::new(db.clone(), credentials).await.unwrap();
    assert_eq!(
        restarted.runner().unwrap().token(),
        settings.runner().unwrap().token()
    );
    let key = db.deploy_agent_key_path().unwrap();
    std::fs::remove_file(&key).unwrap();
    assert!(deploy_agent::Service::new(db.clone()).await.is_err());
    assert!(!key.exists());
    drop(restarted);
    drop(settings);
    db.inner().close().await;
    std::fs::remove_dir_all(directory).unwrap();
}
