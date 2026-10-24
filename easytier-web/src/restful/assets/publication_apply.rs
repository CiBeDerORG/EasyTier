//! Apply committed service port mappings to the exact selected runtime.
//! The asset transaction owns desired state. Runtime failures are reported as
//! pending/failed application, never as a rolled-back or fully applied save.
use std::{collections::{BTreeMap, BTreeSet}, net::IpAddr, sync::Arc};

use easytier::proto::{
    api::manage::{CollectNetworkInfoRequest, ConfigSource, GetNetworkInstanceConfigRequest,
        ListNetworkInstanceMetaRequest, NetworkConfig, PortForwardConfig, RunNetworkInstanceRequest},
    rpc_types::controller::BaseController,
};
use easytier_core::management::ConfigFilePermission;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{central_network::service::CentralNetworkService, client_manager::session::Session};
use super::{AppStateInner, HttpResult, database_error};

type Target = (Uuid, Uuid);

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SavedConfig {
    id: i64,
    created: String,
    source: String,
    disabled: bool,
    raw: String,
    config: NetworkConfig,
}

#[derive(Clone)]
pub(crate) struct ApplyPlan {
    before: BTreeMap<Target, Option<SavedConfig>>,
    since: i64,
    selected: Option<Target>,
}

fn binding_target(value: &Value) -> Option<Target> {
    Some((value["machine_id"].as_str()?.trim().parse().ok()?,
        value["instance_id"].as_str()?.trim().parse().ok()?))
}

async fn load(service: &CentralNetworkService, owner: i32, target: Target) -> Result<Option<SavedConfig>, sqlx::Error> {
    let row = sqlx::query("SELECT id, create_time, source, disabled, network_config FROM user_running_network_configs WHERE user_id = ? AND device_id = ? AND network_instance_id = ?")
        .bind(owner).bind(target.0.to_string()).bind(target.1.to_string())
        .fetch_optional(&service.db.inner()).await?;
    row.map(|row| {
        let raw: String = row.try_get("network_config")?;
        let config = serde_json::from_str(&raw).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        Ok(SavedConfig { id: row.try_get("id")?, created: row.try_get("create_time")?,
            source: row.try_get("source")?, disabled: row.try_get("disabled")?, raw, config })
    }).transpose()
}

/// Called while holding the shared asset/network mutation lock, before saving.
/// Capture old targets as well as the requested new one: unpublishing/rebinding
/// must still apply the removal after its publication record has been deleted.
pub(crate) async fn snapshot(
    service: &CentralNetworkService, owner: i32, node: &str, requested: Option<&Value>,
) -> HttpResult<ApplyPlan> {
    let since:i64=sqlx::query_scalar("SELECT COALESCE(MAX(seq),0) FROM discovery_endpoint_events").fetch_one(&service.db.inner()).await.map_err(database_error)?;
    let rows = sqlx::query("SELECT DISTINCT public_machine_id, instance_id FROM asset_service_publications WHERE owner_user_id = ? AND machine_id = ?")
        .bind(owner).bind(node).fetch_all(&service.db.inner()).await.map_err(database_error)?;
    let mut targets = BTreeSet::new();
    for row in rows {
        let machine: String = row.try_get("public_machine_id").map_err(database_error)?;
        let instance: String = row.try_get("instance_id").map_err(database_error)?;
        if let (Ok(machine), Ok(instance)) = (machine.parse(), instance.parse()) { targets.insert((machine, instance)); }
    }
    if let Some(target) = requested.and_then(binding_target) { targets.insert(target); }
    let mut before = BTreeMap::new();
    for target in targets {
        before.insert(target, load(service, owner, target).await.map_err(database_error)?);
    }
    Ok(ApplyPlan { before, since, selected: None })
}

fn address(raw: &str) -> Option<IpAddr> {
    let raw = raw.strip_prefix('[').and_then(|value| value.strip_suffix(']')).unwrap_or(raw);
    raw.parse::<IpAddr>().ok().map(|address| match address {
        IpAddr::V6(address) => address.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(address)),
        address => address,
    })
}

fn same_rule(left: &PortForwardConfig, right: &PortForwardConfig) -> bool {
    left.proto.eq_ignore_ascii_case(&right.proto)
        && left.bind_port == right.bind_port && left.dst_port == right.dst_port
        && address(&left.bind_ip).is_some() && address(&left.bind_ip) == address(&right.bind_ip)
        && address(&left.dst_ip).is_some() && address(&left.dst_ip) == address(&right.dst_ip)
}

fn same_rules(left: &[PortForwardConfig], right: &[PortForwardConfig]) -> bool {
    let mut remaining = right.to_vec();
    for rule in left {
        let Some(index) = remaining.iter().position(|other| same_rule(rule, other)) else { return false; };
        remaining.remove(index);
    }
    remaining.is_empty()
}

fn overlaps(left: &PortForwardConfig, right: &PortForwardConfig) -> bool {
    if !left.proto.eq_ignore_ascii_case(&right.proto) || left.bind_port != right.bind_port { return false; }
    match (address(&left.bind_ip), address(&right.bind_ip)) {
        (Some(left), Some(right)) => left == right || left.is_unspecified() || right.is_unspecified(),
        _ => true,
    }
}

/// Keep manual rules observed on the runtime, except exact service-owned rules
/// removed by this transaction. Never replace a conflicting runtime listener.
fn merge_rules(before: &[PortForwardConfig], desired: &[PortForwardConfig], live: &[PortForwardConfig]) -> Result<Vec<PortForwardConfig>, &'static str> {
    let mut removed = before.to_vec();
    for rule in desired {
        if let Some(index) = removed.iter().position(|old| same_rule(old, rule)) { removed.remove(index); }
    }
    let mut merged = desired.to_vec();
    for rule in live {
        if let Some(index) = removed.iter().position(|old| same_rule(old, rule)) {
            removed.remove(index);
            continue;
        }
        if merged.iter().any(|existing| same_rule(existing, rule)) { continue; }
        if merged.iter().any(|existing| overlaps(existing, rule)) { return Err("runtime_port_conflict"); }
        merged.push(rule.clone());
    }
    Ok(merged)
}

fn controller() -> BaseController {
    BaseController { timeout_ms: 5_000, ..Default::default() }
}

async fn current_session(manager: &AppStateInner, owner: i32, machine: Uuid, session: &Arc<Session>) -> bool {
    manager.get_session_by_machine_id(owner, &machine).is_some_and(|current| Arc::ptr_eq(&current, session))
        && session.get_token().await.is_some_and(|token| token.user_id == owner && token.machine_id == machine)
}

async fn preserve_live_rules(service: &CentralNetworkService, owner: i32, target: Target, desired: &SavedConfig, rules: &[PortForwardConfig]) -> Result<(), &'static str> {
    if same_rules(&desired.config.port_forwards, rules) { return Ok(()); }
    let mut value: Value = serde_json::from_str(&desired.raw).map_err(|_| "saved_config_invalid")?;
    value["port_forwards"] = json!(rules);
    let pool = service.db.inner();
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(|_| "database_unavailable")?;
    let updated = sqlx::query("UPDATE user_running_network_configs SET network_config = ?, update_time = ? WHERE id = ? AND user_id = ? AND device_id = ? AND network_instance_id = ? AND create_time = ? AND source = 'web' AND disabled = 0 AND network_config = ?")
        .bind(value.to_string()).bind(chrono::Local::now().fixed_offset()).bind(desired.id).bind(owner)
        .bind(target.0.to_string()).bind(target.1.to_string()).bind(&desired.created).bind(&desired.raw)
        .execute(&mut *transaction).await.map_err(|_| "database_unavailable")?;
    if updated.rows_affected() != 1 { return Err("saved_config_changed"); }
    sqlx::query("DELETE FROM managed_config_revisions WHERE user_id = ? AND device_id = ?")
        .bind(owner).bind(target.0.to_string()).execute(&mut *transaction).await.map_err(|_| "database_unavailable")?;
    transaction.commit().await.map_err(|_| "database_unavailable")?;
    Ok(())
}

async fn apply_one(manager: &AppStateInner, service: &CentralNetworkService, owner: i32,
    target: Target, before: Option<&SavedConfig>, desired: &SavedConfig,
) -> Result<(), &'static str> {
    if desired.source != "web" || desired.disabled { return Err("saved_network_unavailable"); }
    if desired.config.instance_id.as_deref().and_then(|id| id.parse::<Uuid>().ok()) != Some(target.1) {
        return Err("instance_id_mismatch");
    }
    if before.is_some_and(|before| before.id != desired.id || before.created != desired.created) {
        return Err("saved_config_changed");
    }
    if !service.db.is_user_active(owner).await.map_err(|_| "database_unavailable")? { return Err("owner_unavailable"); }
    let central = service.db.central_owned_runtime_configs(owner, target.0).await.map_err(|_| "database_unavailable")?;
    if central.iter().any(|owned| owned.instance_id == target.1
        || desired.config.network_name.as_deref() == Some(owned.network_name.as_str())) { return Err("centrally_managed"); }
    let session = manager.get_session_by_machine_id(owner, &target.0).ok_or("server_offline")?;
    if !current_session(manager, owner, target.0, &session).await { return Err("session_changed"); }
    let rpc = session.scoped_rpc_client();
    // Own this Send client in the future; a boxed RPC service need not be Sync.
    async move {
        let observed = rpc.collect_network_info(controller(), CollectNetworkInfoRequest {
            inst_ids: vec![target.1.into()],
        }).await.map_err(|_| "runtime_unavailable")?;
        if !observed.info.as_ref().and_then(|info| info.map.get(&target.1.to_string())).is_some_and(|info| info.running) {
            return Err("network_not_running");
        }
        let metas = rpc.list_network_instance_meta(controller(), ListNetworkInstanceMetaRequest {
            inst_ids: vec![target.1.into()],
        }).await.map_err(|_| "runtime_unavailable")?;
        let metadata = metas.metas.iter().find(|meta| meta.inst_id.map(Uuid::from) == Some(target.1)).ok_or("runtime_metadata_missing")?;
        if metadata.source != ConfigSource::User as i32 && metadata.source != ConfigSource::Web as i32 {
            return Err("runtime_source_unavailable");
        }
        if ConfigFilePermission::from(metadata.config_permission).has_flag(ConfigFilePermission::READ_ONLY) {
            return Err("runtime_read_only");
        }
        if central.iter().any(|owned| owned.network_name == metadata.network_name) { return Err("centrally_managed"); }
        let current = rpc.get_network_instance_config(controller(), GetNetworkInstanceConfigRequest {
            inst_id: Some(target.1.into()),
        }).await.map_err(|_| "runtime_unavailable")?.config.ok_or("runtime_config_missing")?;
        if current.instance_id.as_deref().and_then(|id| id.parse::<Uuid>().ok()) != Some(target.1) { return Err("instance_id_mismatch"); }
        let previous_rules = before.map(|config| config.config.port_forwards.as_slice()).unwrap_or_default();
        let rules = merge_rules(previous_rules, &desired.config.port_forwards, &current.port_forwards)?;
        // Preserve runtime-only manual forwards in desired state too, otherwise
        // the heartbeat reconciler would delete them after this immediate apply.
        preserve_live_rules(service, owner, target, desired, &rules).await?;
        if !current_session(manager, owner, target.0, &session).await { return Err("session_changed"); }
        if same_rules(&current.port_forwards, &rules) { return Ok(()); }
        let mut config = current;
        config.port_forwards = rules.clone();
        session.invalidate_runtime_config_for_direct_mutation().await;
        // The same RunNetworkInstance RPC/overwrite semantics as the network
        // editor's Run button; keep the live settings and replace only forwards.
        let applied = rpc.run_network_instance(controller(), RunNetworkInstanceRequest {
            inst_id: Some(target.1.into()), config: Some(config), overwrite: true,
            source: ConfigSource::Web as i32,
        }).await;
        session.invalidate_runtime_config_for_direct_mutation().await;
        let applied = applied.map_err(|_| "runtime_apply_failed")?;
        if applied.inst_id.map(Uuid::from) != Some(target.1) { return Err("instance_id_mismatch"); }
        if !current_session(manager, owner, target.0, &session).await { return Err("session_changed"); }
        // Read via the SAME API used by the network editor, not a DB fallback.
        let readback = rpc.get_network_instance_config(controller(), GetNetworkInstanceConfigRequest {
            inst_id: Some(target.1.into()),
        }).await.map_err(|_| "runtime_readback_failed")?.config.ok_or("runtime_config_missing")?;
        if readback.instance_id.as_deref().and_then(|id| id.parse::<Uuid>().ok()) != Some(target.1)
            || !same_rules(&readback.port_forwards, &rules) { return Err("runtime_mapping_mismatch"); }
        let observed = rpc.collect_network_info(controller(), CollectNetworkInfoRequest {
            inst_ids: vec![target.1.into()],
        }).await.map_err(|_| "runtime_readback_failed")?;
        if !observed.info.as_ref().and_then(|info| info.map.get(&target.1.to_string())).is_some_and(|info| info.running) {
            return Err("network_not_running");
        }
        if !current_session(manager, owner, target.0, &session).await { return Err("session_changed"); }
        Ok(())
    }.await
}

/// The caller keeps the shared mutation lock through this bounded RPC work.
/// Only changed targets, plus the selected publication on a retry, are applied.
async fn apply_inner(manager: &AppStateInner, service: &CentralNetworkService, owner: i32,
    mut plan: ApplyPlan, response: Option<&Value>,
) -> Value {
    let selected = response.and_then(|value| binding_target(&value["publication"])).or(plan.selected);
    if let Some(target) = selected { plan.before.entry(target).or_insert(None); }
    let mut targets: Vec<_> = plan.before.into_iter().collect();
    // Release the old instance's listener first, even if its UUID sorts after
    // the newly selected one. Network changes on one server share socket ports.
    targets.sort_by_key(|(target, _)| selected == Some(*target));
    let mut cleanup_pending = BTreeSet::new();
    let mut reports = Vec::new();
    for (target, before) in targets {
        let desired = match load(service, owner, target).await {
            Ok(Some(desired)) => desired,
            Ok(None) if before.is_none() && selected != Some(target) => continue,
            _ => {
                cleanup_pending.insert(target.0);
                reports.push(json!({"machine_id":target.0,"instance_id":target.1,"status":"pending","code":"saved_config_unavailable"}));
                continue;
            }
        };
        if selected != Some(target) && before.as_ref().is_some_and(|before|
            before.id == desired.id && before.created == desired.created && before.source == desired.source
            && same_rules(&before.config.port_forwards, &desired.config.port_forwards)) { continue; }
        if selected == Some(target) && cleanup_pending.contains(&target.0) {
            reports.push(json!({"machine_id":target.0,"instance_id":target.1,"status":"pending","code":"previous_mapping_cleanup_pending"}));
            continue;
        }
        let result = apply_one(manager, service, owner, target, before.as_ref(), &desired).await;
        // Keep the existing persisted desired-state retry mechanism for offline
        // nodes and fence any runtime observation made before this operation.
        manager.invalidate_applied_config_revision(owner, target.0).await;
        let (status, code) = match result {
            Ok(()) => ("applied", "verified"),
            Err(code @ ("server_offline" | "session_changed" | "network_not_running" | "runtime_unavailable")) => ("pending", code),
            Err(code) => ("failed", code),
        };
        if status != "applied" {
            cleanup_pending.insert(target.0);
            tracing::warn!(owner, machine_id = %target.0, instance_id = %target.1, code,
                "service saved, but public port mappings were not confirmed on the runtime");
        }
        reports.push(json!({"machine_id":target.0,"instance_id":target.1,"status":status,"code":code}));
    }
    let status = if reports.iter().any(|report| report["status"] == "failed") { "failed" }
        else if reports.iter().any(|report| report["status"] == "pending") { "pending" } else { "applied" };
    json!({"status":status,"targets":reports})
}

#[cfg(test)]
#[path = "publication_apply_tests.rs"]
mod tests;

impl ApplyPlan {
    pub(crate) fn encode(&self) -> Value {
        json!({"selected":self.selected,"targets":self.before.iter().map(|(key,value)|json!({"machine":key.0,"instance":key.1,"before":value})).collect::<Vec<_>>()})
    }
    pub(crate) fn decode(value: &Value) -> Option<Self> {
        let mut before = BTreeMap::new();
        for row in value.get("targets").unwrap_or(value).as_array()? {
            before.insert((row["machine"].as_str()?.parse().ok()?,row["instance"].as_str()?.parse().ok()?),serde_json::from_value(row["before"].clone()).ok()?);
        }
        Some(Self{before,since:0,selected:serde_json::from_value(value["selected"].clone()).ok().flatten()})
    }
    pub(crate) async fn fence(&self, service:&CentralNetworkService,owner:i32)->Option<Value> {
        let mut values=Vec::new();
        for target in self.before.keys(){let current=load(service,owner,*target).await.ok()?;values.push(json!([target.0,target.1,current.map(|c|json!([c.id,c.created,c.raw,c.disabled]))]));}
        Some(json!(crate::db::discovery::hash(&json!(values))))
    }
}

pub(crate) async fn apply(manager:&AppStateInner,service:&CentralNetworkService,owner:i32,mut plan:ApplyPlan,response:Option<&Value>)->Value {
    plan.selected=response.and_then(|v|binding_target(&v["publication"]));
    if let Some(target)=plan.selected{plan.before.entry(target).or_insert(None);}
    let id=Uuid::new_v4().to_string();
    let Some(fence)=plan.fence(service,owner).await else{return json!({"status":"pending","code":"publication_fence_unavailable"});};
    let saved=sqlx::query("INSERT INTO discovery_publication_plans(id,owner_user_id,plan_json,desired_fence_json,created_at) VALUES(?,?,?,?,?)").bind(&id).bind(owner).bind(plan.encode().to_string()).bind(fence.to_string()).bind(crate::db::assets::common::now()).execute(&service.db.inner()).await;
    if saved.is_err(){return json!({"status":"pending","code":"publication_plan_not_saved"});}
    let pending=json!({"state":"pending","plan_id":id});
    if sqlx::query("UPDATE discovery_endpoint_events SET publication_json=? WHERE owner_user_id=? AND seq>?").bind(pending.to_string()).bind(owner).bind(plan.since).execute(&service.db.inner()).await.is_err(){return pending;}
    let report=apply_inner(manager,service,owner,plan.clone(),response).await;
    if let Some(fence)=plan.fence(service,owner).await{let _=sqlx::query("UPDATE discovery_publication_plans SET desired_fence_json=? WHERE id=?").bind(fence.to_string()).bind(&id).execute(&service.db.inner()).await;}
    let _=sqlx::query("UPDATE discovery_publication_plans SET report_json=? WHERE id=?").bind(report.to_string()).bind(&id).execute(&service.db.inner()).await;
    let _=sqlx::query("UPDATE discovery_endpoint_events SET publication_json=? WHERE owner_user_id=? AND json_extract(publication_json,'$.plan_id')=?").bind(json!({"state":report["status"],"plan_id":id,"report":report}).to_string()).bind(owner).bind(&id).execute(&service.db.inner()).await;
    report
}
pub(crate) async fn retry(manager:&AppStateInner,service:&CentralNetworkService,owner:i32,id:&str)->HttpResult<Value>{
    let row=sqlx::query("SELECT * FROM discovery_publication_plans WHERE id=? AND owner_user_id=?").bind(id).bind(owner).fetch_optional(&service.db.inner()).await.map_err(database_error)?.ok_or_else(||super::asset_error(crate::db::assets::AssetError::not_found()))?;
    let raw:String=row.try_get("plan_json").map_err(database_error)?;
    let plan=serde_json::from_str::<Value>(&raw).ok().and_then(|v|ApplyPlan::decode(&v)).ok_or_else(||super::asset_error(crate::db::assets::AssetError::conflict("publication_plan_invalid","The original publication plan is unavailable.")))?;
    let fence:String=row.try_get("desired_fence_json").map_err(database_error)?;
    if plan.fence(service,owner).await.map(|v|v.to_string())!=Some(fence){return Err(super::asset_error(crate::db::assets::AssetError::conflict("publication_changed","The saved public configuration changed. Review its current mapping.")));}
    let previous:Option<String>=row.try_get("report_json").map_err(database_error)?;
    if let Some(previous)=previous.as_deref().and_then(|s|serde_json::from_str::<Value>(s).ok()).filter(|v|v["status"]=="applied"){return Ok(previous);}
    let report=apply_inner(manager,service,owner,plan.clone(),None).await;
    if let Some(fence)=plan.fence(service,owner).await{sqlx::query("UPDATE discovery_publication_plans SET desired_fence_json=? WHERE id=?").bind(fence.to_string()).bind(id).execute(&service.db.inner()).await.map_err(database_error)?;}
    sqlx::query("UPDATE discovery_publication_plans SET report_json=? WHERE id=?").bind(report.to_string()).bind(id).execute(&service.db.inner()).await.map_err(database_error)?;
    sqlx::query("UPDATE discovery_endpoint_events SET publication_json=? WHERE owner_user_id=? AND json_extract(publication_json,'$.plan_id')=?").bind(json!({"state":report["status"],"plan_id":id,"report":report}).to_string()).bind(owner).bind(id).execute(&service.db.inner()).await.map_err(database_error)?;
    Ok(report)
}
