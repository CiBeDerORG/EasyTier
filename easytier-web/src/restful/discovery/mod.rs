use super::{AppStateInner, assets::publication_apply, automation::common::*, users::AuthSession};
use crate::{
    db::{
        assets::RuntimeObservations,
        discovery::{Gateway, Store, error_value},
    },
    discovery::{Runtime as DiscoveryRuntime, reconnect},
};
use axum::{
    Extension, Json, Router,
    body::Body as AxumBody,
    extract::{Path, State},
    http::{HeaderMap, Method},
    middleware::Next,
    response::{Response, Sse, sse::Event},
    routing::{get, post},
};
use serde_json::{Value, json};
use sqlx::Row;
use std::{convert::Infallible, sync::Arc};
type Discovery = Extension<Arc<DiscoveryRuntime>>;
const PREFIX: &str = "/api/v1/assets/discovery/gateways/{gateway}";
pub(crate) fn router() -> Router<AppStateInner> {
    Router::new()
        .route(&format!("{PREFIX}/access"), get(access).put(save_access))
        .route(&format!("{PREFIX}/{{kind}}/scan"), post(scan))
        .route(&format!("{PREFIX}/access/check"), post(check))
        .route(&format!("{PREFIX}/devices/apply"), post(apply))
        .route(&format!("{PREFIX}/operations"), get(operations))
        .route(
            &format!("{PREFIX}/operations/{{operation}}"),
            get(operation),
        )
        .route(
            &format!("{PREFIX}/operations/{{operation}}/{{action}}"),
            post(action),
        )
        .route("/api/v1/assets/discovery/events", get(events))
        .route(
            "/api/v1/automation/devices/{device}/endpoint-events/{event}/retry",
            post(retry_device_event),
        )
}
async fn scoped(
    manager: &AppStateInner,
    service: &crate::central_network::service::CentralNetworkService,
    owner: i32,
    gateway: &str,
    observe: bool,
) -> HttpResult<(Store, Gateway)> {
    let store = Store::new(&service.db, RuntimeObservations::default());
    let g = store.gateway(owner, gateway).await.map_err(store_error)?;
    let store = if observe {
        Store::new(
            &service.db,
            super::assets::runtime::observe(manager, &service.db, owner, &g.node).await,
        )
    } else {
        store
    };
    let g = store.gateway(owner, gateway).await.map_err(store_error)?;
    Ok((store, g))
}
pub(super) async fn access(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
    Path(gateway): Path<String>,
    headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    authorize(&auth, &config, &service, owner).await?;
    let (store, g) = scoped(&manager, &service, owner, &id(&gateway)?, false).await?;
    Ok(Json(store.access(&g).await.map_err(store_error)?))
}
pub(super) async fn save_access(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Extension(runtime): Discovery,
    query: OwnerInput,
    Path(gateway): Path<String>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    authorize(&auth, &config, &service, owner).await?;
    let payload = body(input)?;
    let _lock = service.lock_mutations().await;
    let (store, g) = scoped(&manager, &service, owner, &id(&gateway)?, false).await?;
    Ok(Json(
        store
            .save_access(&g, &payload, &runtime)
            .await
            .map_err(store_error)?,
    ))
}
pub(super) async fn check(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Extension(runtime): Discovery,
    query: OwnerInput,
    Path(gateway): Path<String>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth, &config, &service, owner).await?;
    let payload = body(input)?;
    exact_fields(&payload, &["request_id"])?;
    let _gate = runtime.lock(format!("gateway:{owner}:{gateway}")).await;
    let (store, g) = scoped(&manager, &service, owner, &id(&gateway)?, false).await?;
    let (op, fresh) = store
        .start(actor, &g, "check", &payload, None)
        .await
        .map_err(store_error)?;
    if fresh {
        let result = match store.client(&g, &runtime).await {
            Ok((client, password)) => client.check(&password).await,
            Err(e) => Err(e),
        };
        let result = result.unwrap_or_else(error_value);
        store
            .finish(&op, None, &result)
            .await
            .map_err(store_error)?;
        sqlx::query("UPDATE discovery_gateway_access SET last_result_json=? WHERE gateway_device_id=? AND config_revision=?").bind(result.to_string()).bind(&gateway).bind(g.context["access_revision"].as_i64()).execute(&service.db.inner()).await.map_err(|e|store_error(e.into()))?;
    }
    Ok(Json(store.operation(&g, &op).await.map_err(store_error)?))
}
pub(super) async fn scan(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Extension(runtime): Discovery,
    query: OwnerInput,
    Path((gateway, kind)): Path<(String, String)>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth, &config, &service, owner).await?;
    let payload = body(input)?;
    exact_fields(&payload, &["request_id"])?;
    if !matches!(kind.as_str(), "devices" | "adb") {
        return Err(store_error(crate::db::assets::AssetError::not_found()));
    }
    let _gate = runtime.lock(format!("gateway:{owner}:{gateway}")).await;
    let (store, g) = scoped(&manager, &service, owner, &id(&gateway)?, true).await?;
    let (op, fresh) = store
        .start(actor, &g, &kind, &payload, None)
        .await
        .map_err(store_error)?;
    if !fresh {
        return Ok(Json(store.operation(&g, &op).await.map_err(store_error)?));
    }
    let result = match store.client(&g, &runtime).await {
        Ok((client, password)) => client.scan(&password, kind == "adb").await,
        Err(e) => Err(e),
    };
    let snapshot = match result {
        Ok(v) => v,
        Err(e) => {
            store
                .finish(&op, None, &error_value(e))
                .await
                .map_err(store_error)?;
            return Ok(Json(store.operation(&g, &op).await.map_err(store_error)?));
        }
    };
    // No RPC runs inside a SQL transaction. Recheck actor and binding after the scan.
    authorize(&auth, &config, &service, owner).await?;
    let _mutation = service.lock_mutations().await;
    if kind == "devices" {
        if let Err(e) = store.preview(&g, &op, &snapshot, None).await {
            store
                .finish(&op, Some(&snapshot), &error_value(e))
                .await
                .map_err(store_error)?;
        }
    } else {
        store
            .finish(&op, Some(&snapshot), &json!({"state":"snapshot_saved"}))
            .await
            .map_err(store_error)?;
        let plan = publication_apply::snapshot(&service, owner, &g.node, None).await?;
        let missing = store
            .missing_adb_ips(&g, &snapshot)
            .await
            .map_err(store_error)?;
        if !missing.is_empty() {
            // Persist use BEFORE network I/O; an interrupted supplemental scan is never repeated.
            sqlx::query("UPDATE discovery_operations SET supplement_used=1 WHERE id=?")
                .bind(&op)
                .execute(&service.db.inner())
                .await
                .map_err(|e| store_error(e.into()))?;
            let (supplement, _) = store
                .start(
                    actor,
                    &g,
                    "devices",
                    &json!({"request_id":uuid::Uuid::new_v4().to_string()}),
                    Some(&op),
                )
                .await
                .map_err(store_error)?;
            let result = match store.client(&g, &runtime).await {
                Ok((client, password)) => client.scan(&password, false).await,
                Err(e) => Err(e),
            };
            match result {
                Ok(v) => match store.preview(&g, &supplement, &v, Some(&missing)).await {
                    Ok(preview) => {
                        let rows: Vec<Value> = preview["rows"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|r| {
                                r["suggested_action"] == "create" && r["blocked_reason"].is_null()
                            })
                            .map(|r| json!({"row_id":r["row_id"],"action":"create"}))
                            .collect();
                        if !rows.is_empty() {
                            let _=store.apply_rows(actor,&g,&json!({"request_id":uuid::Uuid::new_v4().to_string(),"preview_id":supplement,"preview_revision":preview["revision"],"rows":rows})).await;
                        }
                    }
                    Err(e) => {
                        store
                            .finish(&supplement, Some(&v), &error_value(e))
                            .await
                            .map_err(store_error)?;
                    }
                },
                Err(e) => {
                    store
                        .finish(&supplement, None, &error_value(e))
                        .await
                        .map_err(store_error)?;
                }
            }
            store
                .finish(&op, None, &json!({"supplement_preview_id":supplement}))
                .await
                .map_err(store_error)?;
        }
        if let Err(e) = store.reconcile_adb(&g, &op, &[], false).await {
            store
                .finish(&op, None, &error_value(e))
                .await
                .map_err(store_error)?;
        }
        publication_apply::apply(&manager, &service, owner, plan, None).await;
    }
    Ok(Json(store.operation(&g, &op).await.map_err(store_error)?))
}
pub(super) async fn apply(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Extension(runtime): Discovery,
    query: OwnerInput,
    Path(gateway): Path<String>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth, &config, &service, owner).await?;
    let payload = body(input)?;
    exact_fields(
        &payload,
        &["request_id", "preview_id", "preview_revision", "rows"],
    )?;
    let _gate = runtime.lock(format!("gateway:{owner}:{gateway}")).await;
    let _mutation = service.lock_mutations().await;
    let (store, g) = scoped(&manager, &service, owner, &id(&gateway)?, true).await?;
    let plan = publication_apply::snapshot(&service, owner, &g.node, None).await?;
    let mut result = store
        .apply_rows(actor, &g, &payload)
        .await
        .map_err(store_error)?;
    if let Some(original) = result["originating_operation_id"].as_str() {
        result["continuation"] = match store.reconcile_adb(&g, original, &[], true).await {
            Ok(v) => v,
            Err(e) => error_value(e),
        };
    }
    result["publication"] = publication_apply::apply(&manager, &service, owner, plan, None).await;
    Ok(Json(result))
}
pub(super) async fn operation(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
    Path((gateway, op)): Path<(String, String)>,
    headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    authorize(&auth, &config, &service, owner).await?;
    let (store, g) = scoped(&manager, &service, owner, &id(&gateway)?, false).await?;
    Ok(Json(
        store.operation(&g, &id(&op)?).await.map_err(store_error)?,
    ))
}
pub(super) async fn action(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Extension(runtime): Discovery,
    query: OwnerInput,
    Path((gateway, op, action)): Path<(String, String, String)>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth, &config, &service, owner).await?;
    let payload = body(input)?;
    let _gate = runtime.lock(format!("gateway:{owner}:{gateway}")).await;
    let (store, g) = scoped(&manager, &service, owner, &id(&gateway)?, true).await?;
    let op = id(&op)?;
    let scope = format!("{action}:{op}");
    if let Some(v) = store
        .replay(actor, owner, &scope, &payload)
        .await
        .map_err(store_error)?
    {
        return Ok(Json(v));
    }
    let original = store.operation(&g, &op).await.map_err(store_error)?;
    if original["revision"] != payload["expected_operation_revision"] {
        return Err(store_error(crate::db::discovery::conflict(
            "Operation revision changed. Reload its results.",
        )));
    }
    let result = match action.as_str() {
        "resolve" => {
            exact_fields(
                &payload,
                &["request_id", "expected_operation_revision", "choices"],
            )?;
            let choices = payload["choices"].as_array().ok_or_else(|| {
                store_error(crate::db::assets::AssetError::field(
                    "choices",
                    "Select saved endpoints.",
                ))
            })?;
            let _mutation = service.lock_mutations().await;
            let plan = publication_apply::snapshot(&service, owner, &g.node, None).await?;
            let mut result = store
                .reconcile_adb(&g, &op, choices, true)
                .await
                .map_err(store_error)?;
            result["publication"] =
                publication_apply::apply(&manager, &service, owner, plan, None).await;
            result
        }
        "retry" => {
            exact_fields(
                &payload,
                &[
                    "request_id",
                    "expected_operation_revision",
                    "event_id",
                    "stage",
                ],
            )?;
            let eid = id(payload["event_id"].as_str().unwrap_or(""))?;
            let event=sqlx::query("SELECT device_id,publication_json FROM discovery_endpoint_events e WHERE event_id=? AND operation_id=? AND owner_user_id=? AND NOT EXISTS(SELECT 1 FROM discovery_endpoint_events newer WHERE newer.owner_user_id=e.owner_user_id AND newer.device_id=e.device_id AND newer.seq>e.seq)").bind(&eid).bind(&op).bind(owner).fetch_optional(&service.db.inner()).await.map_err(|e|store_error(e.into()))?.ok_or_else(||store_error(crate::db::discovery::conflict("The endpoint event is no longer current.")))?;
            let device: String = event
                .try_get("device_id")
                .map_err(|e| store_error(e.into()))?;
            match payload["stage"].as_str() {
                Some("publication") => {
                    let _mutation = service.lock_mutations().await;
                    let p: Value = serde_json::from_str(
                        &event
                            .try_get::<String, _>("publication_json")
                            .map_err(|e| store_error(e.into()))?,
                    )
                    .unwrap_or(Value::Null);
                    let pid = p["plan_id"].as_str().ok_or_else(|| {
                        store_error(crate::db::discovery::conflict(
                            "No saved publication plan exists for this event.",
                        ))
                    })?;
                    publication_apply::retry(&manager, &service, owner, pid).await?
                }
                Some(stage @ ("cleanup" | "connection")) => reconnect::advance(
                    &manager,
                    &service,
                    &runtime,
                    owner,
                    &device,
                    Some((&eid, stage)),
                )
                .await
                .map_err(store_error)?,
                _ => {
                    return Err(store_error(crate::db::assets::AssetError::field(
                        "stage",
                        "Choose publication, cleanup or connection.",
                    )));
                }
            }
        }
        _ => return Err(store_error(crate::db::assets::AssetError::not_found())),
    };
    store
        .remember(actor, owner, &scope, &payload, &result)
        .await
        .map_err(store_error)?;
    Ok(Json(result))
}
// Server-push only. No keepalive or reconnect timer; a closed channel requires a user action.
pub(super) async fn events(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Extension(runtime): Discovery,
    query: OwnerInput,
    headers: HeaderMap,
) -> HttpResult<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth, &config, &service, owner).await?;
    let receiver = runtime.events.subscribe();
    let stream = futures::stream::unfold(
        (receiver, service, actor, owner),
        |(mut rx, service, actor, owner)| async move {
            loop {
                let value = match rx.recv().await {
                    Ok(v) => v,
                    Err(_) => return None,
                };
                if value["owner_user_id"] != owner {
                    continue;
                }
                if store(&service).authorize(actor, owner).await.is_err() {
                    return None;
                }
                let event = Event::default().event("endpoint").data(value.to_string());
                return Some((Ok(event), (rx, service, actor, owner)));
            }
        },
    );
    Ok(Sse::new(stream))
}
/// Captures events from all successful asset mutations, including manual edits.
pub(super) async fn after_mutation(
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(runtime): Discovery,
    request: axum::http::Request<AxumBody>,
    next: Next,
) -> Response {
    let eligible = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && request.uri().path().starts_with("/api/v1/");
    let cursor = if eligible {
        sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(seq),0) FROM discovery_endpoint_events")
            .fetch_one(&service.db.inner())
            .await
            .ok()
    } else {
        None
    };
    let response = next.run(request).await;
    if let Some(cursor) = cursor {
        // Transactions may commit partial row results before an HTTP error. Their fences still need cleanup.
        tokio::spawn(async move {
            let rows=sqlx::query("SELECT owner_user_id,device_id FROM discovery_endpoint_events WHERE seq>? GROUP BY owner_user_id,device_id ORDER BY MAX(predecessor_device_id IS NOT NULL),MIN(seq)").bind(cursor).fetch_all(&service.db.inner()).await.unwrap_or_default();
            use futures::StreamExt;
            futures::stream::iter(rows)
                .for_each_concurrent(1, |row| {
                    let (manager, service, runtime) =
                        (manager.clone(), service.clone(), runtime.clone());
                    async move {
                        if let (Ok(owner), Ok(device)) = (
                            row.try_get::<i32, _>("owner_user_id"),
                            row.try_get::<String, _>("device_id"),
                        ) {
                            if let Err(e) = reconnect::advance(
                                &manager, &service, &runtime, owner, &device, None,
                            )
                            .await
                            {
                                tracing::warn!(code=%e.code,"endpoint transition remains pending");
                            }
                        }
                    }
                })
                .await;
        });
    }
    response
}

async fn retry_device_event(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    Extension(runtime): Discovery,
    query: OwnerInput,
    Path((device, event)): Path<(String, String)>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth, &config, &service, owner).await?;
    let payload = body(input)?;
    exact_fields(&payload, &["request_id", "stage"])?;
    let device = id(&device)?;
    let event = id(&event)?;
    let _request = runtime.lock(format!("event:{owner}:{event}")).await;
    let store = Store::new(&service.db, RuntimeObservations::default());
    let scope = format!("event:{event}");
    if let Some(v) = store
        .replay(actor, owner, &scope, &payload)
        .await
        .map_err(store_error)?
    {
        return Ok(Json(v));
    }
    let row=sqlx::query("SELECT publication_json FROM discovery_endpoint_events e WHERE owner_user_id=? AND device_id=? AND event_id=? AND NOT EXISTS(SELECT 1 FROM discovery_endpoint_events newer WHERE newer.owner_user_id=e.owner_user_id AND newer.device_id=e.device_id AND newer.seq>e.seq)").bind(owner).bind(&device).bind(&event).fetch_optional(&service.db.inner()).await.map_err(|e|store_error(e.into()))?.ok_or_else(||store_error(crate::db::discovery::conflict("The endpoint event is no longer current.")))?;
    let result = match payload["stage"].as_str() {
        Some("publication") => {
            let _mutation = service.lock_mutations().await;
            let p: Value = serde_json::from_str(
                &row.try_get::<String, _>("publication_json")
                    .map_err(|e| store_error(e.into()))?,
            )
            .unwrap_or(Value::Null);
            let pid = p["plan_id"].as_str().ok_or_else(|| {
                store_error(crate::db::discovery::conflict(
                    "No saved publication plan exists.",
                ))
            })?;
            publication_apply::retry(&manager, &service, owner, pid).await?
        }
        Some(stage @ ("cleanup" | "connection")) => reconnect::advance(
            &manager,
            &service,
            &runtime,
            owner,
            &device,
            Some((&event, stage)),
        )
        .await
        .map_err(store_error)?,
        _ => {
            return Err(store_error(crate::db::assets::AssetError::field(
                "stage",
                "Choose publication, cleanup or connection.",
            )));
        }
    };
    store
        .remember(actor, owner, &scope, &payload, &result)
        .await
        .map_err(store_error)?;
    Ok(Json(result))
}

async fn operations(
    auth: AuthSession,
    State(manager): State<AppStateInner>,
    Extension(service): Service,
    Extension(config): Config,
    query: OwnerInput,
    Path(gateway): Path<String>,
    headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers, false)?;
    let owner = owner(query)?;
    authorize(&auth, &config, &service, owner).await?;
    let (_, g) = scoped(&manager, &service, owner, &id(&gateway)?, false).await?;
    let rows=sqlx::query("SELECT id,kind,status,created_at FROM discovery_operations WHERE owner_user_id=? AND gateway_device_id=? ORDER BY created_at DESC LIMIT 100").bind(owner).bind(&g.id).fetch_all(&service.db.inner()).await.map_err(|e|store_error(e.into()))?;
    let items=rows.iter().map(|r|Ok(json!({"id":r.try_get::<String,_>("id")?,"kind":r.try_get::<String,_>("kind")?,"status":r.try_get::<String,_>("status")?,"created_at":r.try_get::<String,_>("created_at")?}))).collect::<Result<Vec<_>,sqlx::Error>>().map_err(|e|store_error(e.into()))?;
    Ok(Json(json!({"items":items})))
}

#[cfg(test)]
mod tests;
