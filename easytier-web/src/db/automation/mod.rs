//! Automation business records. Phone side effects never run inside a DB transaction.
mod runs;
mod scripts;
mod drafts;
mod validation;
#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, net::Ipv4Addr};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool, sqlite::SqliteRow};
use uuid::Uuid;

use super::{Db, assets::{AssetsStore, RuntimeObservations}};
pub(crate) use super::assets::AssetError as AutomationError;
pub(crate) type AutomationResult<T> = Result<T, AutomationError>;
type ObservationsByNode = BTreeMap<(i32,String),RuntimeObservations>;

#[derive(Clone)]
pub(crate) struct AutomationStore {
    db: Db,
    pool: SqlitePool,
    runtime_observations: ObservationsByNode,
}

pub(super) fn now() -> String { chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true) }
pub(super) fn deadline() -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(60)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub(super) fn digest(value: &Value) -> String { format!("{:x}", Sha256::digest(value.to_string().as_bytes())) }
pub(super) fn not_found() -> AutomationError { AutomationError::new(404, "automation_not_found", "The automation resource does not exist in this account.") }
pub(super) fn conflict(code: &str, message: &str) -> AutomationError { AutomationError::new(409, code, message) }
pub(super) fn parse_json(raw: &str) -> AutomationResult<Value> {
    serde_json::from_str(raw).map_err(|_| AutomationError::new(500,"automation_record_invalid","The saved automation record is invalid."))
}

pub(super) async fn active_owner(connection: &mut SqliteConnection, owner: i32) -> AutomationResult<()> {
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=? AND deleted_at IS NULL)")
        .bind(owner).fetch_one(connection).await?;
    if !active { return Err(AutomationError::new(403,"account_inactive","The account is no longer active.")); }
    Ok(())
}

pub(super) async fn authorized(connection: &mut SqliteConnection, actor: i32, owner: i32) -> AutomationResult<()> {
    active_owner(connection, actor).await?;
    active_owner(connection, owner).await?;
    if actor != owner {
        let admin: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users_groups ug JOIN groups g ON g.id=ug.group_id WHERE ug.user_id=? AND g.name='admins')")
            .bind(actor).fetch_one(connection).await?;
        if !admin { return Err(AutomationError::new(403,"automation_access_denied","You cannot manage this account's automation.")); }
    }
    Ok(())
}

/// Hash persisted routing and connection inputs, never presentation names/timestamps.
/// Network configuration contents can contain credentials, so only their hash leaves this function.
pub(super) async fn fingerprint(connection: &mut SqliteConnection, owner: i32, id: &str, observations:&ObservationsByNode) -> AutomationResult<String> {
    let row = sqlx::query(
        "SELECT a.overlay_ip,a.machine_id,a.gateway_group_id,a.address_mode,a.rule_key,a.host_offset,a.network_instance_id,\
         a.binding_snapshot_json,g.binding_snapshot_json AS parent_snapshot,\
         c.id AS current_config_id,c.network_config,c.disabled,c.create_time AS config_created,\
         m.allocated_ipv4,ac.id AS adb_service_id,ac.port AS adb_port,ac.connection_mode,ac.endpoint_revision AS service_revision,a.endpoint_revision,t.semantic_kind,t.semantic_kinds \
         FROM asset_devices a LEFT JOIN asset_gateway_groups g ON g.id=a.gateway_group_id \
         LEFT JOIN user_running_network_configs c ON c.user_id=a.owner_user_id AND c.device_id=a.machine_id \
           AND c.network_instance_id=COALESCE(g.network_instance_id,a.network_instance_id) \
         LEFT JOIN network_members m ON m.user_id=a.owner_user_id AND m.device_id=a.machine_id AND m.network_id=c.network_instance_id \
         LEFT JOIN asset_device_types t ON t.id=a.device_type_id AND t.owner_user_id=a.owner_user_id LEFT JOIN asset_services ac ON ac.device_id=a.id AND ac.service_type='adb' WHERE a.owner_user_id=? AND a.id=?")
        .bind(owner).bind(id).fetch_optional(connection).await?.ok_or_else(not_found)?;
    let mut source = json!({"device_id":id,"owner_user_id":owner,"route_context":"default"});
    for name in ["overlay_ip","machine_id","gateway_group_id","address_mode","rule_key","network_instance_id","config_created","allocated_ipv4","connection_mode","adb_service_id","semantic_kind","semantic_kinds"] {
        source[name] = json!(row.try_get::<Option<String>,_>(name)?);
    }
    for name in ["binding_snapshot_json","parent_snapshot"] {
        let raw:Option<String>=row.try_get(name)?;
        let saved=raw.as_deref().and_then(|value|serde_json::from_str::<Value>(value).ok()).unwrap_or(Value::Null);
        let mut relevant=json!({});
        for field in ["instance_id","role","config_ref","config_fingerprint","ipv4","ipv4_source","rules"] {
            relevant[field]=saved[field].clone();
        }
        if saved["ipv4_source"]=="runtime" {
            let node:String=row.try_get("machine_id")?;
            let current=saved["instance_id"].as_str()
                .and_then(|instance|observations.get(&(owner,node)).and_then(|by_instance|by_instance.get(instance)))
                .and_then(|observation|observation.ipv4.as_deref())
                .map(|ip|ip.split_once('/').map_or(ip,|(ip,_)|ip));
            relevant["observed_ipv4"]=json!(current);
        }
        source[name]=relevant;
    }
    let raw:Option<String>=row.try_get("network_config")?;
    let config=raw.as_deref().and_then(|value|serde_json::from_str::<Value>(value).ok()).unwrap_or(Value::Null);
    let mut route_config=json!({});
    for field in ["virtual_ipv4","dhcp","dhcp_ipv4_cidr","network_length","proxy_cidrs","network_identity"] {
        route_config[field]=config[field].clone();
    }
    source["route_config"]=route_config;
    for name in ["host_offset","current_config_id","disabled","adb_port","endpoint_revision","service_revision"] {
        source[name] = json!(row.try_get::<Option<i64>,_>(name)?);
    }
    Ok(digest(&source))
}

pub(super) fn target_key(target: &Value) -> AutomationResult<String> {
    let ip = target["ip"].as_str().ok_or_else(not_found)?;
    let port = target["adb_port"].as_u64().ok_or_else(not_found)?;
    Ok(format!("default:{ip}:{port}"))
}

pub(super) async fn ensure_route_unambiguous(connection:&mut SqliteConnection,owner:i32,id:&str,ip:&str,port:i64)->AutomationResult<()> {
    // One shared route context cannot distinguish two tenants' equal addresses.
    // Disabling an asset must not transfer the executor's existing ADB trust.
    let collision:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices a JOIN asset_services c ON c.device_id=a.id AND c.service_type='adb' WHERE a.overlay_ip=? AND c.port=? AND a.id<>?)")
        .bind(ip).bind(port).bind(id).fetch_one(&mut *connection).await?;
    let endpoint=format!("default:{ip}:{port}");
    let unresolved:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_sessions WHERE target_key=? AND owner_user_id<>? AND state NOT IN ('closed','failed')) OR EXISTS(SELECT 1 FROM automation_run_devices d JOIN automation_runs r ON r.id=d.run_id WHERE d.target_key=? AND r.owner_user_id<>? AND d.cleanup_confirmed=0 AND d.state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending') AND (r.runner_identity IS NOT NULL OR (r.state='dispatch_pending' AND julianday(r.updated_at)>=julianday('now','-60 seconds'))))")
        .bind(&endpoint).bind(owner).bind(&endpoint).bind(owner).fetch_one(&mut *connection).await?;
    if collision||unresolved {return Err(conflict("route_ambiguous","This shared executor cannot distinguish duplicate network addresses. Resolve conflicting asset mappings or pending cleanup before connecting."));}
    Ok(())
}

fn session_json(row: &SqliteRow) -> AutomationResult<Value> {
    Ok(json!({
        "id":row.try_get::<String,_>("id")?,"actor_user_id":row.try_get::<i32,_>("actor_user_id")?,
        "owner_user_id":row.try_get::<i32,_>("owner_user_id")?,"device_id":row.try_get::<String,_>("device_id")?,
        "target":parse_json(&row.try_get::<String,_>("target_json")?)?,
        "quality":row.try_get::<String,_>("quality")?,"state":row.try_get::<String,_>("state")?,
        "created_at":row.try_get::<String,_>("created_at")?,"updated_at":row.try_get::<String,_>("updated_at")?,
        "expires_at":row.try_get::<String,_>("expires_at")?
    }))
}

impl AutomationStore {
    pub(crate) fn new(db: Db) -> Self { Self { pool:db.inner(), db, runtime_observations:BTreeMap::new() } }

    pub(crate) fn with_runtime_observations(mut self,owner:i32,node:&str,observations:RuntimeObservations)->Self {
        self.runtime_observations.insert((owner,node.to_owned()),observations);
        self
    }

    fn assets_for_node(&self,owner:i32,node:&str)->AssetsStore {
        AssetsStore::new(self.db.clone()).with_runtime_observations(
            self.runtime_observations.get(&(owner,node.to_owned())).cloned().unwrap_or_default())
    }

    pub(crate) async fn authorize(&self, actor: i32, owner: i32) -> AutomationResult<()> {
        let mut connection=self.pool.acquire().await?;
        authorized(&mut connection, actor, owner).await
    }

    pub(crate) async fn list_devices(&self, owner: i32) -> AutomationResult<Value> {
        let mut connection = self.pool.acquire().await?;
        active_owner(&mut connection,owner).await?;
        let rows = sqlx::query(
            "SELECT a.*,COALESCE(NULLIF(n.alias,''),NULLIF(n.hostname,''),a.machine_id) AS node_name,\
             g.name AS gateway_name,t.name AS type_name,1 AS enabled,c.id AS adb_service_id,\
             c.port AS adb_port,c.connection_mode \
             FROM asset_devices a LEFT JOIN devices n ON n.user_id=a.owner_user_id AND n.machine_id=a.machine_id \
             LEFT JOIN asset_gateway_groups g ON g.id=a.gateway_group_id \
             LEFT JOIN asset_device_types t ON t.id=a.device_type_id \
             JOIN asset_services c ON c.device_id=a.id AND c.service_type='adb' \
             WHERE a.owner_user_id=? AND EXISTS (SELECT 1 FROM json_each(t.semantic_kinds) WHERE value='phone') AND a.is_gateway_self=0 AND EXISTS(SELECT 1 FROM asset_service_bindings b WHERE b.service_id=c.id AND b.protocol='tcp' AND b.port=c.port) AND NOT EXISTS(SELECT 1 FROM asset_service_bindings b WHERE b.service_id=c.id AND b.protocol<>'tcp') ORDER BY (a.gateway_group_id IS NULL), COALESCE(g.name,'') COLLATE NOCASE, a.gateway_group_id, a.updated_at DESC, a.id DESC")
            .bind(owner).fetch_all(&mut *connection).await?;
        drop(connection);
        let mut items = Vec::with_capacity(rows.len());
        for row in rows { items.push(self.device_json(owner,&row).await?); }
        let issues = sqlx::query("SELECT a.id,a.machine_id,a.name,t.semantic_kind FROM asset_devices a LEFT JOIN asset_device_types t ON t.id=a.device_type_id WHERE a.owner_user_id=? AND a.is_gateway_self=0 AND (EXISTS (SELECT 1 FROM json_each(t.semantic_kinds) WHERE value='phone') OR t.id IS NULL OR t.semantic_kinds='[]') AND NOT EXISTS(SELECT 1 FROM asset_services s JOIN asset_service_bindings b ON b.service_id=s.id AND b.protocol='tcp' AND b.port=s.port WHERE s.device_id=a.id AND s.service_type='adb' AND EXISTS (SELECT 1 FROM json_each(t.semantic_kinds) WHERE value='phone')) ORDER BY a.name")
            .bind(owner).fetch_all(&self.pool).await?.iter().map(|row| -> AutomationResult<Value> {Ok(json!({"device_id":row.try_get::<String,_>("id")?,"machine_id":row.try_get::<String,_>("machine_id")?,"name":row.try_get::<String,_>("name")?,"reason":if row.try_get::<Option<String>,_>("semantic_kind")?.is_none(){"type_unclassified"}else{"adb_service_missing"}}))}).collect::<AutomationResult<Vec<_>>>()?;
        Ok(json!({"items":items,"issues":issues}))
    }

    async fn device_json(&self, owner: i32, row: &SqliteRow) -> AutomationResult<Value> {
        let id: String = row.try_get("id")?;
        let node: String = row.try_get("machine_id")?;
        let enabled: bool = row.try_get("enabled")?;
        let asset = self.assets_for_node(owner,&node).get_device(owner,&node,&id).await?;
        let binding = asset["binding"]["state"].as_str().unwrap_or("unavailable");
        let reason = if !enabled { Some("Configure and enable ADB for this device.") }
            else if asset["binding"]["current"]["disabled"].as_bool()==Some(true) { Some("The device's network instance is disabled.") }
            else if binding != "current" { Some("The current network binding or its observed IPv4 address is unavailable or changed. Review node details before connecting.") } else { None };
        let mut conn = self.pool.acquire().await?;
        let version = fingerprint(&mut conn,owner,&id,&self.runtime_observations).await?;
        let active_session=sqlx::query("SELECT id,state,actor_user_id,expires_at FROM automation_sessions WHERE owner_user_id=? AND device_id=? AND state NOT IN ('closed','failed') ORDER BY created_at DESC LIMIT 1")
            .bind(owner).bind(&id).fetch_optional(&mut *conn).await?
            .map(|row|->AutomationResult<Value>{Ok(json!({"id":row.try_get::<String,_>("id")?,"state":row.try_get::<String,_>("state")?,"actor_user_id":row.try_get::<i32,_>("actor_user_id")?,"expires_at":row.try_get::<String,_>("expires_at")?}))}).transpose()?;
        // Expose the same in-flight or unconfirmed-cleanup fence as create_run:
        // manual sessions alone do not reflect all occupied devices.
        let route_key = format!("default:{}:{}", asset["overlay_ip"].as_str().unwrap_or(""), row.try_get::<i64,_>("adb_port")?);
        let run_occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_run_devices WHERE (device_id=? OR target_key=?) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending'))")
            .bind(&id).bind(&route_key).fetch_one(&mut *conn).await?;
        let endpoint_event:Option<String>=sqlx::query_scalar("SELECT json_object('event_id',event_id,'endpoint_revision',endpoint_revision,'publication',json(publication_json),'cleanup',json(cleanup_json),'connection',json(connection_json)) FROM discovery_endpoint_events WHERE owner_user_id=? AND device_id=? ORDER BY seq DESC LIMIT 1").bind(owner).bind(&id).fetch_optional(&mut *conn).await?;
        Ok(json!({
            "endpoint_event":endpoint_event.map(|s|parse_json(&s)).transpose()?,
            "id":id,"name":row.try_get::<String,_>("name")?,"model":row.try_get::<Option<String>,_>("model")?,
            "updated_at":row.try_get::<String,_>("updated_at")?,
            "owner_user_id":owner,"machine_id":node,"node_name":row.try_get::<String,_>("node_name")?,
            "gateway_group_id":row.try_get::<Option<String>,_>("gateway_group_id")?,"gateway_name":row.try_get::<Option<String>,_>("gateway_name")?,
            "device_type_id":row.try_get::<Option<String>,_>("device_type_id")?,"type_name":row.try_get::<Option<String>,_>("type_name")?,
            "overlay_ip":asset["overlay_ip"],"lan_ip":asset["lan_ip"],"binding_state":binding,
            "adb_service_id":row.try_get::<String,_>("adb_service_id")?,"endpoint_revision":row.try_get::<i64,_>("endpoint_revision")?,
            "connection":{"enabled":enabled,"adb_port":row.try_get::<i64,_>("adb_port")?,"connection_mode":row.try_get::<String,_>("connection_mode")?},
            "target_fingerprint":version,"availability_reason":reason,"active_session":active_session,
            "run_occupied":run_occupied
        }))
    }

    pub(crate) async fn save_connection(&self, owner: i32, id: &str, _body: &Value) -> AutomationResult<Value> {
        self.target(owner,id).await?;
        Err(conflict("adb_service_authoritative","Edit the device's ADB service; independent automation ports are no longer accepted."))
    }

    pub(crate) async fn target(&self, owner:i32,id:&str)->AutomationResult<Value> {
        self.target_inner(owner,id,true).await
    }

    /// Explicit physical cleanup must be able to resolve this account's current
    /// target even when another account has a lease for the same ADB endpoint.
    /// The cleanup route fences and clears all matching endpoint holders first.
    pub(crate) async fn target_for_cleanup(&self, owner:i32,id:&str)->AutomationResult<Value> {
        self.target_inner(owner,id,false).await
    }

    async fn target_inner(&self, owner:i32,id:&str,enforce_route_unambiguous:bool)->AutomationResult<Value> {
        let mut connection=self.pool.acquire().await?;
        active_owner(&mut connection,owner).await?;
        let row=sqlx::query("SELECT a.name,a.machine_id,a.overlay_ip,a.endpoint_revision,s.id AS adb_service_id,s.port AS adb_port,s.connection_mode FROM asset_devices a JOIN asset_device_types t ON t.id=a.device_type_id AND t.owner_user_id=a.owner_user_id JOIN asset_services s ON s.device_id=a.id AND s.owner_user_id=a.owner_user_id AND s.service_type='adb' WHERE a.id=? AND a.owner_user_id=? AND EXISTS (SELECT 1 FROM json_each(t.semantic_kinds) WHERE value='phone') AND a.is_gateway_self=0 AND EXISTS(SELECT 1 FROM asset_service_bindings b WHERE b.service_id=s.id AND b.protocol='tcp' AND b.port=s.port) AND NOT EXISTS(SELECT 1 FROM asset_service_bindings b WHERE b.service_id=s.id AND b.protocol<>'tcp')")
            .bind(id).bind(owner).fetch_optional(&mut *connection).await?.ok_or_else(|| conflict("adb_service_missing","A phone with one confirmed TCP ADB service is required."))?;
        let cleanup:Option<String>=sqlx::query_scalar("SELECT json_extract(cleanup_json,'$.state') FROM discovery_endpoint_events WHERE owner_user_id=? AND device_id=? ORDER BY seq DESC LIMIT 1").bind(owner).bind(id).fetch_optional(&mut *connection).await?;
        if cleanup.is_some_and(|s|s!="confirmed") {return Err(conflict("target_cleanup_pending","The previous target is not yet confirmed retired. Retry the saved endpoint cleanup first."));}
        let predecessor_pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM discovery_endpoint_events e WHERE e.seq=(SELECT MAX(seq) FROM discovery_endpoint_events WHERE owner_user_id=? AND device_id=?) AND e.predecessor_device_id IS NOT NULL AND COALESCE((SELECT json_extract(p.cleanup_json,'$.state') FROM discovery_endpoint_events p WHERE p.owner_user_id=e.owner_user_id AND p.device_id=e.predecessor_device_id ORDER BY p.seq DESC LIMIT 1),'pending')<>'confirmed')").bind(owner).bind(id).fetch_one(&mut *connection).await?;
        if predecessor_pending{return Err(conflict("predecessor_cleanup_pending","The replaced device's resources must be retired before this new identity can connect."));}
        let initial_version=fingerprint(&mut connection,owner,id,&self.runtime_observations).await?;
        drop(connection);
        let node:String=row.try_get("machine_id")?;
        let asset=self.assets_for_node(owner,&node).get_device(owner,&node,id).await?;
        if asset["binding"]["state"]!="current" {return Err(conflict("target_changed","The network binding is unavailable or has changed. Review it in node details."));}
        if asset["binding"]["current"]["disabled"].as_bool()==Some(true) {return Err(conflict("network_disabled","The device's network instance is disabled."));}
        let ip:String=row.try_get("overlay_ip")?;
        let parsed:Ipv4Addr=ip.parse().map_err(|_|AutomationError::field("overlay_ip","Automation currently requires a plain IPv4 address."))?;
        if parsed.is_unspecified()||parsed.is_loopback()||parsed.is_link_local()||parsed.is_multicast()||parsed.is_broadcast()||parsed.octets()[0]>=240 {
            return Err(AutomationError::field("overlay_ip","This IPv4 address cannot be an automation target."));
        }
        let mut connection=self.pool.acquire().await?;
        let version=fingerprint(&mut connection,owner,id,&self.runtime_observations).await?;
        if version!=initial_version {return Err(conflict("target_changed","The device target changed while it was being prepared."));}
        if enforce_route_unambiguous {
            ensure_route_unambiguous(&mut connection,owner,id,&ip,row.try_get("adb_port")?).await?;
        }
        Ok(json!({"device_id":id,"name":row.try_get::<String,_>("name")?,"owner_user_id":owner,"machine_id":node,
            "ip":parsed.to_string(),"adb_port":row.try_get::<i64,_>("adb_port")?,"connection_mode":row.try_get::<String,_>("connection_mode")?,
            "adb_service_id":row.try_get::<String,_>("adb_service_id")?,"endpoint_revision":row.try_get::<i64,_>("endpoint_revision")?,"target_fingerprint":version,"route_context":"default"}))
    }

    pub(crate) async fn create_session(&self,actor:i32,owner:i32,device_id:&str,quality:&str)->AutomationResult<Value> {
        if !matches!(quality,"wall"|"focus") {return Err(AutomationError::field("quality","Choose wall or focus quality."));}
        self.authorize(actor,owner).await?;
        let target=self.target(owner,device_id).await?;
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        authorized(&mut tx,actor,owner).await?;
        if fingerprint(&mut tx,owner,device_id,&self.runtime_observations).await?!=target["target_fingerprint"].as_str().unwrap_or("") {return Err(conflict("target_changed","The device address changed before the session was saved."));}
        ensure_route_unambiguous(&mut tx,owner,device_id,target["ip"].as_str().ok_or_else(not_found)?,target["adb_port"].as_i64().ok_or_else(not_found)?).await?;
        let key=target_key(&target)?;
        // A NULL runner identity proves the run was never dispatched: binding
        // the executor identity is committed before the POST to /runs. Older
        // builds incorrectly left these rows as outcome_unknown when identity
        // lookup failed, permanently blocking preview despite no device action.
        // Reconcile only that provably-unsent case, plus abandoned dispatch
        // admissions older than one minute. The transaction fences late binders.
        let timestamp=now();
        let never_dispatched:Vec<String>=sqlx::query_scalar(
            "SELECT DISTINCT r.id FROM automation_runs r JOIN automation_run_devices d ON d.run_id=r.id WHERE (d.device_id=? OR d.target_key=?) AND d.cleanup_confirmed=0 AND d.state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending') AND r.runner_identity IS NULL AND (r.state='outcome_unknown' OR (r.state='dispatch_pending' AND julianday(r.updated_at)<julianday('now','-60 seconds')))"
        ).bind(device_id).bind(&key).fetch_all(&mut *tx).await?;
        for run_id in never_dispatched {
            sqlx::query("UPDATE automation_run_devices SET state='cancelled',cleanup_confirmed=1,updated_at=? WHERE run_id=? AND cleanup_confirmed=0")
                .bind(&timestamp).bind(&run_id).execute(&mut *tx).await?;
            sqlx::query("UPDATE automation_runs SET cancel_requested=1,state='cancelled',never_started_confirmed_at=?,updated_at=? WHERE id=? AND runner_identity IS NULL AND state IN ('dispatch_pending','outcome_unknown')")
                .bind(&timestamp).bind(&timestamp).bind(&run_id).execute(&mut *tx).await?;
        }
        let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_sessions WHERE (device_id=? OR target_key=?) AND state NOT IN ('closed','failed'))")
            .bind(device_id).bind(&key).fetch_one(&mut *tx).await?;
        if busy {return Err(conflict("device_busy","This device already has a viewing session or pending cleanup. Close it before opening another."));}
        // T13: the reverse fence of create_run(). Both admissions use BEGIN
        // IMMEDIATE so a competing page cannot open manual control after a
        // script has been recorded for dispatch but before executor claim.
        // Match by physical device OR ADB endpoint, including any tenant
        // sharing that endpoint. Unknown outcomes retain the lease.
        let blocking_rows=sqlx::query(
            "SELECT d.run_id,d.device_id,d.target_key,d.target_fingerprint,d.state AS device_state, \
             d.cleanup_confirmed,d.withdraw_requested,d.withdraw_event_id,d.updated_at AS device_updated_at, \
             r.owner_user_id,r.actor_user_id,r.state AS run_state,r.runner_identity,r.updated_at AS run_updated_at \
             FROM automation_run_devices d LEFT JOIN automation_runs r ON r.id=d.run_id \
             WHERE (d.device_id=? OR d.target_key=?) AND d.cleanup_confirmed=0 \
             AND d.state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending') \
             ORDER BY d.updated_at DESC"
        ).bind(device_id).bind(&key).fetch_all(&mut *tx).await?;
        if !blocking_rows.is_empty() {
            let mut blockers=Vec::with_capacity(blocking_rows.len());
            for row in blocking_rows {
                let run_id:String=row.try_get("run_id")?;
                let row_device:String=row.try_get("device_id")?;
                let row_key:String=row.try_get("target_key")?;
                let row_fingerprint:String=row.try_get("target_fingerprint")?;
                let device_state:String=row.try_get("device_state")?;
                let run_state:Option<String>=row.try_get("run_state")?;
                let cleanup_confirmed:bool=row.try_get("cleanup_confirmed")?;
                let withdraw_requested:bool=row.try_get("withdraw_requested")?;
                let withdraw_event_id:Option<String>=row.try_get("withdraw_event_id")?;
                let device_updated_at:String=row.try_get("device_updated_at")?;
                let run_updated_at:Option<String>=row.try_get("run_updated_at")?;
                let row_owner:Option<i32>=row.try_get("owner_user_id")?;
                let row_actor:Option<i32>=row.try_get("actor_user_id")?;
                let runner_identity:Option<String>=row.try_get("runner_identity")?;
                let device_matches=row_device==device_id;
                let endpoint_matches=row_key==key;
                let match_reason=match (device_matches,endpoint_matches) {
                    (true,true)=>"device_id_and_adb_endpoint",
                    (true,false)=>"device_id",
                    (false,true)=>"adb_endpoint",
                    (false,false)=>"unknown",
                };
                blockers.push(json!({
                    "run_id":run_id,
                    "device_id":row_device,
                    "match_reason":match_reason,
                    "device_state":device_state,
                    "run_state":run_state,
                    "run_record_present":row_owner.is_some(),
                    "orphaned_run_record":row_owner.is_none(),
                    "cleanup_confirmed":cleanup_confirmed,
                    "owner_matches":row_owner==Some(owner),
                    "actor_matches":row_actor==Some(actor),
                    "runner_identity_present":runner_identity.is_some(),
                    "runner_identity":runner_identity,
                    "withdraw_requested":withdraw_requested,
                    "withdraw_event_id":withdraw_event_id,
                    "target_fingerprint_matches":row_fingerprint==target["target_fingerprint"].as_str().unwrap_or(""),
                    "device_updated_at":device_updated_at,
                    "run_updated_at":run_updated_at
                }));
            }
            let blocking_run_count=blockers.len();
            let diagnostics=json!({
                "stage":"database_run_lease",
                "source":"sqlite.automation_run_devices",
                "device_id":device_id,
                "adb_endpoint":key,
                "blocking_run_count":blocking_run_count,
                "blocking_runs":blockers,
                "why_restart_did_not_clear_it":"This is a durable Web database lease. Restarting the Web and executor processes does not delete it. A row with runner_identity present is retained because execution may have reached the executor."
            });
            tracing::warn!(
                owner_user_id=owner,
                device_id=%device_id,
                adb_endpoint=%key,
                blocking_run_count=blocking_run_count,
                diagnostics=%diagnostics,
                "automation manual preview blocked by persistent script lease"
            );
            return Err(conflict("device_busy","The device has a running script or unconfirmed script cleanup; manual preview is unavailable.")
                .with_details(diagnostics));
        }
        let id=Uuid::new_v4().to_string();let timestamp=now();
        sqlx::query("INSERT INTO automation_sessions(id,actor_user_id,owner_user_id,device_id,target_key,target_fingerprint,target_json,quality,state,created_at,updated_at,expires_at) VALUES(?,?,?,?,?,?,?,?,'opening',?,?,?)")
            .bind(&id).bind(actor).bind(owner).bind(device_id).bind(&key).bind(target["target_fingerprint"].as_str()).bind(target.to_string()).bind(quality).bind(&timestamp).bind(&timestamp).bind(deadline()).execute(&mut *tx).await?;
        tx.commit().await?;
        self.session(owner,&id).await
    }

    pub(crate) async fn session(&self,owner:i32,id:&str)->AutomationResult<Value> {
        let row=sqlx::query("SELECT * FROM automation_sessions WHERE id=? AND owner_user_id=?").bind(id).bind(owner).fetch_optional(&self.pool).await?.ok_or_else(not_found)?;
        session_json(&row)
    }

    pub(crate) async fn validate_session(&self,owner:i32,id:&str)->AutomationResult<Value> {
        let session=self.session(owner,id).await?;
        let actor=session["actor_user_id"].as_i64().ok_or_else(not_found)? as i32;
        self.authorize(actor,owner).await?;
        if !matches!(session["state"].as_str(),Some("opening"|"active"|"suspending"|"view_only"|"resuming")) {return Err(conflict("session_revoked","The viewing session has ended or its authority was revoked."));}
        let current=self.target(owner,session["device_id"].as_str().ok_or_else(not_found)?).await?;
        if current["target_fingerprint"]!=session["target"]["target_fingerprint"] {return Err(conflict("target_changed","The session's device target has changed."));}
        Ok(session)
    }

    /// T38: atomically fence manual writes before an Executor downgrade.
    /// The two-phase transition deliberately remains fail-closed if the
    /// remote acknowledgement or its network path becomes unknown.
    pub(crate) async fn begin_session_permission(&self,owner:i32,id:&str,direction:&str)->AutomationResult<()> {
        if !matches!(direction,"view_only"|"active") {
            return Err(AutomationError::field("state","Unsupported permission transition."));
        }
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row=sqlx::query("SELECT actor_user_id,device_id,target_key,target_fingerprint,state FROM automation_sessions WHERE id=? AND owner_user_id=?")
            .bind(id).bind(owner).fetch_optional(&mut *tx).await?.ok_or_else(not_found)?;
        authorized(&mut tx,row.try_get::<i32,_>("actor_user_id")?,owner).await?;
        let device:String=row.try_get("device_id")?;
        let key:String=row.try_get("target_key")?;
        let expected:String=row.try_get("target_fingerprint")?;
        if fingerprint(&mut tx,owner,&device,&self.runtime_observations).await?!=expected {
            return Err(conflict("target_changed","Viewing device target changed. Do not transfer its action lease."));
        }
        let previous:String=row.try_get("state")?;
        let (stable,in_progress,next)=if direction=="view_only" {
            ("view_only","suspending","suspending")
        } else {
            ("active","resuming","resuming")
        };
        if previous==stable || previous==in_progress {
            tx.commit().await?;
            return Ok(());
        }
        if (direction=="view_only" && previous!="active")
            || (direction=="active" && previous!="view_only") {
            return Err(conflict("session_revoked","The media session is not in a transferable state."));
        }
        if direction=="active" {
            let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_run_devices WHERE (device_id=? OR target_key=?) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending'))")
                .bind(&device).bind(&key).fetch_one(&mut *tx).await?;
            if busy {return Err(conflict("device_busy","Script action lease or cleanup still occupies this device."));}
        }
        sqlx::query("UPDATE automation_sessions SET state=?,updated_at=? WHERE id=? AND owner_user_id=?")
            .bind(next).bind(now()).bind(id).bind(owner).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn confirm_session_permission(&self,owner:i32,id:&str,direction:&str)->AutomationResult<()> {
        let (expected,next)=match direction {
            "view_only"=>("suspending","view_only"),
            "active"=>("resuming","active"),
            _=>return Err(AutomationError::field("state","Unsupported permission transition.")),
        };
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row=sqlx::query("SELECT state,device_id,target_key,target_fingerprint FROM automation_sessions WHERE id=? AND owner_user_id=?")
            .bind(id).bind(owner).fetch_optional(&mut *tx).await?.ok_or_else(not_found)?;
        let actual:String=row.try_get("state")?;
        if actual==next {tx.commit().await?;return Ok(());}
        if actual!=expected {return Err(conflict("session_revoked","Session permission transition is no longer pending."));}
        let device:String=row.try_get("device_id")?;
        let key:String=row.try_get("target_key")?;
        let finger:String=row.try_get("target_fingerprint")?;
        if fingerprint(&mut tx,owner,&device,&self.runtime_observations).await?!=finger {
            return Err(conflict("target_changed","Device binding changed while transferring the session."));
        }
        if direction=="active" {
            let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_run_devices WHERE (device_id=? OR target_key=?) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending'))")
                .bind(&device).bind(&key).fetch_one(&mut *tx).await?;
            if busy {return Err(conflict("device_busy","Script cleanup has not confirmed release of the device."));}
        }
        sqlx::query("UPDATE automation_sessions SET state=?,updated_at=? WHERE id=? AND owner_user_id=? AND state=?")
            .bind(next).bind(now()).bind(id).bind(owner).bind(expected).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn update_session(&self,id:&str,state:&str)->AutomationResult<()> {
        if !matches!(state,"active"|"suspending"|"view_only"|"resuming"|"closed"|"closing"|"failed"|"cleanup_pending"|"outcome_unknown"|"revoked") {return Err(AutomationError::field("state","Invalid session state."));}
        let predicate=if state=="active" {"state IN ('opening','active')"} else {"state NOT IN ('closed','failed')"};
        let changed=sqlx::query(&format!("UPDATE automation_sessions SET state=?,updated_at=? WHERE id=? AND {predicate}"))
            .bind(state).bind(now()).bind(id).execute(&self.pool).await?;
        if state=="active" && changed.rows_affected()==0 {return Err(conflict("session_revoked","The session cannot be reactivated."));}
        Ok(())
    }

    pub(crate) async fn renew_session(&self,id:&str)->AutomationResult<()> {
        let changed=sqlx::query("UPDATE automation_sessions SET expires_at=?,updated_at=? WHERE id=? AND state IN ('opening','active','suspending','view_only','resuming')")
            .bind(deadline()).bind(now()).bind(id).execute(&self.pool).await?;
        if changed.rows_affected()==0 {return Err(conflict("session_revoked","The session cannot be renewed."));}
        Ok(())
    }
}
