//! Automation business records. Phone side effects never run inside a DB transaction.
mod runs;
mod scripts;
mod validation;

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
         m.allocated_ipv4,COALESCE(ac.enabled,0) AS enabled,COALESCE(ac.adb_port,5555) AS adb_port,COALESCE(ac.connection_mode,'tcp') AS connection_mode \
         FROM asset_devices a LEFT JOIN asset_gateway_groups g ON g.id=a.gateway_group_id \
         LEFT JOIN user_running_network_configs c ON c.user_id=a.owner_user_id AND c.device_id=a.machine_id \
           AND c.network_instance_id=COALESCE(g.network_instance_id,a.network_instance_id) \
         LEFT JOIN network_members m ON m.user_id=a.owner_user_id AND m.device_id=a.machine_id AND m.network_id=c.network_instance_id \
         LEFT JOIN automation_connections ac ON ac.device_id=a.id WHERE a.owner_user_id=? AND a.id=?")
        .bind(owner).bind(id).fetch_optional(connection).await?.ok_or_else(not_found)?;
    let mut source = json!({"device_id":id,"owner_user_id":owner,"route_context":"default"});
    for name in ["overlay_ip","machine_id","gateway_group_id","address_mode","rule_key","network_instance_id","config_created","allocated_ipv4","connection_mode"] {
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
    for name in ["host_offset","current_config_id","disabled","enabled","adb_port"] {
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
    let collision:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices a LEFT JOIN automation_connections c ON c.device_id=a.id WHERE a.overlay_ip=? AND COALESCE(c.adb_port,5555)=? AND a.id<>? AND (a.owner_user_id<>? OR COALESCE(c.enabled,0)=1))")
        .bind(ip).bind(port).bind(id).bind(owner).fetch_one(&mut *connection).await?;
    let endpoint=format!("default:{ip}:{port}");
    let unresolved:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_sessions WHERE target_key=? AND owner_user_id<>? AND state NOT IN ('closed','failed')) OR EXISTS(SELECT 1 FROM automation_run_devices d JOIN automation_runs r ON r.id=d.run_id WHERE d.target_key=? AND r.owner_user_id<>? AND d.cleanup_confirmed=0 AND d.state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending'))")
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
             g.name AS gateway_name,t.name AS type_name,COALESCE(c.enabled,0) AS enabled,\
             COALESCE(c.adb_port,5555) AS adb_port,COALESCE(c.connection_mode,'tcp') AS connection_mode \
             FROM asset_devices a LEFT JOIN devices n ON n.user_id=a.owner_user_id AND n.machine_id=a.machine_id \
             LEFT JOIN asset_gateway_groups g ON g.id=a.gateway_group_id \
             LEFT JOIN asset_device_types t ON t.id=a.device_type_id \
             LEFT JOIN automation_connections c ON c.device_id=a.id \
             WHERE a.owner_user_id=? ORDER BY a.created_at DESC,a.id DESC")
            .bind(owner).fetch_all(&mut *connection).await?;
        drop(connection);
        let mut items = Vec::with_capacity(rows.len());
        for row in rows { items.push(self.device_json(owner,&row).await?); }
        Ok(json!({"items":items}))
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
        Ok(json!({
            "id":id,"name":row.try_get::<String,_>("name")?,"model":row.try_get::<Option<String>,_>("model")?,
            "owner_user_id":owner,"machine_id":node,"node_name":row.try_get::<String,_>("node_name")?,
            "gateway_group_id":row.try_get::<Option<String>,_>("gateway_group_id")?,"gateway_name":row.try_get::<Option<String>,_>("gateway_name")?,
            "device_type_id":row.try_get::<Option<String>,_>("device_type_id")?,"type_name":row.try_get::<Option<String>,_>("type_name")?,
            "overlay_ip":asset["overlay_ip"],"lan_ip":asset["lan_ip"],"binding_state":binding,
            "connection":{"enabled":enabled,"adb_port":row.try_get::<i64,_>("adb_port")?,"connection_mode":row.try_get::<String,_>("connection_mode")?},
            "target_fingerprint":version,"availability_reason":reason,"active_session":active_session
        }))
    }

    pub(crate) async fn save_connection(&self, owner: i32, id: &str, body: &Value) -> AutomationResult<Value> {
        validation::keys(body,&["enabled","adb_port","connection_mode"])?;
        let enabled=body["enabled"].as_bool().ok_or_else(||AutomationError::field("enabled","Choose whether ADB is enabled."))?;
        let port=body["adb_port"].as_i64().filter(|p|(1..=65535).contains(p)).ok_or_else(||AutomationError::field("adb_port","Use a port from 1 to 65535."))?;
        let mode=body["connection_mode"].as_str().filter(|m|matches!(*m,"tcp"|"wireless")).ok_or_else(||AutomationError::field("connection_mode","Choose TCP or wireless debugging."))?;
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        active_owner(&mut tx,owner).await?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices WHERE id=? AND owner_user_id=?)").bind(id).bind(owner).fetch_one(&mut *tx).await?;
        if !exists {return Err(not_found());}
        sqlx::query("INSERT INTO automation_connections(device_id,owner_user_id,enabled,adb_port,connection_mode,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(device_id) DO UPDATE SET enabled=excluded.enabled,adb_port=excluded.adb_port,connection_mode=excluded.connection_mode,updated_at=excluded.updated_at")
            .bind(id).bind(owner).bind(enabled).bind(port).bind(mode).bind(now()).execute(&mut *tx).await?;
        tx.commit().await?;
        let devices=self.list_devices(owner).await?;
        devices["items"].as_array().and_then(|items|items.iter().find(|v|v["id"]==id)).cloned().ok_or_else(not_found)
    }

    pub(crate) async fn target(&self, owner:i32,id:&str)->AutomationResult<Value> {
        let mut connection=self.pool.acquire().await?;
        active_owner(&mut connection,owner).await?;
        let row=sqlx::query("SELECT a.name,a.machine_id,a.overlay_ip,COALESCE(c.enabled,0) AS enabled,COALESCE(c.adb_port,5555) AS adb_port,COALESCE(c.connection_mode,'tcp') AS connection_mode FROM asset_devices a LEFT JOIN automation_connections c ON c.device_id=a.id WHERE a.id=? AND a.owner_user_id=?")
            .bind(id).bind(owner).fetch_optional(&mut *connection).await?.ok_or_else(not_found)?;
        let initial_version=fingerprint(&mut connection,owner,id,&self.runtime_observations).await?;
        drop(connection);
        if !row.try_get::<bool,_>("enabled")? {return Err(conflict("adb_not_configured","Enable and configure this device's ADB connection first."));}
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
        ensure_route_unambiguous(&mut connection,owner,id,&ip,row.try_get("adb_port")?).await?;
        Ok(json!({"device_id":id,"name":row.try_get::<String,_>("name")?,"owner_user_id":owner,"machine_id":node,
            "ip":parsed.to_string(),"adb_port":row.try_get::<i64,_>("adb_port")?,"connection_mode":row.try_get::<String,_>("connection_mode")?,
            "target_fingerprint":version,"route_context":"default"}))
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
        let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_sessions WHERE (device_id=? OR target_key=?) AND state NOT IN ('closed','failed'))")
            .bind(device_id).bind(&key).fetch_one(&mut *tx).await?;
        if busy {return Err(conflict("device_busy","This device already has a viewing session or pending cleanup. Close it before opening another."));}
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
        if !matches!(session["state"].as_str(),Some("opening"|"active")) {return Err(conflict("session_revoked","The viewing session has ended or its authority was revoked."));}
        let current=self.target(owner,session["device_id"].as_str().ok_or_else(not_found)?).await?;
        if current["target_fingerprint"]!=session["target"]["target_fingerprint"] {return Err(conflict("target_changed","The session's device target has changed."));}
        Ok(session)
    }

    pub(crate) async fn update_session(&self,id:&str,state:&str)->AutomationResult<()> {
        if !matches!(state,"active"|"closed"|"closing"|"failed"|"cleanup_pending"|"outcome_unknown"|"revoked") {return Err(AutomationError::field("state","Invalid session state."));}
        let predicate=if state=="active" {"state IN ('opening','active')"} else {"state NOT IN ('closed','failed')"};
        let changed=sqlx::query(&format!("UPDATE automation_sessions SET state=?,updated_at=? WHERE id=? AND {predicate}"))
            .bind(state).bind(now()).bind(id).execute(&self.pool).await?;
        if state=="active" && changed.rows_affected()==0 {return Err(conflict("session_revoked","The session cannot be reactivated."));}
        Ok(())
    }

    pub(crate) async fn renew_session(&self,id:&str)->AutomationResult<()> {
        let changed=sqlx::query("UPDATE automation_sessions SET expires_at=?,updated_at=? WHERE id=? AND state IN ('opening','active')")
            .bind(deadline()).bind(now()).bind(id).execute(&self.pool).await?;
        if changed.rows_affected()==0 {return Err(conflict("session_revoked","The session cannot be renewed."));}
        Ok(())
    }
}
