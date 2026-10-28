use std::{sync::Arc, time::Duration};

use axum::{Extension, Json, extract::{Path, State, WebSocketUpgrade, ws::{Message, WebSocket}}, http::{HeaderMap, StatusCode}, response::Response};
use futures::SinkExt;
use serde_json::{Value, json};
use tokio::time::{Instant, MissedTickBehavior};

use crate::{central_network::service::CentralNetworkService, db::automation::AutomationStore};
use super::{AutomationRuntime, common::*, settings::Settings, super::{AppStateInner, users::AuthSession}};

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

pub(super) async fn create(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, Extension(settings): Extension<Arc<Settings>>, query: OwnerInput, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    let actor = authorize(&auth,&config,&service,owner).await?;
    let runner = runner(&runtime)?;
    let payload = body(input)?;
    exact_fields(&payload,&["device_id","quality","media"])?;
    let media = payload.get("media").map(|value| -> HttpResult<Value> {
        exact_fields(value, &["max_size","video_bit_rate","max_fps","low_latency","screen_off"])?;
        for (name, minimum, maximum) in [("max_size",256,3840),("video_bit_rate",250_000,24_000_000),("max_fps",5,120)] {
            if !value[name].as_u64().is_some_and(|v| (name == "max_size" && v == 0) || (v >= minimum && v <= maximum)) {
                return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_media","分辨率、码率或 FPS 参数超出允许范围。"));
            }
        }
        if !value["low_latency"].is_boolean() || !value["screen_off"].is_boolean() {
            return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_media","低延时及实体屏幕关闭选项必须是布尔值。"));
        }
        Ok(value.clone())
    }).transpose()?;
    let device_id = id(payload["device_id"].as_str().unwrap_or_default())?;
    let quality = payload["quality"].as_str().unwrap_or("wall");
    if !matches!(quality,"wall"|"focus") {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_quality","请选择有效的画质档位。"));
    }
    // Resolve the precise ADB service selected by the automation target.
    // A phone may have historical/other ADB service records: a device-only
    // lookup could read the wrong (or no) credential without an error.
    let store = observed_store(&manager,&service,owner,Some(&[device_id.clone()])).await?;
    let current_target = store.target(owner,&device_id).await.map_err(store_error)?;
    let adb_service_id = id(current_target["adb_service_id"].as_str().unwrap_or_default())?;
    // Read only the matching service's ciphertext; neither the password nor
    // the ciphertext is returned to the browser or persisted in the session.
    let secret:Option<String>=sqlx::query_scalar(
        "SELECT u.secret_cipher FROM adb_service_unlock_secrets u JOIN asset_services s ON s.id=u.service_id AND s.owner_user_id=u.owner_user_id WHERE u.service_id=? AND u.owner_user_id=? AND s.device_id=? AND s.service_type='adb'"
    ).bind(&adb_service_id).bind(owner).bind(&device_id).fetch_optional(&service.db.inner()).await
        .map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","无法读取 ADB 服务凭据。"))?;
    let unlock_password=secret.map(|cipher|
        settings.open_unlock_password(owner,&adb_service_id,&cipher)
            .map_err(|_|error(StatusCode::SERVICE_UNAVAILABLE,"unlock_vault_unavailable","开屏密码的加密密钥无法读取，请恢复密钥备份。"))
    ).transpose()?;
    let executor=runner.identity().await?;
    let mut session = store.create_session(actor,owner,&device_id,quality).await.map_err(store_error)?;
    let session_id = id(session["id"].as_str().unwrap_or_default())?;
    if session["target"]["adb_service_id"].as_str() != Some(adb_service_id.as_str()) {
        let _ = store.update_session(&session_id,"failed").await;
        return Err(error(StatusCode::CONFLICT,"adb_service_changed","ADB 服务在创建会话时发生变化，请重新打开画面。"));
    }
    sqlx::query("UPDATE automation_sessions SET executor_identity=? WHERE id=?").bind(&executor).bind(&session_id).execute(&service.db.inner()).await.map_err(|e|store_error(e.into()))?;
    crate::discovery::reconnect::remember_target(&service,&session["target"],&executor).await.map_err(|e|store_error(e.into()))?;
    let outbound = json!({"id":session_id,"target":wire_target(&session["target"]),"quality":quality,"media":media,"unlock_password":unlock_password,"lease_seconds":60,"expected_executor_id":executor});
    let result = runner.post("sessions",&outbound).await.and_then(|result| {
        if result["executor_id"] != executor || result["id"] != session_id || result["device_id"] != device_id
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

pub(super) async fn unlock(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, Extension(settings): Extension<Arc<Settings>>, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    let actor=authorize(&auth,&config,&service,owner).await?;
    let session_id=id(&raw_id)?;
    let (_,session)=observed_session(&manager,&service,owner,&session_id).await?;
    creator(&session,actor)?;
    if session["state"]!="active" {
        return Err(error(StatusCode::CONFLICT,"viewer_only","当前原视频处于只读或权限移交中，禁止解锁手机。"));
    }
    // The manual switch stays ON while a script is executing, but a phone
    // unlock is still a physical action. Never bypass the script action lease.
    let run_occupied:bool=sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM automation_run_devices d JOIN automation_sessions s ON s.id=? AND s.owner_user_id=? WHERE (d.device_id=s.device_id OR d.target_key=s.target_key) AND d.cleanup_confirmed=0 AND d.state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending'))"
    ).bind(&session_id).bind(owner).fetch_one(&service.db.inner()).await
        .map_err(|_|error(StatusCode::SERVICE_UNAVAILABLE,"permission_unconfirmed","无法确认手机动作独占状态，已拒绝解锁操作。"))?;
    if run_occupied {
        return Err(error(StatusCode::CONFLICT,"device_busy","当前手机正在执行自动化脚本，不能同时手动解锁。"));
    }
    let device_id=id(session["device_id"].as_str().unwrap_or_default())?;
    let service_id=id(session["target"]["adb_service_id"].as_str().unwrap_or_default())?;
    // A session belongs to the executor which created it; never send a secret
    // to a replacement executor when the original identity is unavailable.
    let expected:Option<String>=sqlx::query_scalar("SELECT executor_identity FROM automation_sessions WHERE id=? AND owner_user_id=?")
        .bind(&session_id).bind(owner).fetch_one(&service.db.inner()).await
        .map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","无法核验投屏会话的执行器身份。"))?;
    let expected=expected.ok_or_else(||error(StatusCode::CONFLICT,"executor_identity_missing","会话缺少执行器身份，必须重新打开画面。"))?;
    let runner=runner(&runtime)?;
    if runner.identity().await? != expected {
        return Err(error(StatusCode::CONFLICT,"executor_identity_changed","执行器已变更，请先清理原投屏会话。"));
    }
    // Resolve the exact bound ADB service: do not accept a device ID or
    // credential from the browser. Never return/decode a password in JSON.
    let cipher:Option<String>=sqlx::query_scalar(
        "SELECT u.secret_cipher FROM adb_service_unlock_secrets u JOIN asset_services s ON s.id=u.service_id AND s.owner_user_id=u.owner_user_id WHERE u.service_id=? AND u.owner_user_id=? AND s.device_id=? AND s.service_type='adb'"
    ).bind(&service_id).bind(owner).bind(&device_id).fetch_optional(&service.db.inner()).await
        .map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","无法读取 ADB 服务凭据。"))?;
    let password=cipher.map(|value|settings.open_unlock_password(owner,&service_id,&value)
        .map_err(|_|error(StatusCode::SERVICE_UNAVAILABLE,"unlock_vault_unavailable","开屏密码的加密密钥不可用。"))).transpose()?;
    let result=runner.post(&format!("sessions/{session_id}/unlock"),
        &json!({"expected_executor_id":expected,"unlock_password":password})).await?;
    let state=result["unlock_state"].as_str().unwrap_or_default();
    if result["id"]!=session_id || result["executor_id"]!=expected || !matches!(state,
        "wake_sent"|"already_unlocked"|"lock_state_unknown"|"lock_state_changed"|
        "unlock_size_unknown"|"unlock_swipe_failed"|"unlock_skipped"|"unlock_sent"|"unlock_confirmed") {
        return Err(invalid_response());
    }
    // Do not report a result for a device which was re-bound or revoked mid-flight.
    observed_session(&manager,&service,owner,&session_id).await?;
    Ok(Json(json!({"unlock_state":state})))
}

/// T38: Keep the original WebRTC viewer but revoke all manual device actions.
/// On any uncertain remote result the persisted state remains suspending
/// (fail-closed); this route never silently reacquires a write permission.
pub(super) async fn view_only(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    let actor=authorize(&auth,&config,&service,owner).await?;
    let session_id=id(&raw_id)?;
    let (observed,session)=observed_session(&manager,&service,owner,&session_id).await?;
    creator(&session,actor)?;
    let expected:Option<String>=sqlx::query_scalar("SELECT executor_identity FROM automation_sessions WHERE id=? AND owner_user_id=?")
        .bind(&session_id).bind(owner).fetch_one(&service.db.inner()).await.map_err(|e|store_error(e.into()))?;
    let expected=expected.ok_or_else(||error(StatusCode::CONFLICT,"executor_identity_missing","投屏执行器身份丢失，禁止权限移交。"))?;
    let runner=runner(&runtime)?;
    if runner.identity().await?!=expected {
        return Err(error(StatusCode::CONFLICT,"executor_identity_changed","执行器身份已变，无法安全移交原视频。"));
    }
    observed.begin_session_permission(owner,&session_id,"view_only").await.map_err(store_error)?;
    let result=runner.post(&format!("sessions/{session_id}/view-only"),&json!({"expected_executor_id":expected})).await?;
    if result["id"]!=session_id || result["executor_id"]!=expected || result["state"]!="view_only"
        || result["device_id"]!=session["device_id"] {
        return Err(invalid_response());
    }
    observed.confirm_session_permission(owner,&session_id,"view_only").await.map_err(store_error)?;
    Ok(Json(json!({"id":session_id,"state":"view_only","viewing":true,"manual_allowed":false})))
}

/// T38: restore manual eligibility only after script cleanup is confirmed.
/// The store's BEGIN IMMEDIATE fence blocks all competing run admissions.
pub(super) async fn resume_manual(auth: AuthSession, State(manager): State<AppStateInner>, Extension(service): Service, Extension(config): Config, Extension(runtime): Runtime, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    let actor=authorize(&auth,&config,&service,owner).await?;
    let session_id=id(&raw_id)?;
    let (observed,session)=observed_session(&manager,&service,owner,&session_id).await?;
    creator(&session,actor)?;
    let expected:Option<String>=sqlx::query_scalar("SELECT executor_identity FROM automation_sessions WHERE id=? AND owner_user_id=?")
        .bind(&session_id).bind(owner).fetch_one(&service.db.inner()).await.map_err(|e|store_error(e.into()))?;
    let expected=expected.ok_or_else(||error(StatusCode::CONFLICT,"executor_identity_missing","投屏执行器身份丢失，不能恢复手控。"))?;
    let runner=runner(&runtime)?;
    if runner.identity().await?!=expected {
        return Err(error(StatusCode::CONFLICT,"executor_identity_changed","执行器身份已变，不能恢复手控。"));
    }
    observed.begin_session_permission(owner,&session_id,"active").await.map_err(store_error)?;
    let result=runner.post(&format!("sessions/{session_id}/resume-manual"),&json!({"expected_executor_id":expected})).await?;
    if result["id"]!=session_id || result["executor_id"]!=expected || result["state"]!="active"
        || result["device_id"]!=session["device_id"] {
        return Err(invalid_response());
    }
    observed.confirm_session_permission(owner,&session_id,"active").await.map_err(store_error)?;
    Ok(Json(json!({"id":session_id,"state":"active","viewing":true,"manual_allowed":true})))
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

pub(crate) async fn close_remote(service: &CentralNetworkService, runtime: &AutomationRuntime, session_id: &str) -> HttpResult<Value> {
    let store = store(service);
    store.update_session(session_id,"cleanup_pending").await.map_err(store_error)?;
    let expected:Option<String>=sqlx::query_scalar("SELECT executor_identity FROM automation_sessions WHERE id=?").bind(session_id).fetch_one(&service.db.inner()).await.map_err(|e|store_error(e.into()))?;
    let Some(expected)=expected else{return Ok(json!({"id":session_id,"state":"cleanup_pending","cleanup_pending":true,"code":"legacy_executor_unconfirmed","message":"旧会话没有执行器身份记录，请先完成旧执行器清理验收。"}));};
    // Never reinterpret a new executor's empty session table as proof that a
    // previous executor has released its process or ADB forwards.
    let remote = match runner(runtime) {
        Ok(runner) => match runner.identity().await {
            Ok(current) if current != expected => {
                return Ok(json!({
                    "id":session_id,"state":"cleanup_pending","cleanup_pending":true,
                    "code":"executor_identity_changed",
                    "message":"原投屏会话绑定的执行器身份与当前执行器不一致。请恢复原执行器数据卷并完成清理，不能直接释放这条旧会话。"
                }));
            },
            Ok(_) => runner.post(&format!("sessions/{session_id}/close"),&json!({"expected_executor_id":expected})).await,
            Err(cause) => Err(cause),
        },
        Err(cause) => Err(cause),
    };
    match remote {
        Ok(result) if confirmed_closed(&result,session_id) && result["executor_id"]==expected => {
            store.update_session(session_id,"closed").await.map_err(store_error)?;
            Ok(json!({"id":session_id,"state":"closed","cleanup_pending":false}))
        }
        Ok(result) => {
            let code=result["code"].as_str().filter(|s|s.len()<=80).unwrap_or("executor_cleanup_unconfirmed");
            let message=result["message"].as_str().filter(|s|s.len()<=1024)
                .unwrap_or("执行器尚未确认投屏进程及资源清理完成，请检查执行器日志后重试。");
            Ok(json!({"id":session_id,"state":"cleanup_pending","cleanup_pending":true,
                "code":code,"message":message}))
        }
        Err((_,Json(detail))) => {
            let code=detail["code"].as_str().filter(|s|s.len()<=80).unwrap_or("executor_cleanup_unreachable");
            let message=detail["message"].as_str().filter(|s|s.len()<=1024)
                .unwrap_or("未能取得执行器清理确认，请检查执行器服务及数据卷。");
            Ok(json!({"id":session_id,"state":"cleanup_pending","cleanup_pending":true,
                "code":code,"message":message}))
        }
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
                        match observed.validate_session(owner,&session_id).await {
                            Ok(current) if current["state"]=="active" => {},
                            Ok(_) => {
                                // Keep the current WebRTC and websocket alive,
                                // but never relay a viewer's manual action.
                                keep_running=send(&mut socket,&json!({"type":"error","command_id":value["command_id"],"code":"viewer_only","message":"手机当前仅允许实时观看，不能发送手控指令。"})).await;
                                continue;
                            },
                            Err(cause) => {
                                let (_,Json(mut failure))=store_error(cause); failure["type"]=json!("error"); failure["command_id"]=value["command_id"].clone(); let _=send(&mut socket,&failure).await; break;
                            }
                        }
                        // A run may now coexist with this actor's original
                        // manual-enabled video. The run's persisted action
                        // lease still fences any new hand control, even if
                        // the viewer has not refreshed its device metadata.
                        // The Executor additionally arbitrates physical
                        // claims, covering admission/relay timing races.
                        let occupied:Result<bool,_>=sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM automation_run_devices d JOIN automation_sessions s ON s.id=? AND s.owner_user_id=? WHERE (d.device_id=s.device_id OR d.target_key=s.target_key) AND d.cleanup_confirmed=0 AND d.state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending'))"
                        ).bind(&session_id).bind(owner).fetch_one(&service.db.inner()).await;
                        match occupied {
                            Ok(false)=>{},
                            Ok(true)=>{
                                keep_running=send(&mut socket,&json!({"type":"error","command_id":value["command_id"],"code":"device_busy","message":"自动化动作正占用当前手机；手控开关状态不变，请在脚本确认结束后再操作。"})).await;
                                continue;
                            },
                            Err(_)=>{
                                keep_running=send(&mut socket,&json!({"type":"error","command_id":value["command_id"],"code":"permission_unconfirmed","message":"未能确认设备执行占用状态，已拒绝本次手动动作。"})).await;
                                continue;
                            }
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
