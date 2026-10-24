//! Passive host management. Only explicit HTTP actions call a device.
//! There are no workers, timers, polling loops, retry policies or shared sessions.
use crate::db::Db;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex},
};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("Agent or task not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Remote(String),
    #[error("Storage operation failed")]
    Storage(#[from] sqlx::Error),
    #[error("Credential storage is unavailable")]
    Credential,
}
type Result<T> = std::result::Result<T, Error>;
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Deserialize)]
pub struct NewAgent {
    pub alias: String,
    pub endpoint: String,
    pub ca_pem: String,
    pub token: String,
}
#[derive(Serialize)]
pub struct AgentView {
    pub id: String,
    pub alias: String,
    pub endpoint: String,
    pub agent_id: Option<String>,
    pub identity: Option<Value>,
    pub services: Option<Value>,
    pub catalog: Option<Value>,
    pub last_error: Option<String>,
    pub updated_at: String,
}
struct Record {
    id: String,
    endpoint: String,
    ca_pem: String,
    credential: String,
    agent_id: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartTask {
    pub task_id: Uuid,
    pub application_id: String,
    pub action: String,
    pub variant: String,
    pub catalog_revision: String,
    #[serde(default)]
    pub parameters: HashMap<String, String>,
    #[serde(default)]
    pub secrets: HashMap<String, String>,
}
type Locks = Mutex<HashMap<(i32, String), Arc<AsyncMutex<()>>>>;
pub struct Service {
    db: Db,
    key: aead::LessSafeKey,
    locks: Locks,
}

impl Service {
    pub async fn new(db: Db) -> anyhow::Result<Self> {
        let mut bytes = [0u8; 32];
        if let Some(path) = db.deploy_agent_key_path() {
            if path.exists() {
                let mut file = std::fs::File::open(&path)?;
                file.read_exact(&mut bytes)?;
                anyhow::ensure!(
                    file.read(&mut [0u8])? == 0,
                    "invalid Agent credential key length"
                );
            } else {
                let records: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM deploy_agents)+(SELECT COUNT(*) FROM discovery_gateway_access WHERE secret_cipher IS NOT NULL)+(SELECT COUNT(*) FROM automation_runner_settings)")
                    .fetch_one(&db.inner())
                    .await?;
                anyhow::ensure!(
                    records == 0,
                    "Agent credential key is missing; restore the database's .deploy-agent.key backup"
                );
                SystemRandom::new()
                    .fill(&mut bytes)
                    .map_err(|_| anyhow::anyhow!("key generation failed"))?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&path)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
        } else {
            SystemRandom::new()
                .fill(&mut bytes)
                .map_err(|_| anyhow::anyhow!("key generation failed"))?;
        }
        let key = aead::UnboundKey::new(&aead::AES_256_GCM, &bytes)
            .map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
        Ok(Self {
            db,
            key: aead::LessSafeKey::new(key),
            locks: Mutex::new(HashMap::new()),
        })
    }
    pub(crate) fn seal(&self, context: &str, plain: &[u8]) -> Result<String> {
        let mut nonce = [0u8; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| Error::Credential)?;
        let mut data = plain.to_vec();
        self.key
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(context.as_bytes()),
                &mut data,
            )
            .map_err(|_| Error::Credential)?;
        Ok(STANDARD.encode([nonce.as_slice(), data.as_slice()].concat()))
    }
    pub(crate) fn open(&self, context: &str, cipher: &str) -> Result<Vec<u8>> {
        let data = STANDARD.decode(cipher).map_err(|_| Error::Credential)?;
        if data.len() < 28 {
            return Err(Error::Credential);
        }
        let nonce: [u8; 12] = data[..12].try_into().map_err(|_| Error::Credential)?;
        let mut body = data[12..].to_vec();
        self.key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(context.as_bytes()),
                &mut body,
            )
            .map(|v| v.to_vec())
            .map_err(|_| Error::Credential)
    }
    fn operation(&self, user: i32, id: &str) -> Result<OwnedMutexGuard<()>> {
        let lock = self
            .locks
            .lock()
            .map_err(|_| Error::Conflict("Operation lock unavailable".into()))?
            .entry((user, id.to_owned()))
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone();
        lock.try_lock_owned().map_err(|_| {
            Error::Conflict("Another device action is in progress; no operation was queued".into())
        })
    }
    pub async fn list(&self, user: i32) -> Result<Vec<AgentView>> {
        let rows = sqlx::query("SELECT * FROM deploy_agents WHERE user_id=? ORDER BY alias,id")
            .bind(user)
            .fetch_all(&self.db.inner())
            .await?;
        rows.into_iter()
            .map(|r| {
                Ok(AgentView {
                    id: r.try_get("id")?,
                    alias: r.try_get("alias")?,
                    endpoint: r.try_get("endpoint")?,
                    agent_id: r.try_get("agent_id")?,
                    identity: parse_optional(r.try_get("identity_json")?)?,
                    services: parse_optional(r.try_get("services_json")?)?,
                    catalog: parse_optional(r.try_get("catalog_json")?)?,
                    last_error: r.try_get("last_error")?,
                    updated_at: r.try_get("updated_at")?,
                })
            })
            .collect()
    }
    pub async fn create(&self, user: i32, request: NewAgent) -> Result<String> {
        let endpoint = validate_endpoint(&request.endpoint)?;
        if request.alias.trim().is_empty() || request.token.is_empty() {
            return Err(Error::Invalid(
                "Device name and credential are required".into(),
            ));
        }
        if !request.ca_pem.is_empty() {
            reqwest::Certificate::from_pem(request.ca_pem.as_bytes())
                .map_err(|_| Error::Invalid("Invalid trust certificate".into()))?;
        }
        let id = Uuid::new_v4().to_string();
        let credential = self.seal(&format!("{user}:{id}"), request.token.as_bytes())?;
        sqlx::query(
            "INSERT INTO deploy_agents(id,user_id,alias,endpoint,ca_pem,credential,updated_at) \
             VALUES(?,?,?,?,?,?,?)",
        )
        .bind(&id)
        .bind(user)
        .bind(request.alias)
        .bind(endpoint)
        .bind(request.ca_pem)
        .bind(credential)
        .bind(now())
        .execute(&self.db.inner())
        .await?;
        Ok(id)
    }
    async fn record(&self, user: i32, id: &str) -> Result<Record> {
        let r = sqlx::query(
            "SELECT id,endpoint,ca_pem,credential,agent_id FROM deploy_agents WHERE id=? AND user_id=?",
        )
        .bind(id)
        .bind(user)
        .fetch_optional(&self.db.inner())
        .await?
        .ok_or(Error::NotFound)?;
        Ok(Record {
            id: r.try_get("id")?,
            endpoint: r.try_get("endpoint")?,
            ca_pem: r.try_get("ca_pem")?,
            credential: r.try_get("credential")?,
            agent_id: r.try_get("agent_id")?,
        })
    }
    pub async fn delete(&self, user: i32, id: &str) -> Result<()> {
        self.record(user, id).await?;
        let _operation = self.operation(user, id)?;
        sqlx::query("DELETE FROM deploy_agents WHERE id=? AND user_id=?")
            .bind(id)
            .bind(user)
            .execute(&self.db.inner())
            .await?;
        Ok(())
    }
    async fn call(
        &self,
        user: i32,
        record: &Record,
        method: reqwest::Method,
        path: &str,
        payload: Option<&Value>,
    ) -> Result<Value> {
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .http1_only()
            .pool_idle_timeout(None)
            .pool_max_idle_per_host(0)
            .tcp_keepalive(None)
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none());
        if !record.ca_pem.is_empty() {
            let cert = reqwest::Certificate::from_pem(record.ca_pem.as_bytes())
                .map_err(|_| Error::Credential)?;
            builder = builder.tls_certs_only([cert]);
        }
        let client = builder.build().map_err(|e| Error::Remote(e.to_string()))?;
        let token = self.open(&format!("{user}:{}", record.id), &record.credential)?;
        let mut request = client
            .request(method, format!("{}{path}", record.endpoint))
            .header(
                "Authorization",
                format!("DeployToken {}", STANDARD.encode(token)),
            )
            .header("Connection", "close");
        if let Some(id) = &record.agent_id {
            request = request.header("X-Deploy-Agent-ID", id)
        }
        if let Some(payload) = payload {
            request = request.json(payload)
        }
        let mut response = request.send().await.map_err(request_error)?;
        let status = response.status();
        let response_id = response
            .headers()
            .get("X-Deploy-Agent-ID")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        if let Some(expected) = &record.agent_id {
            if expected != &response_id {
                return Err(Error::Remote(
                    "Device identity changed; response was not accepted".into(),
                ));
            }
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::Remote(e.to_string()))?
        {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(Error::Remote(
                    "Device response exceeded the allowed size".into(),
                ));
            };
            bytes.extend_from_slice(&chunk)
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Remote("Device returned invalid JSON".into()))?;
        if !status.is_success() {
            return Err(Error::Remote(format!(
                "Device returned HTTP {}: {}",
                status.as_u16(),
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("request rejected")
            )));
        }
        if response_id.is_empty()
            || value.get("agent_id").and_then(Value::as_str) != Some(response_id.as_str())
        {
            return Err(Error::Remote(
                "Missing or inconsistent device identity".into(),
            ));
        }
        Ok(value)
    }
    pub async fn refresh(&self, user: i32, id: &str, action: &str) -> Result<Value> {
        let record = self.record(user, id).await?;
        let _operation = self.operation(user, id)?;
        if action != "probe" && record.agent_id.is_none() {
            return Err(Error::Invalid(
                "Click connect to verify this device first".into(),
            ));
        }
        let (method, path, column) = match action {
            "probe" => (reqwest::Method::GET, "/v1/identity", "identity_json"),
            "discover" => (reqwest::Method::POST, "/v1/discovery", "services_json"),
            "catalog" => (reqwest::Method::GET, "/v1/catalog", "catalog_json"),
            _ => return Err(Error::NotFound),
        };
        match self.call(user, &record, method, path, None).await {
            Ok(mut value) => {
                if action == "probe"
                    && value.get("protocol_version").and_then(Value::as_i64) != Some(1)
                {
                    return Err(Error::Remote("Unsupported Agent protocol version".into()));
                }
                if action == "discover" {
                    let old: Option<String> = sqlx::query_scalar(
                        "SELECT services_json FROM deploy_agents WHERE id=? AND user_id=?",
                    )
                    .bind(id)
                    .bind(user)
                    .fetch_one(&self.db.inner())
                    .await?;
                    value = merge_discovery(parse_optional(old)?, value)?;
                }
                let bound = value
                    .get("agent_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::Remote("Missing device identity".into()))?;
                let query = format!(
                    "UPDATE deploy_agents SET {column}=?, agent_id=?, last_error=NULL,updated_at=? WHERE id=? AND user_id=?"
                );
                sqlx::query(&query)
                    .bind(value.to_string())
                    .bind(bound)
                    .bind(now())
                    .bind(id)
                    .bind(user)
                    .execute(&self.db.inner())
                    .await?;
                Ok(value)
            }
            Err(e) => {
                sqlx::query(
                    "UPDATE deploy_agents SET last_error=?,updated_at=? WHERE id=? AND user_id=?",
                )
                .bind(e.to_string())
                .bind(now())
                .bind(id)
                .bind(user)
                .execute(&self.db.inner())
                .await?;
                Err(e)
            }
        }
    }
    pub async fn start(&self, user: i32, id: &str, request: StartTask) -> Result<Value> {
        let record = self.record(user, id).await?;
        let _operation = self.operation(user, id)?;
        let agent_id = record
            .agent_id
            .as_ref()
            .ok_or_else(|| Error::Invalid("Verify the device first".into()))?;
        let task_id = request.task_id.to_string();
        let mut payload =
            serde_json::to_value(request).map_err(|_| Error::Invalid("Invalid task".into()))?;
        payload["agent_id"] = json!(agent_id);
        let context = format!("{user}:{id}:{task_id}");
        let existing = sqlx::query_scalar::<_, String>(
            "SELECT request_cipher FROM deploy_tasks WHERE connection_id=? AND task_id=? AND user_id=?",
        )
        .bind(id)
        .bind(&task_id)
        .bind(user)
        .fetch_optional(&self.db.inner())
        .await?;
        if let Some(existing) = existing {
            let old: Value = serde_json::from_slice(&self.open(&context, &existing)?)
                .map_err(|_| Error::Credential)?;
            if old != payload {
                return Err(Error::Conflict(
                    "Task ID belongs to a different request".into(),
                ));
            }
        } else {
            let cipher = self.seal(&context, payload.to_string().as_bytes())?;
            let initial = json!({
                "task_id": task_id,
                "status": "outcome_unknown",
                "application_id": payload["application_id"],
                "action": payload["action"],
                "message": "Submission result has not been verified"
            });
            sqlx::query(
                "INSERT INTO deploy_tasks(task_id,connection_id,user_id,request_cipher,result_json,updated_at) \
                 VALUES(?,?,?,?,?,?)",
            )
            .bind(&task_id)
            .bind(id)
            .bind(user)
            .bind(cipher)
            .bind(initial.to_string())
            .bind(now())
            .execute(&self.db.inner())
            .await?;
        }
        let value = self
            .call(
                user,
                &record,
                reqwest::Method::POST,
                "/v1/tasks",
                Some(&payload),
            )
            .await?;
        if value["task_id"] != task_id {
            return Err(Error::Remote("Task response identity mismatch".into()));
        }
        self.save_task(user, id, &task_id, &value).await?;
        Ok(value)
    }
    async fn save_task(&self, user: i32, id: &str, task: &str, value: &Value) -> Result<()> {
        sqlx::query(
            "UPDATE deploy_tasks SET result_json=?,updated_at=? \
             WHERE connection_id=? AND task_id=? AND user_id=?",
        )
        .bind(value.to_string())
        .bind(now())
        .bind(id)
        .bind(task)
        .bind(user)
        .execute(&self.db.inner())
        .await?;
        Ok(())
    }
    pub async fn resubmit(&self, user: i32, id: &str, task: Uuid) -> Result<Value> {
        self.record(user, id).await?;
        let task = task.to_string();
        let cipher = sqlx::query_scalar::<_, String>(
            "SELECT request_cipher FROM deploy_tasks WHERE connection_id=? AND task_id=? AND user_id=?",
        )
        .bind(id)
        .bind(&task)
        .bind(user)
        .fetch_optional(&self.db.inner())
        .await?
        .ok_or(Error::NotFound)?;
        let mut value: Value =
            serde_json::from_slice(&self.open(&format!("{user}:{id}:{task}"), &cipher)?)
                .map_err(|_| Error::Credential)?;
        value
            .as_object_mut()
            .ok_or(Error::Credential)?
            .remove("agent_id");
        let request = serde_json::from_value(value).map_err(|_| Error::Credential)?;
        self.start(user, id, request).await
    }
    pub async fn tasks(&self, user: i32, id: &str) -> Result<Vec<Value>> {
        self.record(user, id).await?;
        let rows = sqlx::query(
            "SELECT result_json,logs,log_offset,updated_at FROM deploy_tasks \
             WHERE connection_id=? AND user_id=? ORDER BY updated_at DESC",
        )
        .bind(id)
        .bind(user)
        .fetch_all(&self.db.inner())
        .await?;
        rows.into_iter()
            .map(|r| {
                let raw: String = r.try_get("result_json")?;
                let mut value = parse(&raw)?;
                value["logs"] = json!(r.try_get::<String, _>("logs")?);
                value["log_offset"] = json!(r.try_get::<i64, _>("log_offset")?);
                value["queried_at"] = json!(r.try_get::<String, _>("updated_at")?);
                Ok(value)
            })
            .collect()
    }
    pub async fn refresh_task(&self, user: i32, id: &str, task: Uuid, logs: bool) -> Result<Value> {
        let record = self.record(user, id).await?;
        let _operation = self.operation(user, id)?;
        let task = task.to_string();
        let row = sqlx::query(
            "SELECT log_offset,logs FROM deploy_tasks WHERE connection_id=? AND task_id=? AND user_id=?",
        )
        .bind(id)
        .bind(&task)
        .bind(user)
        .fetch_optional(&self.db.inner())
        .await?
        .ok_or(Error::NotFound)?;
        let offset: i64 = row.try_get("log_offset")?;
        let path = if logs {
            format!("/v1/tasks/{task}/logs?offset={offset}")
        } else {
            format!("/v1/tasks/{task}")
        };
        let value = self
            .call(user, &record, reqwest::Method::GET, &path, None)
            .await?;
        if value["task_id"] != task {
            return Err(Error::Remote("Task response identity mismatch".into()));
        }
        if logs {
            let next = value["next_offset"]
                .as_i64()
                .filter(|n| *n >= offset)
                .ok_or_else(|| Error::Remote("Invalid log cursor".into()))?;
            let chunk = value["text"]
                .as_str()
                .ok_or_else(|| Error::Remote("Invalid log response".into()))?;
            let mut text: String = row.try_get("logs")?;
            if text.len() + chunk.len() > 512 * 1024 {
                return Err(Error::Remote("Log storage limit reached".into()));
            };
            text.push_str(chunk);
            sqlx::query(
                "UPDATE deploy_tasks SET logs=?,log_offset=? \
                 WHERE connection_id=? AND task_id=? AND user_id=?",
            )
            .bind(text)
            .bind(next)
            .bind(id)
            .bind(&task)
            .bind(user)
            .execute(&self.db.inner())
            .await?;
        } else {
            self.save_task(user, id, &task, &value).await?
        }
        Ok(value)
    }
}
fn request_error(error: reqwest::Error) -> Error {
    let mut message = format!("Device request failed: {error}");
    let mut cause = std::error::Error::source(&error);
    while let Some(error) = cause {
        message.push_str(&format!(": {error}"));
        cause = error.source();
    }
    Error::Remote(message)
}
fn parse(raw: &str) -> Result<Value> {
    serde_json::from_str(raw).map_err(|_| Error::Invalid("Stored snapshot is invalid".into()))
}
fn parse_optional(raw: Option<String>) -> Result<Option<Value>> {
    raw.map(|s| parse(&s)).transpose()
}
pub fn validate_endpoint(input: &str) -> Result<String> {
    let url = url::Url::parse(input).map_err(|_| Error::Invalid("Invalid Agent URL".into()))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(Error::Invalid(
            "Use an HTTPS origin without user info, path, query or fragment".into(),
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}
pub fn merge_discovery(previous: Option<Value>, mut current: Value) -> Result<Value> {
    let providers = current["providers"]
        .as_object()
        .ok_or_else(|| Error::Remote("Missing provider results".into()))?
        .clone();
    let observed = current["observed_at"].clone();
    let rows = current["services"]
        .as_array_mut()
        .ok_or_else(|| Error::Remote("Missing service results".into()))?;
    for row in rows.iter_mut() {
        row["observed_at"] = observed.clone();
        row["stale"] = json!(false)
    }
    if let Some(old) = previous {
        if let Some(old_rows) = old["services"].as_array() {
            for row in old_rows {
                let provider = row["provider"].as_str().unwrap_or_default();
                if providers.get(provider).and_then(|p| p["status"].as_str()) != Some("ok") {
                    let mut retained = row.clone();
                    retained["stale"] = json!(true);
                    rows.push(retained)
                }
            }
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_origins_without_redirect_or_url_credentials() {
        assert_eq!(
            validate_endpoint("https://192.168.1.20:7014/").unwrap(),
            "https://192.168.1.20:7014"
        );
        for input in [
            "http://host:7014",
            "https://user:pass@host",
            "https://host/path",
            "https://host/?token=x",
            "https://host/#x",
            "file:///etc/passwd",
        ] {
            assert!(validate_endpoint(input).is_err());
        }
    }

    #[test]
    fn failed_provider_keeps_only_its_previous_snapshot() {
        let previous = json!({"services":[{"id":"old-docker","provider":"docker","observed_at":"old"},{"id":"stopped-systemd","provider":"systemd","observed_at":"old"}]});
        let current = json!({"agent_id":"a","observed_at":"new","providers":{"docker":{"status":"error"},"systemd":{"status":"ok"}},"services":[{"id":"current-systemd","provider":"systemd"}]});
        let merged = merge_discovery(Some(previous), current).unwrap();
        let rows = merged["services"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["observed_at"], "new");
        assert_eq!(rows[1]["id"], "old-docker");
        assert_eq!(rows[1]["observed_at"], "old");
        assert_eq!(rows[1]["stale"], true);
    }

    async fn fixture() -> (Service, i32, i32) {
        easytier::utils::init_crypto_provider();
        let db = Db::memory_db().await;
        let one = db
            .create_user_and_join_users_group("deploy-test-one", "test-password-hash".into())
            .await
            .unwrap()
            .id;
        let two = db
            .create_user_and_join_users_group("deploy-test-two", "test-password-hash".into())
            .await
            .unwrap()
            .id;
        (Service::new(db).await.unwrap(), one, two)
    }

    #[tokio::test]
    async fn credentials_are_encrypted_and_devices_are_tenant_scoped() {
        let (service, one, two) = fixture().await;
        let original = " spaces ' \" 中文 $ ! ";
        let id = service
            .create(
                one,
                NewAgent {
                    alias: "NAS".into(),
                    endpoint: "https://127.0.0.1:9".into(),
                    ca_pem: "".into(),
                    token: original.into(),
                },
            )
            .await
            .unwrap();
        let record = service.record(one, &id).await.unwrap();
        assert_ne!(record.credential, original);
        assert_eq!(
            service
                .open(&format!("{one}:{id}"), &record.credential)
                .unwrap(),
            original.as_bytes()
        );
        assert!(
            service
                .open(&format!("{two}:{id}"), &record.credential)
                .is_err()
        );
        assert!(service.list(two).await.unwrap().is_empty());
        assert!(matches!(
            service.record(two, &id).await,
            Err(Error::NotFound)
        ));
        let public = serde_json::to_string(&service.list(one).await.unwrap()).unwrap();
        assert!(!public.contains("credential"));
        assert!(!public.contains("token"));
        assert!(!public.contains(original));
        // create/list above succeed without dialing an unreachable device.
        assert!(service.list(one).await.unwrap()[0].identity.is_none());
    }

    #[tokio::test]
    async fn lost_submission_is_durable_and_conflicting_retry_is_rejected() {
        let (service, user, _) = fixture().await;
        let id = service
            .create(
                user,
                NewAgent {
                    alias: "NAS".into(),
                    endpoint: "https://127.0.0.1:9".into(),
                    ca_pem: "".into(),
                    token: "device-credential".into(),
                },
            )
            .await
            .unwrap();
        sqlx::query("UPDATE deploy_agents SET agent_id='bound' WHERE id=?")
            .bind(&id)
            .execute(&service.db.inner())
            .await
            .unwrap();
        let task = Uuid::new_v4();
        let make = |action: &str| StartTask {
            task_id: task,
            application_id: "portainer".into(),
            action: action.into(),
            variant: "docker-compose".into(),
            catalog_revision: "revision".into(),
            parameters: HashMap::new(),
            secrets: HashMap::from([("PORTAINER_ADMIN_PASSWORD".into(), "user-secret".into())]),
        };
        assert!(matches!(
            service.start(user, &id, make("install")).await,
            Err(Error::Remote(_))
        ));
        let tasks = service.tasks(user, &id).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["status"], "outcome_unknown");
        assert!(!tasks[0].to_string().contains("user-secret"));
        assert!(matches!(
            service.start(user, &id, make("update")).await,
            Err(Error::Conflict(_))
        ));
        assert_eq!(service.tasks(user, &id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn persistent_key_survives_restart_and_is_not_replaced_when_missing() {
        let directory = std::env::temp_dir().join(format!("deploy-key-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("test.db");
        let db = Db::new(path.to_string_lossy()).await.unwrap();
        let user = db
            .create_user_and_join_users_group("key-test", "unused".into())
            .await
            .unwrap()
            .id;
        let service = Service::new(db.clone()).await.unwrap();
        let id = service
            .create(
                user,
                NewAgent {
                    alias: "NAS".into(),
                    endpoint: "https://localhost:7014".into(),
                    ca_pem: "".into(),
                    token: "preserved device value".into(),
                },
            )
            .await
            .unwrap();
        let restarted = Service::new(db.clone()).await.unwrap();
        let record = restarted.record(user, &id).await.unwrap();
        assert_eq!(
            restarted
                .open(&format!("{user}:{id}"), &record.credential)
                .unwrap(),
            b"preserved device value"
        );
        let key = db.deploy_agent_key_path().unwrap();
        std::fs::remove_file(&key).unwrap();
        assert!(Service::new(db.clone()).await.is_err());
        assert!(
            !key.exists(),
            "missing key must not generate a replacement over an encrypted database"
        );
        drop(restarted);
        drop(service);
        db.inner().close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }
}
