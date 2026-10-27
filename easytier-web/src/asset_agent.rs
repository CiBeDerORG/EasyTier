//! Manual, tenant-scoped EasyHostAgent binding to an existing asset_device.
//! No timers, retries, implicit IP discovery, public URLs or host duplication.
use crate::{asset_agent_tls::{Fingerprint, pinned_client}, db::Db, deploy_agent::Service as CredentialStore};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::{net::IpAddr, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressChoice { Overlay, Lan }
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
    #[error("{0}")] Invalid(&'static str),
    #[error("{0}")] Conflict(&'static str),
    #[error("Agent unavailable or its pinned TLS identity could not be verified")]
    Remote,
    #[error("Agent identity changed; no binding was modified")]
    IdentityChanged,
    #[error("Agent rejected the pairing code; check expiry and remaining attempts on the host")]
    PairRejected,
    #[error("Local binding persistence failed; remote pairing outcome may require manual recovery")]
    Storage,
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
        Self { db, credentials, pair_lock: Mutex::new(()) }
    }
    fn address(device: &Value, kind: AddressChoice) -> Result<String, Error> {
        let raw = match kind {
            AddressChoice::Overlay => device["overlay_ip"].as_str(),
            AddressChoice::Lan => device["lan_ip"].as_str(),
        }.filter(|value| !value.trim().is_empty()).ok_or(Error::Invalid("Selected device IP is missing"))?;
        let ip: IpAddr = raw.parse().map_err(|_| Error::Invalid("Device address must be a literal IP"))?;
        if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
            return Err(Error::Invalid("Loopback, unspecified and multicast targets are not allowed"));
        }
        if let IpAddr::V4(v4) = ip {
            if v4.is_link_local() || v4.is_broadcast() {
                return Err(Error::Invalid("Link-local and broadcast targets are prohibited"));
            }
        }
        if let IpAddr::V6(v6) = ip {
            if v6.is_unicast_link_local() {return Err(Error::Invalid("IPv6 link-local targets are prohibited"));}
        }
        Ok(format!("https://{}", std::net::SocketAddr::new(ip, 7014)))
    }

    async fn saved(&self, id: &str) -> Result<Option<(String, String, String)>, Error> {
        let row = sqlx::query("SELECT agent_id, spki_sha256, endpoint FROM asset_agent_bindings WHERE asset_device_id=?")
            .bind(id).fetch_optional(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        row.map(|r| Ok((r.try_get("agent_id").map_err(|_| Error::Storage)?,
            r.try_get("spki_sha256").map_err(|_| Error::Storage)?,
            r.try_get("endpoint").map_err(|_| Error::Storage)?))).transpose()
    }

    async fn check(&self, device: &Value, address: AddressChoice, expected: &str) -> Result<(String, Value, Fingerprint), Error> {
        let pin = Fingerprint::parse(expected).map_err(Error::Invalid)?;
        let url = Self::address(device, address)?;
        // This verifier checks the certificate's SPKI within every handshake.
        // Redirects and HTTP proxy use are disabled; no bearer tokens here.
        let client = pinned_client(&pin).map_err(|_| Error::Remote)?;
        let response = client.get(format!("{url}/v1/identity")).send().await.map_err(|_| Error::Remote)?;
        if !response.status().is_success() { return Err(Error::Remote); }
        let response_id = response.headers().get("X-EHA-Agent-ID")
            .and_then(|h| h.to_str().ok()).ok_or(Error::Remote)?.to_string();
        let body = response.bytes().await.map_err(|_| Error::Remote)?;
        if body.len() > 4096 { return Err(Error::Remote); }
        let identity: Value = serde_json::from_slice(&body).map_err(|_| Error::Remote)?;
        let agent_id = identity["agent_id"].as_str().ok_or(Error::Remote)?;
        Uuid::parse_str(agent_id).map_err(|_| Error::Remote)?;
        if agent_id != response_id || identity["tls_spki_sha256"].as_str() != Some(pin.hex().as_str()) ||
            identity["protocol_version"].as_u64() != Some(1) {
            return Err(Error::IdentityChanged);
        }
        if let Some((bound_id, bound_pin, _)) = self.saved(device["id"].as_str().unwrap_or_default()).await? {
            if bound_id != agent_id || bound_pin != pin.hex() {
                return Err(Error::IdentityChanged);
            }
        }
        Ok((url,identity,pin))
    }

    async fn verify_saved_authorization(&self, device: &Value, endpoint: &str, pin: &Fingerprint, agent_id: &str) -> Result<(), Error> {
        let id = device["id"].as_str().ok_or(Error::Storage)?;
        let owner = device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let (stored_id, _, _) = self.saved(id).await?.ok_or(Error::IdentityChanged)?;
        if stored_id != agent_id { return Err(Error::IdentityChanged); }
        let cipher: String = sqlx::query_scalar(
            "SELECT token_cipher FROM asset_agent_bindings WHERE asset_device_id=? AND owner_user_id=?"
        ).bind(id).bind(owner).fetch_one(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        let plain = self.credentials.open(&format!("asset-agent:{owner}:{id}"), &cipher)
            .map_err(|_| Error::Storage)?;
        let token = String::from_utf8(plain).map_err(|_| Error::Storage)?;
        let controller = self.controller_id().await?;
        let client = pinned_client(pin).map_err(|_| Error::Remote)?;
        let response = client.get(format!("{endpoint}/v1/pairings/current"))
            .header("X-EHA-Controller-ID", &controller)
            .header("X-EHA-Agent-ID", agent_id)
            .bearer_auth(token).send().await.map_err(|_| Error::Remote)?;
        if !response.status().is_success() { return Err(Error::PairRejected); }
        let bytes = response.bytes().await.map_err(|_| Error::Remote)?;
        if bytes.len()>4096 { return Err(Error::Remote); }
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Remote)?;
        if body["authorized"] != true ||
            body["agent_id"].as_str()!=Some(agent_id) ||
            body["controller_id"].as_str()!=Some(controller.as_str()) {
            return Err(Error::IdentityChanged);
        }
        Ok(())
    }

    pub async fn probe(&self, device: &Value, input: ProbeInput) -> Result<Value, Error> {
        let (endpoint, identity, pin) = self.check(device, input.address, &input.trusted_spki_sha256).await?;
        let existing = self.saved(device["id"].as_str().unwrap_or_default()).await?;
        // When a paired device's IP changes, only this manual verified probe
        // may update the endpoint. A failed probe preserves the old endpoint.
        if existing.is_some() {
            // A pinned identity is necessary but not sufficient: verify the stored
            // bearer still belongs to this controller before accepting a new IP.
            self.verify_saved_authorization(device, &endpoint, &pin,
                identity["agent_id"].as_str().ok_or(Error::Remote)?).await?;
            sqlx::query("UPDATE asset_agent_bindings SET endpoint=?,verified_at=? WHERE asset_device_id=? AND agent_id=? AND spki_sha256=?")
                .bind(&endpoint).bind(chrono::Utc::now().to_rfc3339())
                .bind(device["id"].as_str().unwrap_or_default()).bind(identity["agent_id"].as_str().unwrap_or_default())
                .bind(pin.hex()).execute(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        }
        Ok(json!({"identity":identity,"endpoint":endpoint,"bound":existing.is_some(),"tls_verified":true}))
    }

    async fn controller_id(&self) -> Result<String, Error> {
        let candidate = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO asset_agent_controller(singleton,controller_id) VALUES(1,?) ON CONFLICT(singleton) DO NOTHING")
            .bind(&candidate).execute(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        sqlx::query_scalar::<_,String>("SELECT controller_id FROM asset_agent_controller WHERE singleton=1")
            .fetch_one(&self.db.inner()).await.map_err(|_| Error::Storage)
    }

    pub async fn pair(&self, device: &Value, input: PairInput) -> Result<Value, Error> {
        if input.pairing_code.len()!=32 || !input.pairing_code.bytes().all(|c| c.is_ascii_hexdigit()){
            return Err(Error::Invalid("Pairing code must contain 32 hexadecimal characters"));
        }
        let _guard = self.pair_lock.lock().await;
        let id = device["id"].as_str().ok_or(Error::Invalid("Invalid saved asset device"))?;
        let (endpoint, identity, pin) = self.check(device, input.address, &input.trusted_spki_sha256).await?;
        let agent = identity["agent_id"].as_str().ok_or(Error::Remote)?;
        let controller = self.controller_id().await?;
        // Conflicting Agent UUID on a different asset is never silently moved.
        let assigned: Option<String> = sqlx::query_scalar("SELECT asset_device_id FROM asset_agent_bindings WHERE agent_id=?")
            .bind(agent).fetch_optional(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        if assigned.as_deref().is_some_and(|other| other != id) {
            return Err(Error::Conflict("Agent is already bound to a different asset device"));
        }
        let client = pinned_client(&pin).map_err(|_| Error::Remote)?;
        let response = client.post(format!("{endpoint}/v1/pairings")).json(&json!({
            "controller_id":controller,
            "device_id":id,
            "pairing_code":input.pairing_code,
        })).send().await.map_err(|_| Error::Remote)?;
        if response.status() != reqwest::StatusCode::CREATED { return Err(Error::PairRejected); }
        let response_id = response.headers().get("X-EHA-Agent-ID")
            .and_then(|v| v.to_str().ok()).ok_or(Error::IdentityChanged)?;
        if response_id != agent { return Err(Error::IdentityChanged); }
        let body = response.bytes().await.map_err(|_| Error::Remote)?;
        if body.len()>4096 { return Err(Error::Remote); }
        let result:Value=serde_json::from_slice(&body).map_err(|_| Error::Remote)?;
        let token=result["access_token"].as_str().ok_or(Error::Remote)?;
        if token.len()!=64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) ||
            result["agent_id"].as_str()!=Some(agent) ||
            result["controller_id"].as_str()!=Some(controller.as_str()) ||
            result["device_id"].as_str()!=Some(id) {
            return Err(Error::IdentityChanged);
        }
        let owner=device["owner_user_id"].as_i64().ok_or(Error::Storage)?;
        let node=device["machine_id"].as_str().ok_or(Error::Storage)?;
        let cipher=self.credentials.seal(&format!("asset-agent:{owner}:{id}"),token.as_bytes())
            .map_err(|_| Error::Storage)?;
        sqlx::query("INSERT INTO asset_agent_bindings(asset_device_id,owner_user_id,machine_id,agent_id,spki_sha256,endpoint,token_cipher,paired_at,verified_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(asset_device_id) DO UPDATE SET agent_id=excluded.agent_id,spki_sha256=excluded.spki_sha256,endpoint=excluded.endpoint,token_cipher=excluded.token_cipher,paired_at=excluded.paired_at,verified_at=excluded.verified_at")
            .bind(id).bind(owner).bind(node).bind(agent).bind(pin.hex())
            .bind(&endpoint).bind(cipher).bind(chrono::Utc::now().to_rfc3339())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        // Never expose the bearer token to the browser or logs.
        Ok(json!({"agent_id":agent,"endpoint":endpoint,"tls_spki_sha256":pin.hex(),"bound":true,"identity":identity}))
    }
    pub async fn status(&self, device:&Value)->Result<Value,Error>{
        let id=device["id"].as_str().ok_or(Error::Storage)?;
        let row=sqlx::query("SELECT agent_id,spki_sha256,endpoint,paired_at,verified_at FROM asset_agent_bindings WHERE asset_device_id=?")
            .bind(id).fetch_optional(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        Ok(if let Some(r)=row {
            json!({"bound":true,"agent_id":r.try_get::<String,_>("agent_id").map_err(|_|Error::Storage)?,
                "tls_spki_sha256":r.try_get::<String,_>("spki_sha256").map_err(|_|Error::Storage)?,
                "endpoint":r.try_get::<String,_>("endpoint").map_err(|_|Error::Storage)?,
                "paired_at":r.try_get::<String,_>("paired_at").map_err(|_|Error::Storage)?,
                "verified_at":r.try_get::<String,_>("verified_at").map_err(|_|Error::Storage)?,
                "live_status":"not_checked"})
        }else{json!({"bound":false,"live_status":"not_checked"})})
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn endpoints_accept_only_safe_ip_literals() {
        let device=json!({"overlay_ip":"10.0.0.5","lan_ip":"192.168.1.7"});
        assert_eq!(Manager::address(&device,AddressChoice::Overlay).unwrap(),"https://10.0.0.5:7014");
        for addr in ["127.0.0.1","0.0.0.0","169.254.1.1","example.org","192.168.1.8:9090","224.1.1.1"] {
            let bad=json!({"overlay_ip":addr});
            assert!(Manager::address(&bad,AddressChoice::Overlay).is_err(),"{addr}");
        }
    }
}
