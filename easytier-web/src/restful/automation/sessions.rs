use std::{sync::Arc, time::Duration};

use axum::{Extension, Json, extract::{Path, State, WebSocketUpgrade, ws::{Message, WebSocket}}, http::{HeaderMap, StatusCode}, response::Response};
use futures::SinkExt;
use serde_json::{Value, json};
use tokio::time::{Instant, MissedTickBehavior};

use crate::{central_network::service::CentralNetworkService, db::automation::AutomationStore};
use super::{AutomationRuntime, common::*, super::{AppStateInner, users::AuthSession}};

fn creator(session: &Value, actor: i32) -> HttpResult<()> {
    if session["actor_user_id"].as_i64() != Some(i64::from(actor)) {
        return Err(error(StatusCode::FORBIDDEN,"session_actor_mismatch","请使用自己创建的设备会话。"));
    }
    Ok(())
}

fn confirmed_closed(result: &Value, session_id: &str) -> bool {
    result["id"] == session_id && result["state"] == "closed"
        && result["cleanup_pending"].as_bool() == Some(false)
}

fn invalid_response() -> HttpError {
    error(StatusCode::BAD_GATEWAY,"runner_response_invalid","执行器返回的会话状态无效，当前资源清理仍需确认。")
}

async fn observed_session(manager:&AppStateInner, service:&CentralNetworkService, owner:i32, session_id:&str) -> HttpResult<(AutomationStore,Value)> {
    let session=store(service).session(owner,session_id).await.map_err(store_error)?;
    let actor=session["actor_user_id"].as_i64().and_then(|value|i32::try_from(value).ok())
        .ok_or_else(||error(StatusCode::FORBIDDEN,"session_revoked","会话账号资料已失效。"))?;
    store(service).authorize(actor,owner).await.map_err(store_error)?;
    let device_id=id(session["device_id"].as_str().unwrap_or_default())?;
    let store=observed_store(manager,service,owner,Some(&[device_id])).await?;
    let session=store.validate_session(owner,session_id).await.map_err(store_error)?;
    Ok((store,session))
}

pub(super) async fn create(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth,&config,&service,owner).await?;
    let runner = runner(&runtime)?;
    let payload = body(input)?;
    exact_fields(&payload,&["device_id","quality"])?;
    let device_id = id(payload["device_id"].as_str().unwrap_or_default())?;
    let quality = payload["quality"].as_str().unwrap_or("wall");
    if !matches!(quality,"wall"|"focus") {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_quality","请选择有效的画质档位。"));
    }
    let store = observed_store(&manager,&service,owner,Some(&[device_id.clone()])).await?;
    let mut session = store.create_session(actor,owner,&device_id,quality).await.map_err(store_error)?;
    let session_id = id(session["id"].as_str().unwrap_or_default())?;
    let outbound = json!({"id":session_id,"target":wire_target(&session["target"]),"quality":quality,"lease_seconds":60});
    let result = runner.post("sessions",&outbound).await.and_then(|result| {
        if result["id"] != session_id || result["device_id"] != device_id
            || !matches!(result["state"].as_str(),Some("created"|"opening"|"active"))
            || !result["ice_servers"].is_array()
        {
            return Err(invalid_response());
        }
        Ok(result)
    });
    match result {
        Ok(result) => {
            if let Err(cause) = store.update_session(&session_id,"active").await {
                let _ = close_remote(&service,&runtime,&session_id).await;
                return Err(store_error(cause));
            }
            session["state"] = json!("active");
            session["ice_servers"] = result.get("ice_servers").cloned().unwrap_or_else(||json!([]));
            Ok(Json(public_session(session)))
        }
        Err(cause) => {
            // A timed-out create might have reached the Runner. Close the same ID;
            // never open a replacement or claim that unknown resources are free.
            let _ = close_remote(&service,&runtime,&session_id).await;
            Err(cause)
        }
    }
}

pub(super) async fn offer(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth,&config,&service,owner).await?;
    let session_id = id(&raw_id)?;
    let payload = body(input)?;
    exact_fields(&payload,&["sdp","type"])?;
    if payload["type"] != "offer" || !payload["sdp"].as_str().is_some_and(|s| (100..=65536).contains(&s.len()) && s.contains("m=video ")) {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_offer","视频协商请求无效。"));
    }
    let (_,session) = observed_session(&manager,&service,owner,&session_id).await?;
    creator(&session,actor)?;
    let result = match runner(&runtime)?.post(&format!("sessions/{session_id}/offer"),&payload).await {
        Ok(result) if result["type"]=="answer"
            && result["sdp"].as_str().is_some_and(|value| !value.is_empty() && value.len()<=65536 && value.contains("m=video "))
            && result["width"].as_u64().is_some_and(|value|(1..=16384).contains(&value))
            && result["height"].as_u64().is_some_and(|value|(1..=16384).contains(&value)) => result,
        Ok(_) => {
            let _ = close_remote(&service,&runtime,&session_id).await;
            return Err(invalid_response());
        }
        Err(cause) => {
            // A failed first negotiation must not leave an invisible session
            // holding its device. Closing is idempotent and fences late offers.
            let _ = close_remote(&service,&runtime,&session_id).await;
            return Err(cause);
        }
    };
    // An account or target may have changed while ICE/ADB initialization waited.
    if let Err(cause) = observed_session(&manager,&service,owner,&session_id).await {
        let _ = close_remote(&service,&runtime,&session_id).await;
        return Err(cause);
    }
    Ok(Json(result))
}

pub(super) async fn close(auth: AuthSession, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let session_id = id(&raw_id)?;
    // Owners/administrators may stop an existing session, but may not inject
    // input into somebody else's viewing session.
    store(&service).session(owner,&session_id).await.map_err(store_error)?;
    Ok(Json(close_remote(&service,&runtime,&session_id).await?))
}

pub(super) async fn close_remote(service: &CentralNetworkService, runtime: &AutomationRuntime, session_id: &str) -> HttpResult<Value> {
    let store = store(service);
    store.update_session(session_id,"cleanup_pending").await.map_err(store_error)?;
    let remote = match runner(runtime) {
        Ok(runner) => runner.post(&format!("sessions/{session_id}/close"),&json!({})).await,
        Err(cause) => Err(cause),
    };
    match remote {
        Ok(result) if confirmed_closed(&result,session_id) => {
            store.update_session(session_id,"closed").await.map_err(store_error)?;
            Ok(json!({"id":session_id,"state":"closed","cleanup_pending":false}))
        }
        _ => Ok(json!({"id":session_id,"state":"cleanup_pending","cleanup_pending":true,
            "message":"关闭请求已记录，执行器清理尚未确认。"})),
    }
}

pub(super) async fn control(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap, ws: WebSocketUpgrade) -> HttpResult<Response> {
    require_origin(&headers,true)?;
    let owner = owner(query)?;
    let actor = authorize(&auth,&config,&service,owner).await?;
    runner(&runtime)?;
    let session_id = id(&raw_id)?;
    let (observed,session) = observed_session(&manager,&service,owner,&session_id).await?;
    creator(&session,actor)?;
    Ok(ws.max_message_size(16 * 1024).max_frame_size(16 * 1024)
        .on_upgrade(move |socket| control_loop(socket,manager,service,runtime,owner,session_id,session,observed)))
}

async fn send(socket: &mut WebSocket, value: &Value) -> bool {
    matches!(tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Text(value.to_string().into()))).await,Ok(Ok(())))
}

fn command(value: &Value, previous_seq: u64) -> HttpResult<u64> {
    let kind = value["type"].as_str().unwrap_or_default();
    let fields: &[&str] = match kind {
        "touch" => &["type","command_id","seq","action","x","y","width","height"],
        "touch.release" | "stop" => &["type","command_id","seq"],
        "key" => &["type","command_id","seq","key"],
        "input_text" => &["type","command_id","seq","text"],
        _ => return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_command","不支持该控制操作。")),
    };
    exact_fields(value,fields)?;
    id(value["command_id"].as_str().unwrap_or_default())?;
    let seq = value["seq"].as_u64().filter(|seq| *seq == previous_seq.saturating_add(1) && *seq <= 9_007_199_254_740_991)
        .ok_or_else(||error(StatusCode::CONFLICT,"command_sequence_invalid","控制序号已过期，请重新连接。"))?;
    match kind {
        "touch" => {
            let width = value["width"].as_u64().filter(|n| (1..=16384).contains(n));
            let height = value["height"].as_u64().filter(|n| (1..=16384).contains(n));
            let x = value["x"].as_f64();
            let y = value["y"].as_f64();
            if !matches!(value["action"].as_str(),Some("down"|"move"|"up")) || width.is_none() || height.is_none()
                || !x.is_some_and(|x| x.is_finite() && x >= 0.0 && x < width.unwrap_or(0) as f64)
                || !y.is_some_and(|y| y.is_finite() && y >= 0.0 && y < height.unwrap_or(0) as f64)
            {
                return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_touch","触控坐标或画面尺寸无效。"));
            }
        }
        "key" if !matches!(value["key"].as_str(),Some("HOME"|"BACK"|"RECENT"|"WAKE")) =>
            return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_key","不支持该导航键。")),
        "input_text" if !value["text"].as_str().is_some_and(|text| !text.is_empty() && text.len() <= 2000 && !text.contains('\0')) =>
            return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_text","请输入不超过2000字节的有效文字。")),
        _ => {},
    }
    Ok(seq)
}

async fn control_loop(mut socket: WebSocket, manager:AppStateInner, service: Arc<CentralNetworkService>, runtime: Arc<AutomationRuntime>, owner: i32, session_id: String, session: Value, mut observed:AutomationStore) {
    {
        let mut controls = runtime.controls.lock().await;
        if !controls.insert(session_id.clone()) {
            drop(controls);
            let _ = send(&mut socket,&json!({"type":"error","code":"session_in_use","message":"此会话已有控制连接。"})).await;
            let _ = socket.close().await;
            return;
        }
    }
    let ready = json!({"type":"ready","session_id":session_id,"device_id":session["device_id"]});
    let mut keep_running = send(&mut socket,&ready).await;
    let mut seq = 0;
    let mut last_browser = Instant::now();
    let mut lease_tick = tokio::time::interval_at(Instant::now()+Duration::from_secs(20),Duration::from_secs(20));
    lease_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    while keep_running {
        tokio::select! {
            _ = lease_tick.tick() => {
                // Only an already-open session owns this timer. Ping/Pong bounds
                // orphan viewing; it never discovers or reconnects devices.
                if last_browser.elapsed() > Duration::from_secs(40) { break; }
                let current = match observed_session(&manager,&service,owner,&session_id).await {
                    Ok((fresh,value)) => { observed=fresh; value },
                    Err((_,Json(mut value))) => { value["type"]=json!("error"); let _=send(&mut socket,&value).await; break; }
                };
                let Some(runner) = runtime.runner.as_ref() else { break; };
                let renew = json!({"target_fingerprint":current["target"]["target_fingerprint"],"lease_seconds":60});
                let renewal = runner.post(&format!("sessions/{session_id}/renew"),&renew).await;
                if !renewal.as_ref().is_ok_and(|result|result["id"]==session_id
                    && matches!(result["state"].as_str(),Some("created"|"starting"|"streaming"|"opening"|"active"))
                    && result["expires_at"].is_string())
                    || store(&service).renew_session(&session_id).await.is_err() { break; }
                if !matches!(tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Ping(Vec::new().into()))).await,Ok(Ok(()))) { break; }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(raw))) => {
                        last_browser = Instant::now();
                        let value: Value = match serde_json::from_str(&raw) {
                            Ok(value) => value,
                            Err(_) => { let _=send(&mut socket,&json!({"type":"error","code":"invalid_json","message":"控制消息格式无效。"})).await; break; }
                        };
                        let next = match command(&value,seq) {
                            Ok(next) => next,
                            Err((_,Json(mut failure))) => { failure["type"]=json!("error"); failure["command_id"]=value["command_id"].clone(); let _=send(&mut socket,&failure).await; break; }
                        };
                        // Consume a valid sequence even when the device later
                        // rejects a command. Never replay uncertain touch input.
                        seq = next;
                        // Reuse this session's most recent bounded observation;
                        // every input still checks current DB permissions/routes.
                        if let Err(cause) = observed.validate_session(owner,&session_id).await {
                            let (_,Json(mut failure))=store_error(cause); failure["type"]=json!("error"); failure["command_id"]=value["command_id"].clone(); let _=send(&mut socket,&failure).await; break;
                        }
                        let Some(runner) = runtime.runner.as_ref() else { break; };
                        match runner.post(&format!("sessions/{session_id}/commands"),&value).await {
                            Ok(result) if result["type"]=="ack" && result["command_id"]==value["command_id"] && result["state"]=="action_sent" => {
                                keep_running=send(&mut socket,&result).await;
                            }
                            Ok(_) => {
                                let _=send(&mut socket,&json!({"type":"error","command_id":value["command_id"],"code":"outcome_unknown","message":"控制响应未确认，已停止本次连接，不会自动重发。"})).await;
                                break;
                            }
                            Err((status,Json(mut failure))) => {
                                failure["type"]=json!("error"); failure["command_id"]=value["command_id"].clone();
                                keep_running=send(&mut socket,&failure).await && !status.is_server_error()
                                    && status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN;
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Ping(_))) => { last_browser=Instant::now(); }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Binary(_))) => break,
                }
            }
        }
    }
    let _ = socket.close().await;
    let _ = close_remote(&service,&runtime,&session_id).await;
    runtime.controls.lock().await.remove(&session_id);
}
