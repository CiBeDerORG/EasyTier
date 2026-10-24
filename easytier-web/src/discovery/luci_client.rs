//! Fixed authenticated LuCI RPCs over the explicitly configured HTTP(S) endpoint.
//! No scan retries, redirects, or automatic protocol fallback.
use crate::db::assets::common::{AssetError, AssetResult};
use reqwest::Client;
use serde_json::{Value, json};
use std::time::Duration;

pub(crate) fn endpoint(raw: &str) -> AssetResult<url::Url> {
    let url = url::Url::parse(raw)
        .map_err(|_| AssetError::field("endpoint_url", "Use an HTTP or HTTPS /ubus endpoint."))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/ubus"
    {
        return Err(AssetError::field(
            "endpoint_url",
            "Use HTTP or HTTPS with the /ubus path, without URL credentials, query or fragment.",
        ));
    }
    Ok(url)
}
pub(crate) struct LuciClient {
    client: Client,
    url: url::Url,
}
impl LuciClient {
    pub(crate) fn new(raw: &str, ca: &str) -> AssetResult<Self> {
        let url = endpoint(raw)?;
        let mut builder = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(0)
            .tcp_keepalive(None);
        let ca = ca.trim();
        if url.scheme() == "https" && !ca.is_empty() {
            let certs = reqwest::Certificate::from_pem_bundle(ca.as_bytes()).map_err(|_| {
                AssetError::field("ca_pem", "The trusted certificate must be valid PEM.")
            })?;
            if certs.is_empty() {
                return Err(AssetError::field(
                    "ca_pem",
                    "The trusted certificate must be valid PEM.",
                ));
            }
            for cert in certs {
                builder = builder.add_root_certificate(cert);
            }
        }
        let client = builder.build().map_err(|_| {
            AssetError::new(
                500,
                "discovery_transport_unavailable",
                "Cannot initialize the LuCI transport.",
            )
        })?;
        Ok(Self { client, url })
    }
    async fn call(
        &self,
        session: &str,
        object: &str,
        method: &str,
        args: Value,
    ) -> AssetResult<Value> {
        let mut response=self.client.post(self.url.clone()).json(&json!({"jsonrpc":"2.0","id":1,"method":"call","params":[session,object,method,args]})).send().await
            .map_err(|error| {
                let https = self.url.scheme() == "https";
                AssetError::new(502,
                    if error.is_timeout() { "discovery_timeout" } else if https { "discovery_tls_or_connection_failed" } else { "discovery_connection_failed" },
                    if https { "LuCI HTTPS failed. Check reachability, certificate trust and the HTTPS port." }
                    else { "LuCI HTTP failed. Check reachability and the router's HTTP listening port (usually 80)." })
            })?;
        if response.status().is_redirection() {
            return Err(AssetError::new(
                502,
                "discovery_redirect",
                "LuCI redirected the request. Configure an address that serves /ubus directly, and check the router's HTTP-to-HTTPS redirect setting.",
            ));
        }
        if !response.status().is_success() {
            return Err(AssetError::new(
                502,
                "discovery_http_error",
                "LuCI did not accept the HTTP(S) request.",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            AssetError::new(
                502,
                "discovery_incomplete",
                "LuCI response was interrupted.",
            )
        })? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(AssetError::new(
                    502,
                    "discovery_response_too_large",
                    "LuCI response exceeds the supported size.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            AssetError::new(502, "discovery_invalid_json", "LuCI returned invalid JSON.")
        })?;
        if value["jsonrpc"] != "2.0" || value["id"] != 1 || value.get("error").is_some() {
            return Err(AssetError::new(
                502,
                "discovery_rpc_error",
                "LuCI returned a JSON-RPC error.",
            ));
        }
        let result = value["result"]
            .as_array()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                AssetError::new(502, "discovery_rpc_invalid", "LuCI result is missing.")
            })?;
        if result[0] != 0 {
            return Err(AssetError::new(
                502,
                if result[0] == 6 {
                    if object == "session" && method == "login" {
                        "discovery_login_failed"
                    } else {
                        "discovery_permission_denied"
                    }
                } else {
                    "discovery_ubus_error"
                },
                "LuCI rejected the RPC call. Check the easyadb authorization and discovery permissions.",
            ));
        }
        result
            .get(1)
            .filter(|v| v.is_object())
            .cloned()
            .ok_or_else(|| {
                AssetError::new(
                    502,
                    "discovery_rpc_invalid",
                    "LuCI returned an invalid business result.",
                )
            })
    }
    pub(crate) async fn login(&self, password: &str) -> AssetResult<String> {
        let result = self
            .call(
                "00000000000000000000000000000000",
                "session",
                "login",
                json!({"username":"easyadb","password":password,"timeout":300}),
            )
            .await?;
        result["ubus_rpc_session"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                AssetError::new(
                    502,
                    "discovery_login_failed",
                    "LuCI did not issue an authenticated session.",
                )
            })
    }
    pub(crate) async fn check(&self, password: &str) -> AssetResult<Value> {
        let session = self.login(password).await?;
        let mut capabilities = serde_json::Map::new();
        for object in ["devices", "adb"] {
            for method in ["list", "scan"] {
                let result=self.call(&session,"session","access",json!({"scope":"ubus","object":format!("easy-discovery.{object}"),"function":method})).await?;
                capabilities.insert(
                    format!("{object}.{method}"),
                    json!(result["access"] == true),
                );
            }
        }
        Ok(json!({"status":"checked","capabilities":capabilities}))
    }
    pub(crate) async fn scan(&self, password: &str, adb: bool) -> AssetResult<Value> {
        let session = self.login(password).await?;
        let result = self
            .call(
                &session,
                if adb {
                    "easy-discovery.adb"
                } else {
                    "easy-discovery.devices"
                },
                "scan",
                json!({}),
            )
            .await?;
        validate_snapshot(&result)?;
        Ok(result)
    }
}
pub(crate) fn validate_snapshot(value: &Value) -> AssetResult<()> {
    if value["schema_version"] != 1
        || !value["has_snapshot"].is_boolean()
        || !value["devices"].is_array()
        || !value["status"]
            .as_str()
            .is_some_and(|s| matches!(s, "ok" | "partial" | "error" | "busy"))
    {
        return Err(AssetError::new(
            502,
            "discovery_schema_mismatch",
            "Unsupported LuCI discovery schema. Existing assets were preserved.",
        ));
    }
    Ok(())
}
pub(crate) fn usable(value: &Value) -> bool {
    value["has_snapshot"] == true && matches!(value["status"].as_str(), Some("ok" | "partial"))
}
pub(crate) fn current_adb(value: &Value) -> bool {
    usable(value)
        && value["source"] == "avahi"
        && (value["status"] == "ok"
            || value["sources"].as_array().is_some_and(|sources| {
                sources.iter().any(|s| {
                    (s["source"] == "avahi" || s["name"] == "avahi") && s["status"] == "ok"
                })
            }))
        && !value["errors"].as_array().is_some_and(|errors| {
            errors
                .iter()
                .any(|e| e["source"] == "avahi" || e["code"] == "busy")
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        path::PathBuf,
        process::{Command, Stdio},
    };
    pub(crate) struct Mock {
        child: std::process::Child,
        pub(crate) root: PathBuf,
        pub(crate) url: String,
        pub(crate) ca: String,
    }
    impl Drop for Mock {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    impl Mock {
        pub(crate) fn start() -> Self {
            Self::start_with_scheme("https")
        }
        pub(crate) fn start_http() -> Self {
            Self::start_with_scheme("http")
        }
        fn start_with_scheme(scheme: &str) -> Self {
            easytier::utils::init_crypto_provider();
            let root = std::env::temp_dir().join(format!("ods-luci-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("requests.jsonl"), "").unwrap();
            if scheme == "https" {
                assert!(
                    Command::new("openssl")
                        .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
                        .arg(root.join("key.pem"))
                        .arg("-out")
                        .arg(root.join("cert.pem"))
                        .args([
                            "-days",
                            "1",
                            "-subj",
                            "/CN=127.0.0.1",
                            "-addext",
                            "subjectAltName=IP:127.0.0.1",
                            "-addext",
                            "basicConstraints=critical,CA:FALSE",
                            "-addext",
                            "extendedKeyUsage=serverAuth"
                        ])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .unwrap()
                        .success()
                );
            }
            let mut child = Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/luci_mock.py"
                ))
                .arg(&root)
                .arg(scheme)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut port = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut port)
                .unwrap();
            let ca = if scheme == "https" { std::fs::read_to_string(root.join("cert.pem")).unwrap() } else { String::new() };
            let mock = Self {
                child,
                root,
                url: format!("{scheme}://127.0.0.1:{}/ubus", port.trim()),
                ca,
            };
            mock.mode("ok");
            mock
        }
        fn mode(&self, status: &str) {
            std::fs::write(self.root.join("mode.json"),json!({"password":"  exact 密码  ","snapshot":{"schema_version":1,"status":status,"has_snapshot":true,"source":"avahi","devices":[{"address":"192.168.1.20","port":37001,"protocol":"IPv4","service":"_adb-tls-connect._tcp"}],"errors":[]}}).to_string()).unwrap();
        }
        pub(crate) fn requests(&self) -> Vec<Value> {
            std::fs::read_to_string(self.root.join("requests.jsonl"))
                .unwrap()
                .lines()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect()
        }
    }
    #[tokio::test]
    async fn https_login_and_capability_check_do_not_scan_and_failed_scans_remain_non_authoritative()
     {
        let mock = Mock::start();
        let client = LuciClient::new(&mock.url, &mock.ca).unwrap();
        let result = client.check("  exact 密码  ").await.unwrap();
        assert_eq!(result["capabilities"]["adb.scan"], true);
        assert!(mock.requests().iter().all(|r| r["params"][2] != "scan"));
        assert_eq!(
            client.login("exact 密码").await.unwrap_err().code,
            "discovery_login_failed"
        );
        let scan = client.scan("  exact 密码  ", true).await.unwrap();
        assert!(current_adb(&scan));
        mock.mode("error");
        let scan = client.scan("  exact 密码  ", true).await.unwrap();
        assert!(!current_adb(&scan));
        assert!(!scan["devices"].as_array().unwrap().is_empty());
        assert_eq!(
            mock.requests()
                .iter()
                .filter(|r| r["params"][2] == "scan")
                .count(),
            2
        );
        assert_eq!(
            mock.requests()
                .iter()
                .filter(|r| r["params"][2] == "list")
                .count(),
            0
        );
        let untrusted = LuciClient::new(&mock.url, "").unwrap();
        assert!(untrusted.login("  exact 密码  ").await.is_err());
    }
    #[test]
    fn endpoint_supports_explicit_http_and_https_only() {
        assert_eq!(endpoint("http://10.10.10.3/ubus").unwrap().port_or_known_default(), Some(80));
        assert_eq!(endpoint("https://10.10.10.3/ubus").unwrap().port_or_known_default(), Some(443));
        assert_eq!(endpoint("http://10.10.10.3:8080/ubus").unwrap().port(), Some(8080));
        for url in ["ftp://10.10.10.3/ubus", "http://user:secret@10.10.10.3/ubus", "http://10.10.10.3/cgi-bin/luci", "http://10.10.10.3/ubus?key=value", "http://10.10.10.3/ubus#fragment"] {
            assert!(endpoint(url).is_err(), "{url}");
        }
    }
    #[tokio::test]
    async fn http_ignores_ca_and_never_follows_a_redirect() {
        let mock = Mock::start_http();
        let target = Mock::start_http();
        let client = LuciClient::new(&mock.url, "old non-PEM text").unwrap();
        assert_eq!(client.check("  exact 密码  ").await.unwrap()["capabilities"]["adb.scan"], true);
        assert_eq!(client.login("wrong").await.unwrap_err().code, "discovery_login_failed");
        std::fs::write(mock.root.join("mode.json"), json!({"redirect": target.url}).to_string()).unwrap();
        assert_eq!(client.login("  exact 密码  ").await.unwrap_err().code, "discovery_redirect");
        assert!(target.requests().is_empty());
        assert!(!mock.requests().iter().any(|r| r["params"][2] == "scan"));
    }
    #[tokio::test]
    async fn https_accepts_blank_ca_but_still_validates_certificates() {
        let mock = Mock::start();
        let untrusted = LuciClient::new(&mock.url, " \n\t ").unwrap();
        assert_eq!(untrusted.login("  exact 密码  ").await.unwrap_err().code, "discovery_tls_or_connection_failed");
        assert!(mock.requests().is_empty());
        assert_eq!(LuciClient::new(&mock.url, "authorization-key").err().unwrap().field_errors["ca_pem"], "The trusted certificate must be valid PEM.");
    }
}
