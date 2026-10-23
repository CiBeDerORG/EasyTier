//! Asset-side binding workflows. They do not start or stop network instances;
//! a confirmed device IP change also updates its service-owned public mappings.
use std::collections::BTreeSet;

use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use super::{
    AssetsStore, address_derivation, bindings, instances,
    common::{AssetError, AssetResult, device_json, ensure_node, group_json, now},
    devices::validate_relations, idempotency::Operation, publications, validation,
};
use instances::RuntimeObservations;

pub(super) fn stored_snapshot(row: &SqliteRow) -> AssetResult<Option<Value>> {
    let unavailable = || AssetError::new(503, "binding_snapshot_unavailable", "The saved binding is incomplete or inconsistent. Other asset information is retained.");
    let instance: Option<String> = row.try_get("network_instance_id")?;
    let record: Option<i64> = row.try_get("config_record_id")?;
    let created: Option<String> = row.try_get("config_create_time")?;
    let fingerprint: Option<String> = row.try_get("config_fingerprint")?;
    let stored: Option<String> = row.try_get("binding_snapshot_json")?;
    let device_mode = row.try_get::<String,_>("address_mode").ok();
    let Some(stored) = stored else {
        let device_has_source = device_mode.as_deref().is_some_and(|mode| mode != "legacy_manual")
            || row.try_get::<Option<String>,_>("rule_key").ok().flatten().is_some()
            || row.try_get::<Option<i64>,_>("host_offset").ok().flatten().is_some();
        if instance.is_some() || record.is_some() || created.is_some() || fingerprint.is_some() || device_has_source {
            return Err(unavailable());
        }
        return Ok(None);
    };
    let snapshot: Value = serde_json::from_str(&stored).map_err(|_| unavailable())?;
    if instance.as_deref().is_none() || record.is_none() || created.is_none() || fingerprint.is_none()
        || snapshot["instance_id"].as_str() != instance.as_deref()
        || snapshot["config_ref"]["id"].as_i64() != record
        || snapshot["config_ref"]["create_time"].as_str() != created.as_deref()
        || snapshot["config_fingerprint"].as_str() != fingerprint.as_deref()
        || device_mode.as_deref() == Some("legacy_manual") {
        return Err(unavailable());
    }
    Ok(Some(snapshot))
}

pub(super) async fn subject_row(connection: &mut SqliteConnection, owner: i32, node: &str, kind: &str, id: &str) -> AssetResult<SqliteRow> {
    let table = match kind {
        "gateway" => "asset_gateway_groups",
        "device" => "asset_devices",
        _ => return Err(AssetError::field("subject.resource_kind", "Choose a gateway or device.")),
    };
    sqlx::query(&format!("SELECT * FROM {table} WHERE id = ? AND owner_user_id = ? AND machine_id = ?"))
        .bind(id).bind(owner).bind(node).fetch_optional(connection).await?
        .ok_or_else(AssetError::not_found)
}

pub(super) fn subject(body: &Value) -> AssetResult<Value> {
    let input = body.get("subject").ok_or_else(|| AssetError::field("subject", "The asset context is required."))?;
    validation::keys(input, &["resource_kind", "id"])?;
    let kind = validation::required_string(input, "resource_kind", 16)?;
    if !matches!(kind.as_str(), "gateway" | "device") {
        return Err(AssetError::field("subject.resource_kind", "Choose a gateway or device."));
    }
    Ok(json!({"resource_kind":kind,"id":validation::optional_id(input,"id")?}))
}

pub(super) fn version(input: &Value) -> AssetResult<String> {
    validation::required_string(input, "preview_version", 256)
}

pub(super) fn assert_version(body: &Value, preview: &Value) -> AssetResult<()> {
    let expected = version(body)?;
    if preview["preview_version"].as_str() != Some(expected.as_str()) {
        return Err(AssetError::conflict("instance_config_changed", "The binding or address changed after the preview. Review the current preview before saving."));
    }
    Ok(())
}

pub(super) async fn usable_candidate(connection: &mut SqliteConnection, owner: i32, node: &str,
    instance_id: &str, role: &str, exclude_group: Option<&str>, runtime: &RuntimeObservations) -> AssetResult<Value> {
    let candidate = instances::get_candidate(connection, owner, node, instance_id, exclude_group, runtime).await?
        .ok_or_else(|| AssetError::conflict("binding_instance_missing", "The selected instance has no saved configuration in this node."))?;
    if candidate["role"].as_str() != Some(role) {
        return Err(AssetError::conflict("binding_role_mismatch", "The selected instance configuration does not match this device role."));
    }
    if candidate["selectable"].as_bool() != Some(true) {
        let code = if !candidate["bound_gateway"].is_null() { "instance_already_bound" } else { "binding_instance_unavailable" };
        return Err(AssetError::conflict(code, "The selected instance cannot be bound. Refresh its information and review the reported reason."));
    }
    Ok(candidate)
}

pub(super) async fn binding_info(connection: &mut SqliteConnection, row: &SqliteRow, kind: &str, runtime: &RuntimeObservations) -> AssetResult<Value> {
    let owner: i32 = row.try_get("owner_user_id")?;
    let node: String = row.try_get("machine_id")?;
    let id: String = row.try_get("id")?;
    let mode = if kind == "device" { row.try_get::<String,_>("address_mode")? } else { "gateway".into() };
    let group_id = if kind == "device" { row.try_get::<Option<String>,_>("gateway_group_id")? } else { None };
    let is_child = group_id.is_some();
    let snapshot_result = stored_snapshot(row);
    let invalid_stored = snapshot_result.is_err();
    let snapshot = snapshot_result.as_ref().ok().and_then(|value| value.as_ref()).cloned();
    let instance_id: Option<String> = row.try_get("network_instance_id")?;
    let mut read_error = snapshot_result.err().map(|error| error.code);
    let mut accepted_parent = None;
    let mut current_id = instance_id.clone();
    let mut exclude_group = (kind == "gateway").then_some(id.clone());
    if let Some(group) = group_id.as_deref() {
        match subject_row(connection, owner, &node, "gateway", group).await {
            Ok(parent) => {
                current_id = parent.try_get("network_instance_id")?;
                match stored_snapshot(&parent) {
                    Ok(value) => accepted_parent = value,
                    Err(error) => read_error = Some(error.code),
                }
                exclude_group = Some(group.to_owned());
            }
            Err(error) => read_error = Some(error.code),
        }
    }
    let current = if let Some(current_id) = current_id.as_deref() {
        match instances::get_candidate(connection, owner, &node, current_id, exclude_group.as_deref(), runtime).await {
            Ok(value) => value,
            Err(error) => { read_error = Some(error.code); None }
        }
    } else { None };
    let invalid_snapshot = Value::Null;
    let state_snapshot = if invalid_stored { Some(&invalid_snapshot) } else { snapshot.as_ref() };
    let state = if is_child && mode != "legacy_manual" {
        let rule: Option<String> = row.try_get("rule_key")?;
        bindings::child_state(state_snapshot, accepted_parent.as_ref(), current.as_ref(), &mode, rule.as_deref(), read_error.as_deref())
    } else {
        bindings::binding_state(state_snapshot, current.as_ref(), if kind == "gateway" { "gateway" } else { "ordinary" }, read_error.as_deref())
    };
    let can_use = current.as_ref().is_some_and(|candidate| candidate["selectable"].as_bool() == Some(true)
        && candidate["role"].as_str() == Some(if kind == "gateway" || is_child { "gateway" } else { "ordinary" }));
    let status = state["state"].as_str().unwrap_or("unavailable");
    Ok(json!({"state":status,"reason":state["reason"],"instance_id":instance_id,
        "snapshot":snapshot,"current":current,"can_attach":snapshot.is_none() && !is_child && read_error.is_none(),
        "can_refresh":snapshot.is_some() && !is_child && can_use,
        "can_edit_address":kind == "device" && !invalid_stored && can_use && !(mode == "legacy_manual" && !is_child)
            && (!is_child || bindings::binding_state(accepted_parent.as_ref(), current.as_ref(), "gateway", read_error.as_deref())["state"] == "current")}))
}

pub(super) struct PreparedAddress {
    pub spec: Value,
    pub snapshot: Value,
    pub addresses: Value,
    pub preview: Value,
    pub group_id: Option<String>,
    pub is_self: bool,
}

fn before_side(row: Option<&SqliteRow>, kind: &str) -> AssetResult<Value> {
    let Some(row) = row else { return Ok(Value::Null); };
    let snapshot = stored_snapshot(row)?;
    let overlay = if kind == "device" { row.try_get::<Option<String>,_>("overlay_ip")? }
        else { snapshot.as_ref().and_then(|value| value["ipv4"].as_str()).map(str::to_owned) };
    let lan = if kind == "device" { row.try_get::<Option<String>,_>("lan_ip")? } else { None };
    Ok(json!({"snapshot":snapshot,"overlay_ip":overlay,"lan_ip":lan}))
}

pub(super) async fn prepare_address(connection: &mut SqliteConnection, owner: i32, node: &str,
    subject: &Value, spec: &Value, runtime: &RuntimeObservations, allow_legacy_direct: bool) -> AssetResult<PreparedAddress> {
    ensure_node(connection, owner, node).await?;
    if subject["resource_kind"].as_str() != Some("device") {
        return Err(AssetError::field("subject", "Address previews belong to devices."));
    }
    let id = subject["id"].as_str();
    let old = match id { Some(id) => Some(subject_row(connection, owner, node, "device", id).await?), None => None };
    let spec = validation::address_spec(spec)?;
    let mode = spec["mode"].as_str().unwrap_or_default();
    let group_id = spec["gateway_group_id"].as_str().map(str::to_owned);
    let is_self = mode == "gateway_self";
    if let Some(old) = old.as_ref() {
        let original_group: Option<String> = old.try_get("gateway_group_id")?;
        if original_group != group_id || old.try_get::<bool,_>("is_gateway_self")? != is_self {
            return Err(AssetError::conflict("asset_ownership_fixed", "The device's gateway and self-device role are fixed for this operation."));
        }
        let old_instance: Option<String> = old.try_get("network_instance_id")?;
        if mode == "direct_instance" {
            if old.try_get::<String,_>("address_mode")? == "legacy_manual" && !allow_legacy_direct {
                return Err(AssetError::conflict("binding_attach_required", "Use the binding action to attach this legacy direct device."));
            }
            if old_instance.as_deref().is_some_and(|old| Some(old) != spec["instance_id"].as_str()) {
                return Err(AssetError::conflict("asset_ownership_fixed", "Changing the bound network instance is not supported."));
            }
        }
    }
    let (candidate, _accepted_parent) = if let Some(group) = group_id.as_deref() {
        let parent = subject_row(connection, owner, node, "gateway", group).await?;
        let accepted = stored_snapshot(&parent)?.ok_or_else(|| AssetError::conflict("binding_attach_required", "Bind the gateway before deriving its device addresses."))?;
        let instance: String = parent.try_get::<Option<String>,_>("network_instance_id")?
            .ok_or_else(|| AssetError::conflict("binding_attach_required", "Bind the gateway first."))?;
        let candidate = usable_candidate(connection, owner, node, &instance, "gateway", Some(group), runtime).await?;
        let state = bindings::binding_state(Some(&accepted), Some(&candidate), "gateway", None);
        if state["state"].as_str() != Some("current") {
            return Err(AssetError::conflict("instance_config_changed", "Review and accept the gateway's current configuration before updating device addresses."));
        }
        (candidate, Some(accepted))
    } else {
        (usable_candidate(connection, owner, node, spec["instance_id"].as_str().unwrap_or_default(), "ordinary", None, runtime).await?, None)
    };
    let snapshot = instances::snapshot(&candidate);
    let rules = snapshot["rules"].as_array().cloned().unwrap_or_default();
    let derived = address_derivation::derive_address(mode, snapshot["ipv4"].as_str().unwrap_or_default(),
        &rules, spec["rule_key"].as_str(), spec["host_offset"].as_u64(), spec["lan_ip"].as_str())?;
    let addresses = json!({"overlay_ip":derived["overlay_ip"],"lan_ip":derived["lan_ip"]});
    validate_relations(connection, owner, node, id.unwrap_or(""), &json!({"gateway_group_id":group_id,
        "lan_ip":addresses["lan_ip"],"is_gateway_self":is_self})).await?;
    let before = before_side(old.as_ref(), "device")?;
    let after = json!({"snapshot":snapshot,"overlay_ip":addresses["overlay_ip"],"lan_ip":addresses["lan_ip"]});
    let preview_version = bindings::preview_version(&json!({"owner_user_id":owner,"machine_id":node}), subject,
        &json!({"address_spec":spec,"before":before}), &snapshot);
    let preview = json!({"can_save":true,"preview_version":preview_version,"field_errors":{},
        "warnings":derived["warnings"],"addresses":addresses,"snapshot":snapshot,"before":before,
        "after":after,"affected_device_ids":[]});
    Ok(PreparedAddress { spec, snapshot, addresses, preview, group_id, is_self })
}

pub(super) async fn prepare_binding(connection: &mut SqliteConnection, owner: i32, node: &str,
    subject: &Value, selection: &Value, runtime: &RuntimeObservations) -> AssetResult<Value> {
    ensure_node(connection, owner, node).await?;
    let kind = subject["resource_kind"].as_str().unwrap_or_default();
    let id = subject["id"].as_str();
    let operation = selection["operation"].as_str().unwrap_or_default();
    if !matches!(operation, "attach" | "refresh") {
        return Err(AssetError::field("operation", "Choose attach or refresh."));
    }
    let instance_id = validation::required_id(selection, "instance_id")?;
    let old = match id { Some(id) => Some(subject_row(connection, owner, node, kind, id).await?), None => None };
    if old.is_none() && (operation != "attach" || kind != "gateway") {
        return Err(AssetError::field("subject", "Use an address preview for a new device."));
    }
    if let Some(old) = old.as_ref() {
        if kind == "device" && old.try_get::<Option<String>,_>("gateway_group_id")?.is_some() {
            return Err(AssetError::conflict("asset_ownership_fixed", "Gateway devices inherit their gateway instance; use the address action."));
        }
        let original: Option<String> = old.try_get("network_instance_id")?;
        if operation == "attach" && original.is_some() {
            return Err(AssetError::conflict("binding_already_attached", "This asset is already bound. Use refresh."));
        }
        if operation == "refresh" && original.as_deref() != Some(instance_id.as_str()) {
            return Err(AssetError::conflict("asset_ownership_fixed", "Refresh must keep the existing network instance."));
        }
    }
    let before = before_side(old.as_ref(), kind)?;
    let (snapshot, addresses, warnings) = if kind == "device" {
        let lan = if selection.get("lan_ip").is_some() { selection["lan_ip"].clone() }
            else { before["lan_ip"].clone() };
        let prepared = prepare_address(connection, owner, node, subject,
            &json!({"mode":"direct_instance","instance_id":instance_id,"lan_ip":lan}), runtime, true).await?;
        (prepared.snapshot, prepared.addresses, prepared.preview["warnings"].clone())
    } else {
        if selection.get("lan_ip").is_some_and(|value| !value.is_null()) {
            return Err(AssetError::field("lan_ip", "A gateway binding does not take a device LAN address."));
        }
        let candidate = usable_candidate(connection, owner, node, &instance_id, "gateway", id, runtime).await?;
        let snapshot = instances::snapshot(&candidate);
        let addresses = json!({"overlay_ip":snapshot["ipv4"],"lan_ip":null});
        (snapshot, addresses, json!([]))
    };
    let mut affected = Vec::<String>::new();
    if kind == "gateway" {
        if let Some(id) = id {
            let children = sqlx::query("SELECT * FROM asset_devices WHERE owner_user_id = ? AND machine_id = ? AND gateway_group_id = ? ORDER BY id")
                .bind(owner).bind(node).bind(id).fetch_all(&mut *connection).await?;
            for child in children {
                let stored = match stored_snapshot(&child) {
                    Ok(stored) => stored,
                    Err(_) => {
                        // Confirming the parent never rewrites a child. Keep an unreadable
                        // child visible for separate repair without blocking the parent.
                        affected.push(child.try_get("id")?);
                        continue;
                    }
                };
                let mode: String = child.try_get("address_mode")?;
                let rule: Option<String> = child.try_get("rule_key")?;
                let state = bindings::child_state(stored.as_ref(), Some(&snapshot), Some(&snapshot), &mode, rule.as_deref(), None);
                if state["state"].as_str() != Some("current") { affected.push(child.try_get("id")?); }
            }
        }
    }
    let after = json!({"snapshot":snapshot,"overlay_ip":addresses["overlay_ip"],"lan_ip":addresses["lan_ip"]});
    let normalized_selection = json!({"operation":operation,"instance_id":instance_id,"lan_ip":addresses["lan_ip"],"before":before});
    let preview_version = bindings::preview_version(&json!({"owner_user_id":owner,"machine_id":node}), subject, &normalized_selection, &snapshot);
    Ok(json!({"can_save":true,"preview_version":preview_version,"field_errors":{},"warnings":warnings,
        "addresses":addresses,"snapshot":snapshot,"before":before,"after":after,"affected_device_ids":affected}))
}

pub(super) async fn save_snapshot(connection: &mut SqliteConnection, owner: i32, node: &str, kind: &str, id: &str, snapshot: &Value) -> AssetResult<()> {
    let table = match kind { "gateway" => "asset_gateway_groups", "device" => "asset_devices", _ => return Err(AssetError::not_found()) };
    sqlx::query(&format!("UPDATE {table} SET network_instance_id = ?, config_record_id = ?, config_create_time = ?, config_fingerprint = ?, binding_snapshot_json = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?"))
        .bind(snapshot["instance_id"].as_str()).bind(snapshot["config_ref"]["id"].as_i64())
        .bind(snapshot["config_ref"]["create_time"].as_str()).bind(snapshot["config_fingerprint"].as_str())
        .bind(snapshot.to_string()).bind(id).bind(owner).bind(node).execute(&mut *connection).await?;
    if kind == "gateway" {
        sqlx::query("DELETE FROM asset_gateway_subnets WHERE group_id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).execute(&mut *connection).await?;
        let cidrs: BTreeSet<&str> = snapshot["rules"].as_array().into_iter().flatten()
            .filter_map(|rule| rule["real_cidr"].as_str()).collect();
        for cidr in cidrs {
            sqlx::query("INSERT INTO asset_gateway_subnets(group_id,owner_user_id,machine_id,cidr) VALUES(?,?,?,?)")
                .bind(id).bind(owner).bind(node).bind(cidr).execute(&mut *connection).await?;
        }
    }
    Ok(())
}

fn preview_result(result: AssetResult<Value>) -> AssetResult<Value> {
    match result {
        Err(error) if error.status == 422 => Ok(json!({"can_save":false,"preview_version":null,
            "field_errors":error.field_errors,"warnings":[],"addresses":null,"snapshot":null,
            "before":null,"after":null,"affected_device_ids":[]})),
        result => result,
    }
}

impl AssetsStore {
    pub(crate) async fn list_instance_candidates(&self, owner: i32, node: &str, role: &str, kind: Option<&str>, id: Option<&str>) -> AssetResult<Value> {
        if !matches!(role, "gateway" | "ordinary") { return Err(AssetError::field("role", "Choose gateway or ordinary.")); }
        let mut connection = self.pool.acquire().await?;
        ensure_node(&mut connection, owner, node).await?;
        match (kind, id) {
            (Some(kind), Some(id)) => { subject_row(&mut connection, owner, node, kind, id).await?; }
            (None, None) => {}
            _ => return Err(AssetError::field("subject_id", "Supply both subject_kind and subject_id.")),
        }
        let excluded = if kind == Some("gateway") { id } else { None };
        Ok(json!({"items":instances::list_candidates(&mut connection, owner, node, role, excluded, &self.runtime_observations).await?}))
    }

    pub(crate) async fn preview_instance_address(&self, owner: i32, node: &str, body: Value) -> AssetResult<Value> {
        let result = async {
            validation::keys(&body, &["subject", "address_spec"])?;
            let subject = subject(&body)?;
            let mut connection = self.pool.acquire().await?;
            Ok(prepare_address(&mut connection, owner, node, &subject, &body["address_spec"], &self.runtime_observations, false).await?.preview)
        }.await;
        preview_result(result)
    }

    pub(crate) async fn preview_binding(&self, owner: i32, node: &str, body: Value) -> AssetResult<Value> {
        let result = async {
            validation::keys(&body, &["subject", "operation", "instance_id", "lan_ip"])?;
            let subject = subject(&body)?;
            let mut connection = self.pool.acquire().await?;
            prepare_binding(&mut connection, owner, node, &subject, &body, &self.runtime_observations).await
        }.await;
        preview_result(result)
    }

    pub(crate) async fn apply_binding(&self, actor: i32, owner: i32, node: &str, kind: &str, id: &str, body: Value) -> AssetResult<Value> {
        validation::keys(&body, &["request_id", "operation", "instance_id", "lan_ip", "preview_version"])?;
        let instance_id = validation::required_id(&body, "instance_id")?;
        let operation_kind = validation::required_string(&body, "operation", 16)?;
        let payload = json!({"operation":operation_kind,"instance_id":instance_id,"lan_ip":body.get("lan_ip"),
            "lan_ip_present":body.get("lan_ip").is_some(),"preview_version":version(&body)?});
        let operation = Operation::new(actor, owner, "apply_binding", json!({"machine_id":node,"kind":kind,"id":id}), &body, &payload)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(response) = operation.replay(&mut transaction).await? { transaction.commit().await?; return Ok(response); }
        let subject = json!({"resource_kind":kind,"id":id});
        let preview = prepare_binding(&mut transaction, owner, node, &subject, &body, &self.runtime_observations).await?;
        assert_version(&body, &preview)?;
        save_snapshot(&mut transaction, owner, node, kind, id, &preview["snapshot"]).await?;
        if kind == "device" {
            sqlx::query("UPDATE asset_devices SET address_mode = 'direct_instance', overlay_ip = ?, lan_ip = ?, rule_key = NULL, host_offset = NULL, updated_at = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
                .bind(preview["addresses"]["overlay_ip"].as_str()).bind(preview["addresses"]["lan_ip"].as_str())
                .bind(now()).bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
            publications::sync_device_address(&mut transaction, owner, node, id).await?;
        } else {
            sqlx::query("UPDATE asset_gateway_groups SET updated_at = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
                .bind(now()).bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        }
        let row = subject_row(&mut transaction, owner, node, kind, id).await?;
        let response = if kind == "gateway" { group_json(&mut transaction, &row, &self.runtime_observations).await? }
            else { device_json(&mut transaction, &row, &self.runtime_observations).await? };
        operation.finish(&mut transaction, if kind == "gateway" { "group" } else { "device" }, id, &response).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn apply_address(&self, actor: i32, owner: i32, node: &str, id: &str, body: Value) -> AssetResult<Value> {
        validation::keys(&body, &["request_id", "address_spec", "preview_version"])?;
        let spec = validation::address_spec(&body["address_spec"])?;
        let payload = json!({"address_spec":spec,"preview_version":version(&body)?});
        let operation = Operation::new(actor, owner, "apply_address", json!({"machine_id":node,"id":id}), &body, &payload)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(response) = operation.replay(&mut transaction).await? { transaction.commit().await?; return Ok(response); }
        let prepared = prepare_address(&mut transaction, owner, node, &json!({"resource_kind":"device","id":id}), &spec, &self.runtime_observations, false).await?;
        assert_version(&body, &prepared.preview)?;
        sqlx::query("UPDATE asset_devices SET overlay_ip = ?, lan_ip = ?, address_mode = ?, rule_key = ?, host_offset = ?, updated_at = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(prepared.addresses["overlay_ip"].as_str()).bind(prepared.addresses["lan_ip"].as_str())
            .bind(spec["mode"].as_str()).bind(spec["rule_key"].as_str()).bind(spec["host_offset"].as_i64())
            .bind(now()).bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        save_snapshot(&mut transaction, owner, node, "device", id, &prepared.snapshot).await?;
        publications::sync_device_address(&mut transaction, owner, node, id).await?;
        let row = subject_row(&mut transaction, owner, node, "device", id).await?;
        let response = device_json(&mut transaction, &row, &self.runtime_observations).await?;
        operation.finish(&mut transaction, "device", id, &response).await?;
        transaction.commit().await?;
        Ok(response)
    }
}
