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
struct LuciSession {
    id: String,
    acls: Option<Value>,
}

fn rpc_error(object: &str, method: &str, code: &str, message: &str) -> AssetError {
    // Object and method are fixed by this client. Never include remote error
    // messages, response bodies, request arguments, passwords or session IDs.
    AssetError::new(502, code, &format!("LuCI {object}.{method}: {message}"))
}

fn rpc_result(value: Value, object: &str, method: &str) -> AssetResult<Value> {
    let invalid = |message| rpc_error(object, method, "discovery_rpc_invalid", message);
    if value["jsonrpc"] != "2.0" || value["id"] != 1 {
        return Err(invalid("Invalid JSON-RPC version or response ID."));
    }
    if let Some(error) = value.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid("Invalid JSON-RPC error code."))?;
        if value.get("result").is_some()
            || !error.is_object()
            || !error["message"].is_string()
        {
            return Err(invalid("Invalid JSON-RPC error response."));
        }
        // uhttpd's numeric JSON-RPC codes are distinct from the numeric ubus
        // status inside result[]. Keep all descriptions local and non-secret.
        let (category, description) = match code {
            -32000 => ("discovery_rpc_object_not_found", "RPC object not found."),
            -32001 => ("discovery_session_not_found", "Session not found."),
            -32002 => (
                "discovery_permission_denied",
                "Access denied. Check the easyadb permissions for this RPC.",
            ),
            -32003 => ("discovery_timeout", "The ubus request timed out."),
            -32601 => ("discovery_rpc_method_not_found", "RPC method not found."),
            _ => ("discovery_rpc_error", "RPC failed."),
        };
        return Err(rpc_error(
            object,
            method,
            category,
            &format!("JSON-RPC error {code}. {description}"),
        ));
    }
    let result = value["result"]
        .as_array()
        .ok_or_else(|| invalid("The ubus result array is missing."))?;
    let status = result
        .first()
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("The ubus status is missing or invalid."))?;
    if status != 0 {
        return Err(rpc_error(
            object,
            method,
            if status == 6 {
                if object == "session" && method == "login" {
                    "discovery_login_failed"
                } else {
                    "discovery_permission_denied"
                }
            } else {
                "discovery_ubus_error"
            },
            &format!(
                "ubus status {status}. Check the easyadb authorization and discovery permissions."
            ),
        ));
    }
    result
        .get(1)
        .filter(|v| v.is_object())
        .cloned()
        .ok_or_else(|| invalid("The business result is missing or invalid."))
}

fn acl_error(code: &str, message: &str) -> AssetError {
    rpc_error("session", "login", code, message)
}

fn supported_acl_pattern(pattern: &str) -> AssetResult<()> {
    // rpcd uses POSIX fnmatch. The companion application's grants are
    // exact names; support ASCII '*' as well, without pretending to implement
    // all POSIX patterns. Reject unsupported syntax instead of reporting a
    // possibly incorrect allow/deny result. Do not echo router-provided grants.
    if !pattern.is_ascii()
        || pattern
            .bytes()
            .any(|c| c.is_ascii_control() || matches!(c, b'?' | b'[' | b'\\'))
    {
        return Err(acl_error(
            "discovery_acl_unsupported",
            "Cannot check this ACL pattern. Only exact ASCII names and '*' are supported.",
        ));
    }
    Ok(())
}

fn acl_pattern_matches(pattern: &str, value: &str) -> bool {
    let (pattern, value) = (pattern.as_bytes(), value.as_bytes());
    let (mut p, mut v) = (0, 0);
    let (mut star, mut star_end) = (None, 0);
    while v < value.len() {
        if pattern.get(p) == Some(&b'*') {
            star = Some(p);
            star_end = v;
            p += 1;
        } else if pattern.get(p) == value.get(v) {
            p += 1;
            v += 1;
        } else if let Some(position) = star {
            star_end += 1;
            v = star_end;
            p = position + 1;
        } else {
            return false;
        }
    }
    while pattern.get(p) == Some(&b'*') {
        p += 1;
    }
    p == pattern.len()
}

fn login_capabilities(acls: Option<&Value>) -> AssetResult<serde_json::Map<String, Value>> {
    let invalid = || {
        acl_error(
            "discovery_acl_invalid",
            "The authenticated session ACLs are missing or invalid; permissions could not be checked.",
        )
    };
    let acls = acls.and_then(Value::as_object).ok_or_else(invalid)?;
    let mut grants = Vec::new();
    if let Some(ubus) = acls.get("ubus") {
        let ubus = ubus.as_object().ok_or_else(invalid)?;
        for (object, methods) in ubus {
            supported_acl_pattern(object)?;
            for method in methods.as_array().ok_or_else(invalid)? {
                let method = method.as_str().ok_or_else(invalid)?;
                supported_acl_pattern(method)?;
                grants.push((object.as_str(), method));
            }
        }
    }
    // session.login already resolves read/write groups and UCI '!group'
    // exclusions. Its acls.ubus is the effective union of grants, not a set of
    // role names or ordered allow/deny rules. An absent ubus scope grants none.
    let mut capabilities = serde_json::Map::new();
    for object in ["devices", "adb"] {
        for method in ["list", "scan"] {
            let full_object = format!("easy-discovery.{object}");
            let allowed = grants.iter().any(|(object_pattern, method_pattern)| {
                acl_pattern_matches(object_pattern, &full_object)
                    && acl_pattern_matches(method_pattern, method)
            });
            capabilities.insert(format!("{object}.{method}"), json!(allowed));
        }
    }
    Ok(capabilities)
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
                rpc_error(object, method,
                    if error.is_timeout() { "discovery_timeout" } else if https { "discovery_tls_or_connection_failed" } else { "discovery_connection_failed" },
                    if https { "LuCI HTTPS failed. Check reachability, certificate trust and the HTTPS port." }
                    else { "LuCI HTTP failed. Check reachability and the router's HTTP listening port (usually 80)." })
            })?;
        if response.status().is_redirection() {
            return Err(rpc_error(
                object,
                method,
                "discovery_redirect",
                "LuCI redirected the request. Configure an address that serves /ubus directly, and check the router's HTTP-to-HTTPS redirect setting.",
            ));
        }
        if !response.status().is_success() {
            return Err(rpc_error(
                object,
                method,
                "discovery_http_error",
                &format!("HTTP status {}.", response.status().as_u16()),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            rpc_error(
                object,
                method,
                "discovery_incomplete",
                "LuCI response was interrupted.",
            )
        })? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(rpc_error(
                    object,
                    method,
                    "discovery_response_too_large",
                    "LuCI response exceeds the supported size.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            rpc_error(object, method, "discovery_invalid_json", "Invalid JSON response.")
        })?;
        rpc_result(value, object, method)
    }
    async fn login_session(&self, password: &str) -> AssetResult<LuciSession> {
        let result = self
            .call(
                "00000000000000000000000000000000",
                "session",
                "login",
                json!({"username":"easyadb","password":password,"timeout":300}),
            )
            .await?;
        let id = result["ubus_rpc_session"]
            .as_str()
            .filter(|s| {
                s.len() == 32
                    && s.bytes().all(|c| c.is_ascii_hexdigit())
                    && s.bytes().any(|c| c != b'0')
            })
            .map(str::to_owned)
            .ok_or_else(|| {
                rpc_error(
                    "session",
                    "login",
                    "discovery_login_failed",
                    "LuCI did not issue an authenticated session.",
                )
            })?;
        Ok(LuciSession {
            id,
            acls: result.get("acls").cloned(),
        })
    }
    pub(crate) async fn login(&self, password: &str) -> AssetResult<String> {
        Ok(self.login_session(password).await?.id)
    }
    pub(crate) async fn check(&self, password: &str) -> AssetResult<Value> {
        let session = self.login_session(password).await?;
        // A restricted easyadb account can use discovery RPCs without being
        // allowed to invoke session.access through uhttpd. login already returns
        // that session's effective grants; checking them needs no extra RPC.
        let capabilities = login_capabilities(session.acls.as_ref())?;
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
        fn update_mode(&self, changes: Value) {
            let path = self.root.join("mode.json");
            let mut mode: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            mode.as_object_mut().unwrap().extend(changes.as_object().unwrap().clone());
            std::fs::write(path, mode.to_string()).unwrap();
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
        assert_eq!(mock.requests().len(), 1);
        assert_eq!(mock.requests()[0]["params"][1], "session");
        assert_eq!(mock.requests()[0]["params"][2], "login");
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

    #[tokio::test]
    async fn restricted_account_check_uses_only_login_and_preserves_denied_capabilities() {
        let mock = Mock::start_http();
        mock.update_mode(json!({"acls": {"ubus": {
            "easy-discovery.devices": ["list"],
            "easy-discovery.adb": ["scan"]
        }}}));
        let client = LuciClient::new(&mock.url, "").unwrap();
        let result = client.check("  exact 密码  ").await.unwrap();
        assert_eq!(result, json!({"status": "checked", "capabilities": {
            "devices.list": true, "devices.scan": false,
            "adb.list": false, "adb.scan": true
        }}));
        let requests = mock.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["params"][1], "session");
        assert_eq!(requests[0]["params"][2], "login");
        assert_eq!(requests[0]["params"][3]["username"], "easyadb");
        assert_eq!(requests[0]["params"][3]["password"], "  exact 密码  ");

        // The same account cannot invoke session.access through uhttpd. The
        // original check implementation fails against this realistic fixture.
        let error = client.call(
            &"a".repeat(32), "session", "access",
            json!({"scope": "ubus", "object": "easy-discovery.adb", "function": "scan"}),
        ).await.unwrap_err();
        assert_eq!(error.code, "discovery_permission_denied");
        assert!(error.message.contains("session.access"));
        assert!(error.message.contains("-32002"));
        assert!(!mock.requests().iter().any(|r| r["params"][2] == "scan" || r["params"][2] == "list"));
    }

    #[test]
    fn empty_effective_acls_grant_nothing_and_malformed_acls_are_errors() {
        let denied = json!({
            "devices.list": false, "devices.scan": false,
            "adb.list": false, "adb.scan": false
        });
        for acls in [
            json!({}),
            json!({"ubus": {}}),
            json!({"ubus": {"easy-discovery.devices": []}}),
            json!({"access-group": {"easy-discovery-adb-scan": ["read"]}}),
        ] {
            assert_eq!(json!(login_capabilities(Some(&acls)).unwrap()), denied);
        }
        assert_eq!(login_capabilities(None).unwrap_err().code, "discovery_acl_invalid");
        for acls in [
            Value::Null,
            json!([]),
            json!({"ubus": null}),
            json!({"ubus": []}),
            json!({"ubus": {"easy-discovery.adb": "scan"}}),
            json!({"ubus": {"easy-discovery.adb": ["scan", false]}}),
        ] {
            let error = login_capabilities(Some(&acls)).unwrap_err();
            assert_eq!(error.code, "discovery_acl_invalid");
            assert!(error.message.contains("session.login"));
        }
    }

    #[test]
    fn effective_acl_patterns_match_exact_names_and_stars_without_guessing_posix_syntax() {
        for (pattern, value, expected) in [
            ("easy-discovery.adb", "easy-discovery.adb", true),
            ("easy-discovery.adb", "easy-discovery.devices", false),
            ("easy-discovery.*", "easy-discovery.adb", true),
            ("*adb", "easy-discovery.adb", true),
            ("*adb", "easy-discovery.adb.extra", false),
            ("e*y-*.*b", "easy-discovery.adb", true),
            ("e*y-*.*x", "easy-discovery.adb", false),
            ("s*n", "scan", true),
            ("scan*", "scan", true),
            ("**", "scan", true),
            ("*", "", true),
            ("", "scan", false),
        ] {
            supported_acl_pattern(pattern).unwrap();
            assert_eq!(acl_pattern_matches(pattern, value), expected, "{pattern} / {value}");
        }
        let acls = json!({"ubus": {
            "easy-discovery.*": ["s*"],
            "easy-discovery.devices": ["l*"],
            "!easy-discovery.adb": ["*"]
        }});
        assert_eq!(json!(login_capabilities(Some(&acls)).unwrap()), json!({
            "devices.list": true, "devices.scan": true,
            "adb.list": false, "adb.scan": true
        }));
        for pattern in ["?", "[as]*", "\\*", "发现", "bad\u{0}name"] {
            for acls in [
                json!({"ubus": {"*": ["*"], (pattern): ["scan"]}}),
                json!({"ubus": {"*": ["*", pattern]}}),
            ] {
                let error = login_capabilities(Some(&acls)).unwrap_err();
                assert_eq!(error.code, "discovery_acl_unsupported");
                assert!(error.message.contains("session.login"));
            }
        }
    }

    #[tokio::test]
    async fn check_requires_a_valid_nonanonymous_login_session_even_with_full_grants() {
        let mock = Mock::start_http();
        let client = LuciClient::new(&mock.url, "").unwrap();
        let invalid_ids = [
            Value::Null,
            json!(""),
            json!("x"),
            json!("a".repeat(31)),
            json!("a".repeat(33)),
            json!("g".repeat(32)),
            json!("ａ".repeat(32)),
            json!("0".repeat(32)),
            json!(123),
            json!(true),
            json!([]),
            json!({}),
        ];
        for id in invalid_ids {
            mock.update_mode(json!({"login_result": {
                "ubus_rpc_session": id, "acls": {"ubus": {"*": ["*"]}}
            }}));
            let before = mock.requests().len();
            let error = client.check("  exact 密码  ").await.unwrap_err();
            assert_eq!(error.code, "discovery_login_failed");
            assert_eq!(error.message, "LuCI session.login: LuCI did not issue an authenticated session.");
            assert!(error.field_errors.is_empty());
            assert_eq!(mock.requests().len(), before + 1);
        }
        // A missing ID must also fail even when the reported grants allow all.
        mock.update_mode(json!({"login_result": {"acls": {"ubus": {"*": ["*"]}}}}));
        let before = mock.requests().len();
        assert_eq!(client.check("  exact 密码  ").await.unwrap_err().code, "discovery_login_failed");
        assert_eq!(mock.requests().len(), before + 1);

        for id in [
            "a".repeat(32),
            "A".repeat(32),
            "0123456789aBcDeF".repeat(2),
            format!("{}1", "0".repeat(31)),
        ] {
            mock.update_mode(json!({"login_result": {
                "ubus_rpc_session": id, "acls": {"ubus": {"*": ["*"]}}
            }}));
            let before = mock.requests().len();
            let result = client.check("  exact 密码  ").await.unwrap();
            assert_eq!(result, json!({"status": "checked", "capabilities": {
                "devices.list": true, "devices.scan": true,
                "adb.list": true, "adb.scan": true
            }}));
            assert_eq!(mock.requests().len(), before + 1);
        }
        assert!(mock.requests().iter().all(|r| r["params"][1] == "session" && r["params"][2] == "login"));
    }

    #[tokio::test]
    async fn missing_login_acls_fail_check_without_blocking_the_normal_scan_rpc() {
        let mock = Mock::start_http();
        mock.update_mode(json!({"login_result": {"ubus_rpc_session": "a".repeat(32)}}));
        let client = LuciClient::new(&mock.url, "").unwrap();
        assert_eq!(client.check("  exact 密码  ").await.unwrap_err().code, "discovery_acl_invalid");
        assert_eq!(mock.requests().len(), 1);
        assert!(current_adb(&client.scan("  exact 密码  ", true).await.unwrap()));
        let methods: Vec<_> = mock.requests().into_iter().map(|r| r["params"][2].clone()).collect();
        assert_eq!(methods, vec![json!("login"), json!("login"), json!("scan")]);
    }

    #[test]
    fn malformed_rpc_responses_are_distinct_from_numeric_ubus_failures() {
        for response in [
            Value::Null,
            json!({"jsonrpc": "1.0", "id": 1, "result": [0, {}]}),
            json!({"jsonrpc": "2.0", "id": 2, "result": [0, {}]}),
            json!({"jsonrpc": "2.0", "id": 1, "error": null}),
            json!({"jsonrpc": "2.0", "id": 1, "error": {"code": "-32002", "message": "remote"}}),
            json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32002, "message": 7}}),
            json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32002, "message": "remote"}, "result": [0, {}]}),
            json!({"jsonrpc": "2.0", "id": 1, "result": []}),
            json!({"jsonrpc": "2.0", "id": 1, "result": [false, {}]}),
            json!({"jsonrpc": "2.0", "id": 1, "result": [0]}),
            json!({"jsonrpc": "2.0", "id": 1, "result": [0, []]}),
        ] {
            let error = rpc_result(response, "session", "login").unwrap_err();
            assert_eq!(error.code, "discovery_rpc_invalid");
            assert!(error.message.contains("session.login"));
        }
        for (object, method, status, code) in [
            ("session", "login", 6, "discovery_login_failed"),
            ("easy-discovery.adb", "scan", 6, "discovery_permission_denied"),
            ("easy-discovery.devices", "scan", 3, "discovery_ubus_error"),
        ] {
            let error = rpc_result(json!({"jsonrpc": "2.0", "id": 1, "result": [status]}), object, method).unwrap_err();
            assert_eq!(error.code, code);
            assert!(error.message.contains(&format!("{object}.{method}")));
            assert!(error.message.contains(&format!("ubus status {status}")));
        }
        for (rpc_code, expected) in [
            (-32000, "discovery_rpc_object_not_found"),
            (-32001, "discovery_session_not_found"),
            (-32002, "discovery_permission_denied"),
            (-32003, "discovery_timeout"),
            (-32601, "discovery_rpc_method_not_found"),
            (-32603, "discovery_rpc_error"),
        ] {
            let error = rpc_result(json!({"jsonrpc": "2.0", "id": 1, "error": {
                "code": rpc_code, "message": "upstream-private-details"
            }}), "session", "login").unwrap_err();
            assert_eq!(error.code, expected);
            assert!(error.message.contains("session.login"));
            assert!(error.message.contains(&rpc_code.to_string()));
            assert!(!format!("{error:?}").contains("upstream-private-details"));
        }
    }

    #[tokio::test]
    async fn rpc_and_http_errors_expose_only_the_stage_and_numeric_code() {
        let mock = Mock::start_http();
        let client = LuciClient::new(&mock.url, "").unwrap();
        let session = "a".repeat(32);
        let upstream_message = format!("  exact 密码   {session} upstream-private-details");
        let remote_error = json!({"jsonrpc": "2.0", "id": 1, "error": {
            "code": -32002, "message": upstream_message,
            "data": {"password": "  exact 密码  ", "session": session}
        }});
        mock.update_mode(json!({"responses": {"session.login": remote_error}}));
        let error = client.check("  exact 密码  ").await.unwrap_err();
        assert_eq!(error.code, "discovery_permission_denied");
        assert!(error.message.contains("session.login"));
        assert!(error.message.contains("-32002"));
        assert!(error.message.contains("Access denied"));
        for secret in ["exact 密码", session.as_str(), "upstream-private-details"] {
            assert!(!format!("{error:?}").contains(secret));
        }

        mock.update_mode(json!({"responses": {"easy-discovery.adb.scan": remote_error}}));
        let error = client.scan("  exact 密码  ", true).await.unwrap_err();
        assert_eq!(error.code, "discovery_permission_denied");
        assert!(error.message.contains("easy-discovery.adb.scan"));
        assert!(error.message.contains("-32002"));
        for secret in ["exact 密码", session.as_str(), "upstream-private-details"] {
            assert!(!format!("{error:?}").contains(secret));
        }

        mock.update_mode(json!({"responses": {}, "http_status": 403}));
        let error = client.check("  exact 密码  ").await.unwrap_err();
        assert_eq!(error.code, "discovery_http_error");
        assert!(error.message.contains("session.login"));
        assert!(error.message.contains("403"));
    }
}

