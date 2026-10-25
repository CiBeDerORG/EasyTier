//! One administrator-managed executor. Reads and saves never probe the network;
//! endpoint replacement explicitly retires old, idle device resources first.
use std::sync::{Arc, RwLock};

use axum::{
    Extension, Json,
    http::{HeaderMap, StatusCode},
};
use serde_json::{Value, json};
use sqlx::Row;

use super::{super::users::AuthSession, common::*, runner::Runner};
use crate::{db::Db, deploy_agent};

const CREDENTIAL_CONTEXT: &str = "automation-runner-settings:v1";

struct Saved {
    runner: Option<Runner>,
    source: &'static str,
    revision: i64,
    updated_at: Option<String>,
}

pub(crate) struct Settings {
    db: Db,
    credentials: Arc<deploy_agent::Service>,
    saved: RwLock<Saved>,
    // Dispatch/preparation and discovery cleanup hold a read lease. Settings
    // changes take the write lease, preventing a newly accepted task from
    // slipping between the active-resource check and configuration replacement.
    pub(crate) operations: tokio::sync::RwLock<()>,
}

impl Settings {
    pub(crate) async fn new(
        db: Db,
        credentials: Arc<deploy_agent::Service>,
    ) -> anyhow::Result<Self> {
        let row = sqlx::query("SELECT endpoint,token_cipher,revision,updated_at FROM automation_runner_settings WHERE id=1")
            .fetch_optional(&db.inner()).await?;
        let saved = if let Some(row) = row {
            let token = String::from_utf8(credentials.open(
                CREDENTIAL_CONTEXT,
                &row.try_get::<String, _>("token_cipher")?,
            )?)?;
            let runner = Runner::configured(&row.try_get::<String, _>("endpoint")?, token)
                .ok_or_else(|| {
                    anyhow::anyhow!("Stored automation executor configuration is invalid")
                })?;
            Saved {
                runner: Some(runner),
                source: "database",
                revision: row.try_get("revision")?,
                updated_at: Some(row.try_get("updated_at")?),
            }
        } else {
            let runner = Runner::from_environment();
            Saved {
                source: if runner.is_some() {
                    "environment"
                } else {
                    "none"
                },
                runner,
                revision: 0,
                updated_at: None,
            }
        };
        Ok(Self {
            db,
            credentials,
            saved: RwLock::new(saved),
            operations: tokio::sync::RwLock::new(()),
        })
    }

    // AEAD associated data binds this ciphertext to one account and ADB service.
    pub(crate) fn seal_unlock_password(&self, owner:i32, service_id:&str, secret:&str)->Result<String,deploy_agent::Error>{
        self.credentials.seal(&format!("adb-unlock:v1:{owner}:{service_id}"),secret.as_bytes())
    }
    pub(crate) fn open_unlock_password(&self, owner:i32, service_id:&str, cipher:&str)->Result<String,deploy_agent::Error>{
        let bytes=self.credentials.open(&format!("adb-unlock:v1:{owner}:{service_id}"),cipher)?;
        String::from_utf8(bytes).map_err(|_|deploy_agent::Error::Credential)
    }

    pub(crate) fn runner(&self) -> Option<Runner> {
        self.saved
            .read()
            .expect("executor settings lock poisoned")
            .runner
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn set_runner_for_test(&self, runner: Option<Runner>) {
        self.saved.write().unwrap().runner = runner;
    }

    pub(crate) fn view(&self) -> Value {
        let saved = self.saved.read().expect("executor settings lock poisoned");
        json!({
            "endpoint":saved.runner.as_ref().map(|runner|runner.endpoint()).unwrap_or(""),
            "token_configured":saved.runner.is_some(), "configured":saved.runner.is_some(),
            "source":saved.source, "revision":saved.revision, "updated_at":saved.updated_at,
        })
    }

    async fn ensure_idle(&self) -> HttpResult<()> {
        let busy: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM automation_sessions WHERE state NOT IN ('closed','failed'))
             OR EXISTS(SELECT 1 FROM automation_runs WHERE state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','cleanup_pending'))
             OR EXISTS(SELECT 1 FROM automation_run_devices WHERE cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending'))"
        ).fetch_one(&self.db.inner()).await.map_err(storage_error)?;
        if busy {
            return Err(error(
                StatusCode::CONFLICT,
                "runner_settings_busy",
                "还有活动画面、任务或待确认的清理。请先关闭画面并完成任务清理，再修改执行器设置。",
            ));
        }
        Ok(())
    }

    async fn retire_idle_targets(&self, runner: &Runner) -> HttpResult<()> {
        let pool = self.db.inner();
        let targets = sqlx::query("SELECT owner_user_id,device_id,executor_id,target_fingerprint,target_json FROM discovery_targets WHERE retired=0")
            .fetch_all(&pool).await.map_err(storage_error)?;
        for row in targets {
            let device: String = row.try_get("device_id").map_err(storage_error)?;
            let executor: String = row.try_get("executor_id").map_err(storage_error)?;
            let target: Value = serde_json::from_str(
                &row.try_get::<String, _>("target_json")
                    .map_err(storage_error)?,
            )
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "runner_target_invalid",
                    "旧执行器目标记录无效，无法确认清理。",
                )
            })?;
            let event = uuid::Uuid::new_v4().to_string();
            let result = runner.post(&format!("targets/{device}/retire"), &json!({
                "event_id":event,"expected_executor_id":executor,"target":wire_target(&target),
            })).await;
            if !result.is_ok_and(|value| {
                value["cleanup_confirmed"] == true
                    && value["device_id"] == device
                    && value["executor_id"] == executor
                    && value["event_id"] == event
            }) {
                return Err(error(
                    StatusCode::CONFLICT,
                    "runner_cleanup_pending",
                    "旧执行器连接尚未释放，地址保持原值。请恢复旧执行器连接后重新保存；已释放的手机需要重新连接。",
                ));
            }
            let mut tx = pool.begin().await.map_err(storage_error)?;
            sqlx::query("UPDATE discovery_targets SET retired=1 WHERE owner_user_id=? AND device_id=? AND executor_id=? AND target_fingerprint=?")
                .bind(row.try_get::<i32,_>("owner_user_id").map_err(storage_error)?)
                .bind(&device).bind(&executor).bind(row.try_get::<String,_>("target_fingerprint").map_err(storage_error)?)
                .execute(&mut *tx).await.map_err(storage_error)?;
            // Retiring a target fences its fingerprint on the old executor.
            // Advance the generation so this phone can also reconnect there if
            // a later retirement or save fails, or the URL is merely an alias.
            sqlx::query(
                "UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id=?",
            )
            .bind(&device)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
            tx.commit().await.map_err(storage_error)?;
        }
        Ok(())
    }

    pub(crate) async fn save(&self, actor: i32, payload: Value) -> HttpResult<Value> {
        exact_fields(&payload, &["endpoint", "token_action", "token", "revision"])?;
        let revision = payload["revision"]
            .as_i64()
            .filter(|value| *value >= 0)
            .ok_or_else(|| {
                error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_revision",
                    "请重新读取设置后保存。",
                )
            })?;
        let _exclusive = self.operations.try_write().map_err(|_| {
            error(
                StatusCode::CONFLICT,
                "runner_settings_busy",
                "正在处理手机操作，请待操作完成后再次保存。",
            )
        })?;
        let (old, current_revision) = {
            let saved = self.saved.read().expect("executor settings lock poisoned");
            (saved.runner.clone(), saved.revision)
        };
        if revision != current_revision {
            return Err(error(
                StatusCode::CONFLICT,
                "runner_settings_changed",
                "设置已被其他管理员修改，请重新读取后保存。",
            ));
        }
        let token = match payload["token_action"].as_str() {
            Some("preserve") if payload.get("token").is_none() => old
                .as_ref()
                .map(|runner| runner.token().to_owned())
                .ok_or_else(|| {
                    error(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "runner_token_required",
                        "首次配置请填写执行器 Token。",
                    )
                })?,
            Some("replace") => payload["token"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| {
                    error(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "runner_token_required",
                        "请填写执行器 Token。",
                    )
                })?,
            _ => {
                return Err(error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_token_action",
                    "请选择保留或替换执行器 Token。",
                ));
            }
        };
        let runner = Runner::configured(payload["endpoint"].as_str().unwrap_or(""), token)
            .ok_or_else(||error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_runner_settings","请填写 HTTP/HTTPS 基础地址（不含账号、查询或片段），Token 应符合执行器的 32～4096 字节、无控制字符要求。"))?;
        let changed = old.as_ref().is_none_or(|previous| {
            previous.endpoint() != runner.endpoint() || previous.token() != runner.token()
        });
        if changed {
            self.ensure_idle().await?;
            if let Some(previous) = old
                .as_ref()
                .filter(|previous| previous.endpoint() != runner.endpoint())
            {
                self.retire_idle_targets(previous).await?;
            }
        }
        let cipher = self
            .credentials
            .seal(CREDENTIAL_CONTEXT, runner.token().as_bytes())
            .map_err(|_| credential_error())?;
        let updated_at = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query("INSERT INTO automation_runner_settings(id,endpoint,token_cipher,revision,updated_by,updated_at) VALUES(1,?,?,1,?,?) ON CONFLICT(id) DO UPDATE SET endpoint=excluded.endpoint,token_cipher=excluded.token_cipher,revision=automation_runner_settings.revision+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at WHERE automation_runner_settings.revision=?")
            .bind(runner.endpoint()).bind(cipher).bind(actor).bind(&updated_at).bind(revision)
            .execute(&self.db.inner()).await.map_err(storage_error)?;
        if result.rows_affected() != 1 {
            return Err(error(
                StatusCode::CONFLICT,
                "runner_settings_changed",
                "设置版本已变化，请重新读取后保存。",
            ));
        }
        *self.saved.write().expect("executor settings lock poisoned") = Saved {
            runner: Some(runner),
            source: "database",
            revision: revision + 1,
            updated_at: Some(updated_at),
        };
        Ok(self.view())
    }
}

fn storage_error(_: sqlx::Error) -> HttpError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "runner_settings_storage_error",
        "无法读取或保存执行器设置。",
    )
}
fn credential_error() -> HttpError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "runner_settings_credential_error",
        "执行器凭证存储不可用，请检查服务端密钥文件。",
    )
}

async fn administrator(
    auth: &AuthSession,
    config: &super::super::ConsoleInfoConfig,
    service: &crate::central_network::service::CentralNetworkService,
    headers: &HeaderMap,
) -> HttpResult<i32> {
    require_origin(headers, false)?;
    available(config)?;
    let actor = actor(auth)?;
    if !service
        .db
        .is_active_administrator(actor)
        .await
        .map_err(|_| storage_error(sqlx::Error::RowNotFound))?
    {
        return Err(error(
            StatusCode::FORBIDDEN,
            "runner_settings_admin_required",
            "只有管理员可以管理全局执行器设置。",
        ));
    }
    Ok(actor)
}

pub(super) async fn get(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Extension(settings): Extension<Arc<Settings>>,
    headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    administrator(&auth, &config, &service, &headers).await?;
    Ok(Json(settings.view()))
}
pub(super) async fn save(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Extension(settings): Extension<Arc<Settings>>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    let actor = administrator(&auth, &config, &service, &headers).await?;
    Ok(Json(settings.save(actor, body(input)?).await?))
}
pub(super) async fn check(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Extension(settings): Extension<Arc<Settings>>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    administrator(&auth, &config, &service, &headers).await?;
    let payload = body(input)?;
    exact_fields(&payload, &["revision"])?;
    let _operation = settings.operations.read().await;
    if payload["revision"].as_i64() != settings.view()["revision"].as_i64() {
        return Err(error(
            StatusCode::CONFLICT,
            "runner_settings_changed",
            "设置已变化，请重新读取后检查。",
        ));
    }
    let runner = settings.runner().ok_or_else(|| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "runner_not_configured",
            "请先保存执行器设置。",
        )
    })?;
    let executor_id = runner.identity().await?;
    Ok(Json(
        json!({"reachable":true,"executor_id":executor_id,"protocol_version":1,"checked_at":chrono::Utc::now().to_rfc3339()}),
    ))
}

pub(super) async fn get_capacity(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Extension(settings): Extension<Arc<Settings>>,
    headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    administrator(&auth, &config, &service, &headers).await?;
    let _lease = settings.operations.read().await;
    let runner = settings.runner().ok_or_else(||error(
        StatusCode::SERVICE_UNAVAILABLE, "runner_not_configured", "请先配置并保存执行器连接。"))?;
    let expected = runner.identity().await?;
    let result = runner.get("settings/capacity").await?;
    if result["executor_id"].as_str() != Some(expected.as_str()) {
        return Err(error(StatusCode::BAD_GATEWAY, "runner_identity_invalid", "执行器容量设置身份不匹配。"));
    }
    Ok(Json(result))
}

pub(super) async fn save_capacity(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Extension(settings): Extension<Arc<Settings>>,
    headers: HeaderMap,
    input: Body,
) -> HttpResult<Json<Value>> {
    administrator(&auth, &config, &service, &headers).await?;
    let payload = body(input)?;
    exact_fields(&payload, &[
        "expected_executor_id","revision","max_running","max_pending",
        "max_media_sessions","max_media_starting",
    ])?;
    let _lease = settings.operations.read().await;
    let runner = settings.runner().ok_or_else(||error(
        StatusCode::SERVICE_UNAVAILABLE, "runner_not_configured", "请先配置并保存执行器连接。"))?;
    let current = runner.identity().await?;
    if payload["expected_executor_id"].as_str() != Some(current.as_str()) {
        return Err(error(StatusCode::CONFLICT, "runner_identity_changed",
            "执行器身份发生变化，请重新读取容量设置。"));
    }
    let mut body = serde_json::Map::new();
    for key in ["revision","max_running","max_pending","max_media_sessions","max_media_starting"] {
        let value = payload[key].as_i64().filter(|n| *n >= if key=="revision" {0} else {1})
            .ok_or_else(||error(StatusCode::UNPROCESSABLE_ENTITY,"invalid_capacity",
                "容量参数必须为正整数，设置版本不能为负数。"))?;
        body.insert(key.to_string(), json!(value));
    }
    let result = runner.request(reqwest::Method::PUT, "settings/capacity",
        Some(&Value::Object(body))).await?;
    if result["executor_id"].as_str() != Some(current.as_str()) {
        return Err(error(StatusCode::BAD_GATEWAY, "runner_identity_invalid", "执行器返回了不匹配的设置结果。"));
    }
    Ok(Json(result))
}

#[cfg(test)]
mod tests;
