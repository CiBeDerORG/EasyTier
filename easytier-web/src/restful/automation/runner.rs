//! The only outbound transport to a configured, trusted automation executor.
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::common::{HttpResult, error};

#[derive(Clone)]
pub(crate) struct Runner {
    base: url::Url,
    token: String,
    client: Client,
}

impl Runner {
    pub(crate) fn endpoint(&self) -> &str { self.base.as_str() }
    pub(crate) fn token(&self) -> &str { &self.token }
    fn authorization(&self) -> String {
        // HTTP field whitespace and non-ASCII header conversions must not
        // change a user-selected credential. Keep legacy Bearer for the
        // existing printable-ASCII tokens; encode other UTF-8 values losslessly.
        if self.token.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
            format!("Bearer {}", self.token)
        } else {
            format!("AutomationToken {}", STANDARD.encode(self.token.as_bytes()))
        }
    }
    pub(crate) fn from_environment() -> Option<Self> {
        let raw = std::env::var("EASYTIER_AUTOMATION_RUNNER_URL").ok()?;
        let token = std::env::var("EASYTIER_AUTOMATION_RUNNER_TOKEN").ok()?;
        Self::configured(&raw,token)
    }
    pub(crate) fn configured(raw:&str,token:String)->Option<Self>{
        let mut base = url::Url::parse(raw.trim()).ok()?;
        if !matches!(base.scheme(), "http" | "https") || base.host_str().is_none()
            || !base.username().is_empty() || base.password().is_some()
            || base.query().is_some() || base.fragment().is_some()
            || token.len() < 32 || token.len() > 4096 || token.chars().any(char::is_control)
        {
            tracing::warn!("automation executor configuration is invalid; automation execution remains unavailable");
            return None;
        }
        let path = format!("{}/", base.path().trim_end_matches('/'));
        base.set_path(&path);
        let client = Client::builder().no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(45))
            .build().ok()?;
        Some(Self { base, token, client })
    }

    pub(crate) fn authenticate(&self, headers: &HeaderMap) -> HttpResult<()> {
        let supplied = headers.get(axum::http::header::AUTHORIZATION).and_then(|value| {
            let raw = value.as_bytes();
            if let Some(encoded) = raw.strip_prefix(b"AutomationToken ") { STANDARD.decode(encoded).ok() }
            else { raw.strip_prefix(b"Bearer ").map(|token| token.to_vec()) }
        });
        let valid = supplied.is_some_and(|value| value.as_slice().ct_eq(self.token.as_bytes()).into());
        if !valid {
            return Err(error(StatusCode::UNAUTHORIZED,"runner_authentication_required","执行器认证失败。"));
        }
        Ok(())
    }

    pub(crate) async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> HttpResult<Value> {
        // Paths are built exclusively from fixed route names and parsed UUIDs.
        let url = self.base.join(&format!("internal/v1/{}",path.trim_start_matches('/')))
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE,"runner_not_configured","执行器地址配置无效。"))?;
        let seconds = if path.ends_with("/force-cleanup") { 65 }
            else if path.ends_with("/offer") || path == "prepare" { 75 }
            else if path == "automation/status" { 15 }
            else if path == "automation/hierarchy" || path == "automation/locator-verify" { 35 }
            else if path.ends_with("/unlock") { 40 }
            else if path.ends_with("/commands") { 8 }
            else if path.ends_with("/renew") { 5 }
            else if path.ends_with("/close") { 15 }
            else { 10 };
        let mut request = self.client.request(method, url).header(axum::http::header::AUTHORIZATION, self.authorization())
            .timeout(Duration::from_secs(seconds));
        if let Some(body) = body { request = request.json(body); }
        let mut response = request.send().await.map_err(|_| error(StatusCode::BAD_GATEWAY,"runner_unreachable","无法确认执行器响应。请检查连接并查询结果，不要重复执行未知操作。"))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(error(StatusCode::BAD_GATEWAY,"runner_redirect_denied","执行器返回了不允许的重定向。"));
        }
        if response.content_length().is_some_and(|size| size > 2 * 1024 * 1024) {
            return Err(error(StatusCode::BAD_GATEWAY,"runner_response_invalid","执行器响应超过大小限制。"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| error(StatusCode::BAD_GATEWAY,"runner_response_incomplete","执行器响应中断，操作结果尚未确认。"))? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(error(StatusCode::BAD_GATEWAY,"runner_response_invalid","执行器响应超过大小限制。"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let payload: Value = if bytes.is_empty() && status.is_success() { json!({}) } else {
            serde_json::from_slice(&bytes).map_err(|_| error(StatusCode::BAD_GATEWAY,"runner_response_invalid","执行器未返回有效响应。"))?
        };
        if !status.is_success() {
            let detail = payload.get("detail").filter(|v| v.is_object()).unwrap_or(&payload);
            let code = detail["code"].as_str().filter(|s| s.len() <= 80).unwrap_or("runner_operation_failed");
            let message = detail["message"].as_str().filter(|s| s.len() <= 1024).unwrap_or("手机操作未完成，请查看设备状态。");
            let status = if status.is_client_error() { status } else { StatusCode::BAD_GATEWAY };
            return Err(error(status, code, message));
        }
        if !payload.is_object() {
            return Err(error(StatusCode::BAD_GATEWAY,"runner_response_invalid","执行器响应格式无效。"));
        }
        Ok(payload)
    }

    pub(crate) async fn post(&self, path: &str, body: &Value) -> HttpResult<Value> {
        self.request(Method::POST, path, Some(body)).await
    }

    pub(crate) async fn get(&self, path: &str) -> HttpResult<Value> {
        self.request(Method::GET, path, None).await
    }

    pub(crate) async fn identity(&self) -> HttpResult<String> {
        let health = self.get("health").await?;
        if health["protocol_version"] != 1 {
            return Err(error(StatusCode::BAD_GATEWAY,"runner_protocol_mismatch","执行器协议版本不兼容，请先更新执行器。"));
        }
        super::common::id(health["executor_id"].as_str().unwrap_or_default())
            .map_err(|_|error(StatusCode::BAD_GATEWAY,"runner_identity_invalid","执行器没有有效的持久身份。"))
    }

    pub(crate) async fn artifact(&self, run_id: &str, artifact_id: &str) -> HttpResult<(String,Vec<u8>)> {
        let url = self.base.join(&format!("internal/v1/runs/{run_id}/artifacts/{artifact_id}"))
            .map_err(|_|error(StatusCode::SERVICE_UNAVAILABLE,"runner_not_configured","执行器地址配置无效。"))?;
        let mut response = self.client.get(url).header(axum::http::header::AUTHORIZATION, self.authorization()).timeout(Duration::from_secs(15)).send().await
            .map_err(|_|error(StatusCode::BAD_GATEWAY,"artifact_unavailable","暂时无法读取执行截图。"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(error(StatusCode::NOT_FOUND,"artifact_not_found","该执行截图不存在或已清理。"));
        }
        if !response.status().is_success() {
            return Err(error(StatusCode::BAD_GATEWAY,"artifact_unavailable","执行器未能提供该截图。"));
        }
        let mime = response.headers().get(axum::http::header::CONTENT_TYPE)
            .and_then(|value|value.to_str().ok()).and_then(|value|value.split(';').next()).unwrap_or("").trim().to_owned();
        const LIMIT: usize = 8 * 1024 * 1024;
        if !matches!(mime.as_str(),"image/png"|"image/jpeg") || response.content_length().is_some_and(|size|size>LIMIT as u64) {
            return Err(error(StatusCode::BAD_GATEWAY,"artifact_invalid","截图格式或大小不符合限制。"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_|error(StatusCode::BAD_GATEWAY,"artifact_incomplete","截图传输未完成。"))? {
            if bytes.len()+chunk.len()>LIMIT {
                return Err(error(StatusCode::BAD_GATEWAY,"artifact_invalid","截图超过大小限制。"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let valid = if mime=="image/png" {bytes.starts_with(b"\x89PNG\r\n\x1a\n")} else {bytes.starts_with(b"\xff\xd8\xff")};
        if !valid {return Err(error(StatusCode::BAD_GATEWAY,"artifact_invalid","截图内容与声明的格式不符。"));}
        Ok((mime,bytes))
    }
}
