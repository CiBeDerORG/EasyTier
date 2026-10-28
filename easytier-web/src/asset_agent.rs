//! Manual, tenant-scoped EasyHostAgent binding to an existing asset_device.
//! No timers, retries, implicit IP discovery, public URLs or host duplication.
use crate::{
    asset_agent_tls::{Fingerprint, pinned_client},
    db::Db,
    deploy_agent::Service as CredentialStore,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::{net::IpAddr, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressChoice {
    Overlay,
    Lan,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeInput {
    pub address: AddressChoice,
    pub trusted_spki_sha256: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairInput {
    pub address: AddressChoice,
    pub trusted_spki_sha256: String,
    pub pairing_code: String,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error("Agent unavailable or its pinned TLS identity could not be verified")]
    Remote,
    #[error("Agent identity changed; no binding was modified")]
    IdentityChanged,
    #[error("Agent rejected the pairing code; check expiry and remaining attempts on the host")]
    PairRejected,
    #[error("Local binding persistence failed; remote pairing outcome may require manual recovery")]
    Storage,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptionIntentInput {
    pub resource_id: String,
    pub application_id: String,
    pub expected_container_ids: Vec<String>,
    pub confirm: bool,
}

pub struct Manager {
    db: Db,
    credentials: Arc<CredentialStore>,
    // Pairing is infrequent. Serialize binding mutations to prevent parallel
    // one-time code consumption/re-pairs targeting the same Agent identity.
    pair_lock: Mutex<()>,
}
impl Manager {
    pub fn new(db: Db, credentials: Arc<CredentialStore>) -> Self {
        Self {
            db,
            credentials,
            pair_lock: Mutex::new(()),
        }
    }
    fn address(device: &Value, kind: AddressChoice) -> Result<String, Error> {
        let raw = match kind {
            AddressChoice::Overlay => device["overlay_ip"].as_str(),
            AddressChoice::Lan => device["lan_ip"].as_str(),
        }
        .filter(|value| !value.trim().is_empty())
        .ok_or(Error::Invalid("Selected device IP is missing"))?;
        let ip: IpAddr = raw
            .parse()
            .map_err(|_| Error::Invalid("Device address must be a literal IP"))?;
        if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
            return Err(Error::Invalid(
                "Loopback, unspecified and multicast targets are not allowed",
            ));
        }
        if let IpAddr::V4(v4) = ip {
            if v4.is_link_local() || v4.is_broadcast() {
                return Err(Error::Invalid(
                    "Link-local and broadcast targets are prohibited",
                ));
            }
        }
        if let IpAddr::V6(v6) = ip {
            if v6.is_unicast_link_local() {
                return Err(Error::Invalid("IPv6 link-local targets are prohibited"));
            }
        }
        Ok(format!("https://{}", std::net::SocketAddr::new(ip, 7014)))
    }

    async fn saved(&self, id: &str) -> Result<Option<(String, String, String)>, Error> {
        let row = sqlx::query("SELECT agent_id, spki_sha256, endpoint FROM asset_agent_bindings WHERE asset_device_id=?")
            .bind(id).fetch_optional(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        row.map(|r| {
            Ok((
                r.try_get("agent_id").map_err(|_| Error::Storage)?,
                r.try_get("spki_sha256").map_err(|_| Error::Storage)?,
                r.try_get("endpoint").map_err(|_| Error::Storage)?,
            ))
        })
        .transpose()
    }

    async fn check(
        &self,
        device: &Value,
        address: AddressChoice,
        expected: &str,
    ) -> Result<(String, Value, Fingerprint), Error> {
        let pin = Fingerprint::parse(expected).map_err(Error::Invalid)?;
        let url = Self::address(device, address)?;
        // This verifier checks the certificate's SPKI within every handshake.
        // Redirects and HTTP proxy use are disabled; no bearer tokens here.
        let client = pinned_client(&pin).map_err(|_| Error::Remote)?;
        let response = client
            .get(format!("{url}/v1/identity"))
            .send()
            .await
            .map_err(|_| Error::Remote)?;
        if !response.status().is_success() {
            return Err(Error::Remote);
        }
        let response_id = response
            .headers()
            .get("X-EHA-Agent-ID")
            .and_then(|h| h.to_str().ok())
            .ok_or(Error::Remote)?
            .to_string();
        let body = response.bytes().await.map_err(|_| Error::Remote)?;
        if body.len() > 4096 {
            return Err(Error::Remote);
        }
        let identity: Value = serde_json::from_slice(&body).map_err(|_| Error::Remote)?;
        let agent_id = identity["agent_id"].as_str().ok_or(Error::Remote)?;
        Uuid::parse_str(agent_id).map_err(|_| Error::Remote)?;
        if agent_id != response_id
            || identity["tls_spki_sha256"].as_str() != Some(pin.hex().as_str())
            || identity["protocol_version"].as_u64() != Some(1)
        {
            return Err(Error::IdentityChanged);
        }
        if let Some((bound_id, bound_pin, _)) = self
            .saved(device["id"].as_str().unwrap_or_default())
            .await?
        {
            if bound_id != agent_id || bound_pin != pin.hex() {
                return Err(Error::IdentityChanged);
            }
        }
        Ok((url, identity, pin))
    }

    async fn verify_saved_authorization(
        &self,
        device: &Value,
        endpoint: &str,
        pin: &Fingerprint,
        agent_id: &str,
    ) -> Result<(), Error> {
        let id = device["id"].as_str().ok_or(Error::Storage)?;
        let owner = device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let (stored_id, _, _) = self.saved(id).await?.ok_or(Error::IdentityChanged)?;
        if stored_id != agent_id {
            return Err(Error::IdentityChanged);
        }
        let cipher: String = sqlx::query_scalar(
            "SELECT token_cipher FROM asset_agent_bindings WHERE asset_device_id=? AND owner_user_id=?"
        ).bind(id).bind(owner).fetch_one(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        let plain = self
            .credentials
            .open(&format!("asset-agent:{owner}:{id}"), &cipher)
            .map_err(|_| Error::Storage)?;
        let token = String::from_utf8(plain).map_err(|_| Error::Storage)?;
        let controller = self.controller_id().await?;
        let client = pinned_client(pin).map_err(|_| Error::Remote)?;
        let response = client
            .get(format!("{endpoint}/v1/pairings/current"))
            .header("X-EHA-Controller-ID", &controller)
            .header("X-EHA-Agent-ID", agent_id)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| Error::Remote)?;
        if !response.status().is_success() {
            return Err(Error::PairRejected);
        }
        let bytes = response.bytes().await.map_err(|_| Error::Remote)?;
        if bytes.len() > 4096 {
            return Err(Error::Remote);
        }
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Remote)?;
        if body["authorized"] != true
            || body["agent_id"].as_str() != Some(agent_id)
            || body["controller_id"].as_str() != Some(controller.as_str())
        {
            return Err(Error::IdentityChanged);
        }
        Ok(())
    }

    pub async fn probe(&self, device: &Value, input: ProbeInput) -> Result<Value, Error> {
        let (endpoint, identity, pin) = self
            .check(device, input.address, &input.trusted_spki_sha256)
            .await?;
        let existing = self
            .saved(device["id"].as_str().unwrap_or_default())
            .await?;
        // When a paired device's IP changes, only this manual verified probe
        // may update the endpoint. A failed probe preserves the old endpoint.
        if existing.is_some() {
            // A pinned identity is necessary but not sufficient: verify the stored
            // bearer still belongs to this controller before accepting a new IP.
            self.verify_saved_authorization(
                device,
                &endpoint,
                &pin,
                identity["agent_id"].as_str().ok_or(Error::Remote)?,
            )
            .await?;
            sqlx::query("UPDATE asset_agent_bindings SET endpoint=?,verified_at=? WHERE asset_device_id=? AND agent_id=? AND spki_sha256=?")
                .bind(&endpoint).bind(chrono::Utc::now().to_rfc3339())
                .bind(device["id"].as_str().unwrap_or_default()).bind(identity["agent_id"].as_str().unwrap_or_default())
                .bind(pin.hex()).execute(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        }
        Ok(
            json!({"identity":identity,"endpoint":endpoint,"bound":existing.is_some(),"tls_verified":true}),
        )
    }

    async fn controller_id(&self) -> Result<String, Error> {
        let candidate = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO asset_agent_controller(singleton,controller_id) VALUES(1,?) ON CONFLICT(singleton) DO NOTHING")
            .bind(&candidate).execute(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        sqlx::query_scalar::<_, String>(
            "SELECT controller_id FROM asset_agent_controller WHERE singleton=1",
        )
        .fetch_one(&self.db.inner())
        .await
        .map_err(|_| Error::Storage)
    }

    pub async fn pair(&self, device: &Value, input: PairInput) -> Result<Value, Error> {
        if input.pairing_code.len() != 32
            || !input.pairing_code.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(Error::Invalid(
                "Pairing code must contain 32 hexadecimal characters",
            ));
        }
        let _guard = self.pair_lock.lock().await;
        let id = device["id"]
            .as_str()
            .ok_or(Error::Invalid("Invalid saved asset device"))?;
        let (endpoint, identity, pin) = self
            .check(device, input.address, &input.trusted_spki_sha256)
            .await?;
        let agent = identity["agent_id"].as_str().ok_or(Error::Remote)?;
        let controller = self.controller_id().await?;
        // Conflicting Agent UUID on a different asset is never silently moved.
        let assigned: Option<String> =
            sqlx::query_scalar("SELECT asset_device_id FROM asset_agent_bindings WHERE agent_id=?")
                .bind(agent)
                .fetch_optional(&self.db.inner())
                .await
                .map_err(|_| Error::Storage)?;
        if assigned.as_deref().is_some_and(|other| other != id) {
            return Err(Error::Conflict(
                "Agent is already bound to a different asset device",
            ));
        }
        let client = pinned_client(&pin).map_err(|_| Error::Remote)?;
        let response = client
            .post(format!("{endpoint}/v1/pairings"))
            .json(&json!({
                "controller_id":controller,
                "device_id":id,
                "pairing_code":input.pairing_code,
            }))
            .send()
            .await
            .map_err(|_| Error::Remote)?;
        if response.status() != reqwest::StatusCode::CREATED {
            return Err(Error::PairRejected);
        }
        let response_id = response
            .headers()
            .get("X-EHA-Agent-ID")
            .and_then(|v| v.to_str().ok())
            .ok_or(Error::IdentityChanged)?;
        if response_id != agent {
            return Err(Error::IdentityChanged);
        }
        let body = response.bytes().await.map_err(|_| Error::Remote)?;
        if body.len() > 4096 {
            return Err(Error::Remote);
        }
        let result: Value = serde_json::from_slice(&body).map_err(|_| Error::Remote)?;
        let token = result["access_token"].as_str().ok_or(Error::Remote)?;
        if token.len() != 64
            || !token.bytes().all(|c| c.is_ascii_hexdigit())
            || result["agent_id"].as_str() != Some(agent)
            || result["controller_id"].as_str() != Some(controller.as_str())
            || result["device_id"].as_str() != Some(id)
        {
            return Err(Error::IdentityChanged);
        }
        let owner = device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let node = device["machine_id"].as_str().ok_or(Error::Storage)?;
        let cipher = self
            .credentials
            .seal(&format!("asset-agent:{owner}:{id}"), token.as_bytes())
            .map_err(|_| Error::Storage)?;
        sqlx::query("INSERT INTO asset_agent_bindings(asset_device_id,owner_user_id,machine_id,agent_id,spki_sha256,endpoint,token_cipher,paired_at,verified_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(asset_device_id) DO UPDATE SET agent_id=excluded.agent_id,spki_sha256=excluded.spki_sha256,endpoint=excluded.endpoint,token_cipher=excluded.token_cipher,paired_at=excluded.paired_at,verified_at=excluded.verified_at")
            .bind(id).bind(owner).bind(node).bind(agent).bind(pin.hex())
            .bind(&endpoint).bind(cipher).bind(chrono::Utc::now().to_rfc3339())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        // Never expose the bearer token to the browser or logs.
        Ok(
            json!({"agent_id":agent,"endpoint":endpoint,"tls_spki_sha256":pin.hex(),"bound":true,"identity":identity}),
        )
    }

    // Host reads are requested by the user, never polled. A cached snapshot is
    // strictly historical and is never interpreted as current connectivity.
    async fn fetch_host(&self, device: &Value, update_root: Option<&str>) -> Result<Value, Error> {
        let device_id = device["id"].as_str().ok_or(Error::Storage)?;
        let owner = device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let row = sqlx::query(
            "SELECT agent_id,spki_sha256,endpoint,token_cipher FROM asset_agent_bindings WHERE asset_device_id=? AND owner_user_id=?"
        ).bind(device_id).bind(owner).fetch_optional(&self.db.inner()).await.map_err(|_|Error::Storage)?
            .ok_or(Error::Invalid("This server has no paired Agent"))?;
        let agent: String = row.try_get("agent_id").map_err(|_|Error::Storage)?;
        let spki: String = row.try_get("spki_sha256").map_err(|_|Error::Storage)?;
        let endpoint: String = row.try_get("endpoint").map_err(|_|Error::Storage)?;
        let cipher: String = row.try_get("token_cipher").map_err(|_|Error::Storage)?;
        let pin = Fingerprint::parse(&spki).map_err(|_|Error::Storage)?;
        let token = String::from_utf8(self.credentials.open(&format!("asset-agent:{owner}:{device_id}"),&cipher)
            .map_err(|_|Error::Storage)?).map_err(|_|Error::Storage)?;
        let controller = self.controller_id().await?;
        let client = pinned_client(&pin).map_err(|_|Error::Remote)?;
        let url = if update_root.is_some() {
            format!("{endpoint}/v1/host/settings")
        } else {
            format!("{endpoint}/v1/host")
        };
        let mut request = if let Some(root) = update_root {
            client.patch(url).json(&json!({"software_root":root}))
        } else {
            client.get(url)
        };
        request = request.header("X-EHA-Controller-ID",&controller)
            .header("X-EHA-Agent-ID",&agent).bearer_auth(token);
        let response = request.send().await.map_err(|_|Error::Remote)?;
        if response.status() == reqwest::StatusCode::CONFLICT {
            return Err(Error::Conflict("Managed applications exist; migrate and verify before changing SOFTWARE_ROOT"));
        }
        if response.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
            return Err(Error::Invalid("Invalid SOFTWARE_ROOT; use a real absolute directory without symlinks"));
        }
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::PairRejected);
        }
        if !response.status().is_success() {
            return Err(Error::Remote);
        }
        let response_agent = response.headers().get("X-EHA-Agent-ID")
            .and_then(|v|v.to_str().ok()).ok_or(Error::IdentityChanged)?;
        if response_agent != agent {
            return Err(Error::IdentityChanged);
        }
        let body = response.bytes().await.map_err(|_|Error::Remote)?;
        if body.len() > 32768 { return Err(Error::Remote); }
        let snapshot: Value = serde_json::from_slice(&body).map_err(|_|Error::Remote)?;
        if snapshot["agent_id"].as_str() != Some(agent.as_str()) ||
            snapshot["observed_at"].as_str().is_none() ||
            snapshot["software_root"].as_str().is_none() ||
            !snapshot["docker"].is_object() ||
            !snapshot["memory"].is_object() ||
            !snapshot["cpu"].is_object() ||
            !snapshot["disks"].is_array() {
            return Err(Error::IdentityChanged);
        }
        sqlx::query("INSERT INTO asset_agent_host_snapshots(asset_device_id,agent_id,snapshot_json,observed_at) VALUES(?,?,?,?) ON CONFLICT(asset_device_id) DO UPDATE SET agent_id=excluded.agent_id,snapshot_json=excluded.snapshot_json,observed_at=excluded.observed_at")
            .bind(device_id).bind(agent)
            .bind(snapshot.to_string())
            .bind(snapshot["observed_at"].as_str().ok_or(Error::Remote)?)
            .execute(&self.db.inner()).await.map_err(|_|Error::Storage)?;
        Ok(json!({"live_status":"online","fresh":true,"snapshot":snapshot}))
    }

    pub async fn host(&self, device: &Value) -> Result<Value, Error> {
        if self.saved(device["id"].as_str().unwrap_or_default()).await?.is_none() {
            return Err(Error::Invalid("This server has no paired Agent"));
        }
        match self.fetch_host(device, None).await {
            Ok(snapshot) => Ok(snapshot),
            Err(Error::Remote) => {
                let id = device["id"].as_str().ok_or(Error::Storage)?;
                let agent = self.saved(id).await?.ok_or(Error::Storage)?.0;
                let row: Option<(String,String)> = sqlx::query_as(
                    "SELECT agent_id,snapshot_json FROM asset_agent_host_snapshots WHERE asset_device_id=?"
                ).bind(id).fetch_optional(&self.db.inner()).await.map_err(|_|Error::Storage)?;
                let cached = row.and_then(|(cached_agent,raw)| {
                    if cached_agent != agent { return None; }
                    serde_json::from_str::<Value>(&raw).ok()
                });
                Ok(json!({"live_status":"offline","fresh":false,"snapshot":cached,
                    "message":"Agent is unavailable; showing last successful snapshot if any"}))
            }
            Err(error) => Err(error),
        }
    }
    pub async fn update_host_root(&self, device: &Value, root: String) -> Result<Value, Error> {
        if root.is_empty() || root.len() > 4096 {
            return Err(Error::Invalid("SOFTWARE_ROOT must be an absolute path"));
        }
        self.fetch_host(device, Some(&root)).await
    }

    // Docker discovery is on-demand only. An Agent-provided resource is never
    // auto-promoted to a managed Deploy app by its name/image/labels alone.
    async fn fetch_docker_inventory(&self, device: &Value) -> Result<Value, Error> {
        let device_id = device["id"].as_str().ok_or(Error::Storage)?;
        let owner = device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let row = sqlx::query(
            "SELECT agent_id,spki_sha256,endpoint,token_cipher FROM asset_agent_bindings WHERE asset_device_id=? AND owner_user_id=?"
        )
        .bind(device_id)
        .bind(owner)
        .fetch_optional(&self.db.inner())
        .await
        .map_err(|_| Error::Storage)?
        .ok_or(Error::Invalid("This server has no paired Agent"))?;
        let agent: String = row.try_get("agent_id").map_err(|_| Error::Storage)?;
        let spki: String = row.try_get("spki_sha256").map_err(|_| Error::Storage)?;
        let endpoint: String = row.try_get("endpoint").map_err(|_| Error::Storage)?;
        let cipher: String = row.try_get("token_cipher").map_err(|_| Error::Storage)?;
        let pin = Fingerprint::parse(&spki).map_err(|_| Error::Storage)?;
        let token = String::from_utf8(
            self.credentials
                .open(&format!("asset-agent:{owner}:{device_id}"), &cipher)
                .map_err(|_| Error::Storage)?,
        )
        .map_err(|_| Error::Storage)?;
        let controller = self.controller_id().await?;
        let client = pinned_client(&pin).map_err(|_| Error::Remote)?;
        let response = client
            .get(format!("{endpoint}/v1/docker/inventory"))
            .header("X-EHA-Controller-ID", &controller)
            .header("X-EHA-Agent-ID", &agent)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| Error::Remote)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::PairRejected);
        }
        if !response.status().is_success() {
            return Err(Error::Remote);
        }
        if response
            .headers()
            .get("X-EHA-Agent-ID")
            .and_then(|v| v.to_str().ok())
            != Some(agent.as_str())
        {
            return Err(Error::IdentityChanged);
        }
        let bytes = response.bytes().await.map_err(|_| Error::Remote)?;
        if bytes.len() > 2_097_152 {
            return Err(Error::Remote);
        }
        let inventory: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Remote)?;
        if inventory["agent_id"].as_str() != Some(agent.as_str())
            || inventory["complete"].as_bool().is_none()
            || inventory["resources"].as_array().is_none()
            || inventory["observed_at"].as_str().is_none()
        {
            return Err(Error::IdentityChanged);
        }
        Ok(inventory)
    }

    // Historical inventory is never authoritative: no adoption, deletion or
    // Docker mutation may use cached resource membership.
    async fn cached_docker_inventory(&self, device_id: &str, agent: &str) -> Result<Option<Value>, Error> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT agent_id,snapshot_json FROM asset_agent_docker_snapshots WHERE asset_device_id=?"
        )
        .bind(device_id)
        .fetch_optional(&self.db.inner())
        .await
        .map_err(|_| Error::Storage)?;
        Ok(row.and_then(|(cached_agent, raw)| {
            if cached_agent != agent {
                return None;
            }
            serde_json::from_str::<Value>(&raw).ok()
        }))
    }

    pub async fn docker_inventory(&self, device: &Value) -> Result<Value, Error> {
        let device_id = device["id"].as_str().ok_or(Error::Storage)?;
        let owner = device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let agent = self.saved(device_id).await?
            .ok_or(Error::Invalid("This server has no paired Agent"))?.0;
        let fetched = self.fetch_docker_inventory(device).await;
        let mut inventory = match fetched {
            Ok(inventory) => inventory,
            Err(Error::Remote) => {
                let cached = self.cached_docker_inventory(device_id, &agent).await?;
                return Ok(json!({
                    "agent_id":agent, "live_status":"offline", "fresh":false,
                    "complete":false, "status":"error", "resources":[],
                    "snapshot":cached, "message":"Agent unavailable; showing historical inventory only"
                }));
            }
            Err(error) => return Err(error),
        };
        if inventory["complete"] != true || inventory["status"] != "ok" {
            let cached = self.cached_docker_inventory(device_id, &agent).await?;
            inventory["live_status"] = json!("degraded");
            inventory["fresh"] = json!(false);
            inventory["snapshot"] = json!(cached);
            inventory["resources"] = json!([]);
            inventory["message"] = json!("Docker scan incomplete; historical inventory is read-only");
            return Ok(inventory);
        }
        if inventory["agent_id"].as_str() != Some(agent.as_str()) {
            return Err(Error::IdentityChanged);
        }
        let rows = sqlx::query(
            "SELECT resource_id,application_id,container_ids_json FROM asset_agent_adoption_intents WHERE asset_device_id=? AND owner_user_id=? AND agent_id=?"
        )
        .bind(device_id)
        .bind(owner)
        .bind(&agent)
        .fetch_all(&self.db.inner())
        .await
        .map_err(|_| Error::Storage)?;
        let mut intentions = std::collections::HashMap::new();
        for row in rows {
            let resource: String = row.try_get("resource_id").map_err(|_| Error::Storage)?;
            let app: String = row.try_get("application_id").map_err(|_| Error::Storage)?;
            let ids: String = row.try_get("container_ids_json").map_err(|_| Error::Storage)?;
            intentions.insert(resource, (app, ids));
        }
        if let Some(resources) = inventory["resources"].as_array_mut() {
            for resource in resources {
                if !resource.is_object() || resource["resource_id"].as_str().is_none()
                    || resource["containers"].as_array().is_none()
                {
                    return Err(Error::Remote);
                }
                resource["management_state"] = json!("external");
                resource["managed"] = json!(false);
                if let Some(id) = resource["resource_id"].as_str() {
                    if let Some((app, stored_ids)) = intentions.get(id) {
                        let ids = Self::container_ids(resource).ok();
                        let expected: Option<Vec<String>> = serde_json::from_str(stored_ids).ok();
                        let valid = ids.as_ref().zip(expected.as_ref()).is_some_and(|(found, wanted)| found == wanted);
                        resource["management_state"] = if valid {
                            json!("adoption_pending_catalog_validation")
                        } else {
                            json!("adoption_needs_revalidation")
                        };
                        resource["proposed_application_id"] = json!(app);
                    }
                }
            }
        }
        // Commit the last complete snapshot only after current Agent identity
        // and inventory structure are verified. Never overwrite it on failure.
        let stored = inventory.clone();
        sqlx::query(
            "INSERT INTO asset_agent_docker_snapshots(asset_device_id,agent_id,snapshot_json,observed_at) VALUES(?,?,?,?) ON CONFLICT(asset_device_id) DO UPDATE SET agent_id=excluded.agent_id,snapshot_json=excluded.snapshot_json,observed_at=excluded.observed_at"
        )
        .bind(device_id)
        .bind(&agent)
        .bind(stored.to_string())
        .bind(stored["observed_at"].as_str().ok_or(Error::Remote)?)
        .execute(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        inventory["live_status"] = json!("online");
        inventory["fresh"] = json!(true);
        inventory["snapshot"] = json!(stored);
        Ok(inventory)
    }

    fn container_ids(resource: &Value) -> Result<Vec<String>, Error> {
        let list = resource["containers"]
            .as_array()
            .ok_or(Error::Invalid("Docker resource has no containers"))?;
        if list.is_empty() || list.len() > 100 {
            return Err(Error::Invalid("Invalid Docker resource size"));
        }
        let mut ids = list
            .iter()
            .map(|item| item["container_id"].as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::Invalid("Docker resource ID is missing"))?;
        if ids.iter().any(|x| x.is_empty() || x.len() > 128) {
            return Err(Error::Invalid("Invalid Docker container ID"));
        }
        ids.sort();
        ids.dedup();
        if ids.len() != list.len() {
            return Err(Error::Invalid("Duplicate Docker container IDs"));
        }
        Ok(ids)
    }

    fn validate_adoption_intent(input: &AdoptionIntentInput) -> Result<Vec<String>, Error> {
        if !input.confirm {
            return Err(Error::Invalid("Explicit confirmation is required"));
        }
        if input.resource_id.is_empty()
            || input.resource_id.len() > 256
            || !(input.resource_id.starts_with("compose:")
                || input.resource_id.starts_with("container:"))
        {
            return Err(Error::Invalid("Invalid resource identifier"));
        }
        if input.application_id.len() < 2
            || input.application_id.len() > 100
            || !input
                .application_id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_.".contains(&c))
        {
            return Err(Error::Invalid("Invalid Deploy application identifier"));
        }
        let mut expected = input.expected_container_ids.clone();
        if expected.is_empty() || expected.len() > 100 {
            return Err(Error::Invalid("Expected container IDs are required"));
        }
        expected.sort();
        expected.dedup();
        if expected.len() != input.expected_container_ids.len() {
            return Err(Error::Invalid("Duplicate expected container IDs"));
        }
        Ok(expected)
    }

    // An explicit intent does not change Docker resources or mark them Deploy-
    // managed. M3 catalog validation + M4 durable Agent ownership must run
    // before privileges can be elevated to "managed".
    pub async fn confirm_adoption_intent(&self, device: &Value, input: AdoptionIntentInput) -> Result<Value, Error> {
        let expected = Self::validate_adoption_intent(&input)?;
        let _guard = self.pair_lock.lock().await;
        let inventory = self.fetch_docker_inventory(device).await?;
        if inventory["complete"] != true || inventory["status"] != "ok" {
            return Err(Error::Remote);
        }
        let resource = inventory["resources"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["resource_id"].as_str() == Some(input.resource_id.as_str())))
            .ok_or(Error::Conflict("Resource is absent from a complete Docker inventory"))?;
        if resource["managed"] == true {
            return Err(Error::Conflict("Resource is already managed by the Agent"));
        }
        let current = Self::container_ids(resource)?;
        if current != expected {
            return Err(Error::Conflict("Docker resource members changed; refresh before confirming"));
        }
        let device_id = device["id"].as_str().ok_or(Error::Storage)?;
        let owner = device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let agent = inventory["agent_id"].as_str().ok_or(Error::Remote)?;
        let ids_json = serde_json::to_string(&current).map_err(|_| Error::Storage)?;
        let insert = sqlx::query(
            "INSERT INTO asset_agent_adoption_intents(asset_device_id,owner_user_id,agent_id,resource_id,application_id,container_ids_json,confirmed_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(asset_device_id,resource_id) DO NOTHING"
        )
        .bind(device_id).bind(owner).bind(agent).bind(&input.resource_id)
        .bind(&input.application_id).bind(&ids_json).bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.db.inner()).await;
        if insert.is_err() {
            return Err(Error::Conflict("This application is already linked to another resource on this host"));
        }
        let persisted: (String,String,String) = sqlx::query_as(
            "SELECT agent_id,application_id,container_ids_json FROM asset_agent_adoption_intents WHERE asset_device_id=? AND resource_id=? AND owner_user_id=?"
        )
        .bind(device_id).bind(&input.resource_id).bind(owner)
        .fetch_one(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        if persisted.0 != agent || persisted.1 != input.application_id || persisted.2 != ids_json {
            return Err(Error::Conflict("Resource already has a different adoption intent"));
        }
        Ok(json!({
            "resource_id":input.resource_id,
            "application_id":input.application_id,
            "agent_id":agent,
            "management_state":"adoption_pending_catalog_validation",
            "managed":false,
            "confirmed":true,
            "message":"Intent recorded; management rights remain disabled until trusted Deploy catalog validation and durable Agent registration"
        }))
    }


    // A user can cancel a pending intent without touching Docker containers,
    // volumes, services or the Agent's own managed-app registry.
    pub async fn cancel_adoption_intent(&self, device: &Value, resource_id: String) -> Result<Value, Error> {
        if resource_id.len() > 256 || resource_id.is_empty() {
            return Err(Error::Invalid("Invalid resource identifier"));
        }
        let _guard = self.pair_lock.lock().await;
        let device_id = device["id"].as_str().ok_or(Error::Storage)?;
        let owner = device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let agent = self.saved(device_id).await?
            .ok_or(Error::Invalid("This server has no paired Agent"))?.0;
        let deleted = sqlx::query(
            "DELETE FROM asset_agent_adoption_intents WHERE asset_device_id=? AND owner_user_id=? AND agent_id=? AND resource_id=?"
        )
        .bind(device_id).bind(owner).bind(agent).bind(&resource_id)
        .execute(&self.db.inner()).await.map_err(|_|Error::Storage)?;
        Ok(json!({"resource_id":resource_id,"removed":deleted.rows_affected()>0,
            "docker_modified":false}))
    }

    pub async fn status(&self, device: &Value) -> Result<Value, Error> {
        let id = device["id"].as_str().ok_or(Error::Storage)?;
        let row=sqlx::query("SELECT agent_id,spki_sha256,endpoint,paired_at,verified_at FROM asset_agent_bindings WHERE asset_device_id=?")
            .bind(id).fetch_optional(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        Ok(if let Some(r) = row {
            json!({"bound":true,"agent_id":r.try_get::<String,_>("agent_id").map_err(|_|Error::Storage)?,
                "tls_spki_sha256":r.try_get::<String,_>("spki_sha256").map_err(|_|Error::Storage)?,
                "endpoint":r.try_get::<String,_>("endpoint").map_err(|_|Error::Storage)?,
                "paired_at":r.try_get::<String,_>("paired_at").map_err(|_|Error::Storage)?,
                "verified_at":r.try_get::<String,_>("verified_at").map_err(|_|Error::Storage)?,
                "live_status":"not_checked"})
        } else {
            json!({"bound":false,"live_status":"not_checked"})
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adoption_intent_rejects_unconfirmed_and_ambiguous_requests() {
        let mut input = AdoptionIntentInput {
            resource_id: "compose:sub2api".into(),
            application_id: "sub2api".into(),
            expected_container_ids: vec!["b-db".into(), "a-api".into()],
            confirm: false,
        };
        assert!(Manager::validate_adoption_intent(&input).is_err());
        input.confirm = true;
        assert_eq!(
            Manager::validate_adoption_intent(&input).unwrap(),
            vec!["a-api".to_string(), "b-db".to_string()],
        );
        input.expected_container_ids.push("b-db".into());
        assert!(Manager::validate_adoption_intent(&input).is_err());
        input.expected_container_ids.pop();
        input.resource_id = "https://127.0.0.1:2375".into();
        assert!(Manager::validate_adoption_intent(&input).is_err());
        input.resource_id = "compose:sub2api".into();
        input.application_id = "../../bad".into();
        assert!(Manager::validate_adoption_intent(&input).is_err());
    }

    #[test]
    fn adoption_resource_membership_requires_stable_nonempty_ids() {
        let resource = json!({"containers":[
            {"container_id":"web"},{"container_id":"db"}
        ]});
        assert_eq!(Manager::container_ids(&resource).unwrap(), vec!["db", "web"]);
        assert!(Manager::container_ids(&json!({"containers":[]})).is_err());
        assert!(Manager::container_ids(&json!({"containers":[
            {"container_id":"web"},{"container_id":"web"}]})).is_err());
    }

    #[test]
    fn endpoints_accept_only_safe_ip_literals() {
        let device = json!({"overlay_ip":"10.0.0.5","lan_ip":"192.168.1.7"});
        assert_eq!(
            Manager::address(&device, AddressChoice::Overlay).unwrap(),
            "https://10.0.0.5:7014"
        );
        for addr in [
            "127.0.0.1",
            "0.0.0.0",
            "169.254.1.1",
            "example.org",
            "192.168.1.8:9090",
            "224.1.1.1",
        ] {
            let bad = json!({"overlay_ip":addr});
            assert!(
                Manager::address(&bad, AddressChoice::Overlay).is_err(),
                "{addr}"
            );
        }
    }
}
