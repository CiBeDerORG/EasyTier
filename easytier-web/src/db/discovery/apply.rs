use super::super::assets::{binding_ops, publications};
use super::preview::device_version;
use super::*;
use std::collections::BTreeSet;

fn text_field<'a>(v: &'a Value, key: &str, max: usize) -> Option<&'a str> {
    v[key]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.chars().count() <= max)
}
impl Store {
    pub async fn apply_rows(&self, actor: i32, g: &Gateway, body: &Value) -> AssetResult<Value> {
        let op = body["preview_id"]
            .as_str()
            .ok_or_else(|| AssetError::field("preview_id", "Select a saved preview."))?;
        let scope = format!("apply:{op}");
        if let Some(r) = self.replay(actor, g.owner, &scope, body).await? {
            return Ok(r);
        }
        let operation = self.operation(g, op).await?;
        if body["preview_revision"] != operation["revision"] {
            return Err(conflict("Preview version changed. Reload the operation."));
        }
        if operation["kind"] != "devices" {
            return Err(conflict("This operation is not a device preview."));
        }
        let rows = body["rows"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 256)
            .ok_or_else(|| AssetError::field("rows", "Choose 1–256 preview rows."))?;
        let mut ids = BTreeSet::new();
        let mut targets = BTreeSet::new();
        let mut ips = BTreeSet::new();
        for row in rows {
            let rid = row["row_id"].as_str().unwrap_or("");
            if !ids.insert(rid) {
                return Err(conflict("A preview row was selected more than once."));
            }
            if row["action"] == "skip" {
                continue;
            }
            if let Some(target) = row["target_device_id"].as_str() {
                if !targets.insert(target) {
                    return Err(conflict("Two rows select the same target device."));
                }
            }
            let p = operation["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["row_id"] == rid)
                .ok_or_else(AssetError::not_found)?;
            if let Some(ip) = p["ipv4"].as_str() {
                if !ips.insert(ip) {
                    return Err(conflict("Two rows claim the same IPv4 address."));
                }
            }
        }
        let mut results = Vec::new();
        for row in rows {
            let result = match self.apply_row(g, op, row).await {
                Ok(v) => v,
                Err(e) => {
                    let mut v = error_value(e);
                    v["row_id"] = row["row_id"].clone();
                    v
                }
            };
            results.push(result);
        }
        let result = json!({"operation_id":op,"rows":results,"originating_operation_id":operation["originating_operation_id"]});
        self.remember(actor, g.owner, &scope, body, &result).await?;
        Ok(result)
    }
    pub(super) async fn apply_row(
        &self,
        g: &Gateway,
        op: &str,
        choice: &Value,
    ) -> AssetResult<Value> {
        let rid = choice["row_id"]
            .as_str()
            .ok_or_else(AssetError::not_found)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let saved=sqlx::query("SELECT preview_json,applied_json FROM discovery_preview_rows WHERE operation_id=? AND row_id=?").bind(op).bind(rid).fetch_optional(&mut *tx).await?.ok_or_else(AssetError::not_found)?;
        if let Some(saved) = saved.try_get::<Option<String>, _>("applied_json")? {
            let saved = parse(&saved)?;
            if saved["choice"] != *choice {
                return Err(conflict(
                    "This row was already applied with a different choice.",
                ));
            }
            return Ok(saved["result"].clone());
        }
        self.assert_operation(&mut tx, g, op).await?;
        let cursor: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq),0) FROM discovery_endpoint_events")
                .fetch_one(&mut *tx)
                .await?;
        let mut preview = parse(&saved.try_get::<String, _>("preview_json")?)?;
        let action = choice["action"].as_str().unwrap_or("");
        let result = if action == "skip" {
            json!({"row_id":rid,"state":"skipped"})
        } else {
            if !matches!(action, "create" | "update" | "replace") {
                return Err(AssetError::field(
                    "action",
                    "Choose create, update, replace or skip.",
                ));
            }
            if !preview["blocked_reason"].is_null() {
                return Err(conflict(
                    "This row needs a new discovery or an address-rule correction.",
                ));
            }
            let target = choice["target_device_id"].as_str();
            let selected = if action == "create" {
                if target.is_some() {
                    return Err(conflict("Create cannot select an existing device."));
                }
                None
            } else {
                let target = target.ok_or_else(|| {
                    AssetError::field(
                        "target_device_id",
                        "Select an existing device from this gateway group.",
                    )
                })?;
                let c = preview["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["id"] == target)
                    .ok_or_else(AssetError::not_found)?;
                if c["version"].as_str() != Some(device_version(&mut tx, target).await?.as_str()) {
                    return Err(conflict(
                        "The selected device, services or publication changed after preview.",
                    ));
                }
                Some(target)
            };
            // Existing candidates matching this IP must still match the reviewed version.
            for c in preview["matched"].as_array().into_iter().flatten() {
                let id = c["id"].as_str().unwrap();
                if c["version"].as_str() != Some(device_version(&mut tx, id).await?.as_str()) {
                    return Err(conflict("The matching device changed after preview."));
                }
            }
            let prepared = binding_ops::prepare_address(
                &mut tx,
                g.owner,
                &g.node,
                &json!({"resource_kind":"device","id":selected}),
                &preview["address_spec"],
                &self.observations,
                false,
            )
            .await?;
            let input = &preview["input"];
            let name = text_field(input, "hostname", 128)
                .or_else(|| text_field(input, "name", 128))
                .unwrap_or(preview["ipv4"].as_str().unwrap());
            let model = text_field(input, "model", 128);
            let id = if action == "update" {
                selected.unwrap().to_owned()
            } else {
                Uuid::new_v4().to_string()
            };
            let mut discovery = input.clone();
            if action == "update" {
                let old: Option<String> =
                    sqlx::query_scalar("SELECT discovery_json FROM asset_devices WHERE id=?")
                        .bind(&id)
                        .fetch_one(&mut *tx)
                        .await?;
                if let Some(old) = old {
                    let mut old = parse(&old)?;
                    for (key, value) in input.as_object().into_iter().flatten() {
                        if !value.is_null() && value != "" && value != &json!([]) {
                            old[key] = value.clone();
                        }
                    }
                    discovery = old;
                }
                sqlx::query("UPDATE asset_devices SET name=CASE WHEN display_name_manual=0 THEN ? ELSE name END,model=COALESCE(?,model),overlay_ip=?,lan_ip=?,address_mode='gateway_rule',rule_key=?,host_offset=?,discovery_json=?,updated_at=? WHERE id=? AND owner_user_id=? AND gateway_group_id=? AND is_gateway_self=0")
                    .bind(name).bind(model).bind(prepared.addresses["overlay_ip"].as_str()).bind(prepared.addresses["lan_ip"].as_str()).bind(prepared.spec["rule_key"].as_str()).bind(prepared.spec["host_offset"].as_i64()).bind(discovery.to_string()).bind(now()).bind(&id).bind(g.owner).bind(&g.group).execute(&mut *tx).await?;
            } else {
                if let Some(old) = selected {
                    sqlx::query("UPDATE asset_devices SET lan_ip=NULL WHERE id=?")
                        .bind(old)
                        .execute(&mut *tx)
                        .await?;
                }
                sqlx::query("INSERT INTO asset_devices(id,owner_user_id,machine_id,gateway_group_id,name,model,overlay_ip,lan_ip,address_mode,rule_key,host_offset,discovery_json,display_name_manual,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'gateway_rule',?,?,?,0,?,?)")
                    .bind(&id).bind(g.owner).bind(&g.node).bind(&g.group).bind(name).bind(model).bind(prepared.addresses["overlay_ip"].as_str()).bind(prepared.addresses["lan_ip"].as_str()).bind(prepared.spec["rule_key"].as_str()).bind(prepared.spec["host_offset"].as_i64()).bind(discovery.to_string()).bind(now()).bind(now()).execute(&mut *tx).await?;
            }
            binding_ops::save_snapshot(
                &mut tx,
                g.owner,
                &g.node,
                "device",
                &id,
                &prepared.snapshot,
            )
            .await?;
            if action == "replace" {
                let old = selected.unwrap();
                let keep = choice["keep_service_ids"].as_array().ok_or_else(|| {
                    AssetError::field(
                        "keep_service_ids",
                        "Explicitly select the services to retain, or an empty list.",
                    )
                })?;
                let mut unique = BTreeSet::new();
                for s in keep {
                    let s = s.as_str().ok_or_else(AssetError::not_found)?;
                    if !unique.insert(s) {
                        return Err(conflict("Duplicate retained service."));
                    }
                }
                let all: Vec<String> =
                    sqlx::query_scalar("SELECT id FROM asset_services WHERE device_id=?")
                        .bind(old)
                        .fetch_all(&mut *tx)
                        .await?;
                if unique.iter().any(|id| !all.iter().any(|a| a == id)) {
                    return Err(conflict(
                        "A retained service no longer belongs to the replaced device.",
                    ));
                }
                sqlx::query("PRAGMA defer_foreign_keys=ON")
                    .execute(&mut *tx)
                    .await?;
                for service in all {
                    if unique.contains(service.as_str()) {
                        // Retained ADB services keep phone classification, never their old task identity.
                        sqlx::query("UPDATE asset_devices SET device_type_id=(SELECT device_type_id FROM asset_devices WHERE id=?) WHERE id=? AND EXISTS(SELECT 1 FROM asset_services WHERE id=? AND service_type='adb')").bind(old).bind(&id).bind(&service).execute(&mut *tx).await?;
                        for table in [
                            "asset_services",
                            "asset_service_bindings",
                            "asset_service_publications",
                        ] {
                            let key = if table == "asset_services" {
                                "id"
                            } else {
                                "service_id"
                            };
                            sqlx::query(&format!(
                                "UPDATE {table} SET device_id=? WHERE {key}=? AND device_id=?"
                            ))
                            .bind(&id)
                            .bind(&service)
                            .bind(old)
                            .execute(&mut *tx)
                            .await?;
                        }
                    } else {
                        publications::remove_service(&mut tx, g.owner, &g.node, &service).await?;
                        sqlx::query("DELETE FROM asset_services WHERE id=?")
                            .bind(&service)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
                sqlx::query("DELETE FROM asset_devices WHERE id=? AND owner_user_id=?")
                    .bind(old)
                    .bind(g.owner)
                    .execute(&mut *tx)
                    .await?;
            }
            if action == "replace" {
                sqlx::query("UPDATE discovery_endpoint_events SET predecessor_device_id=? WHERE owner_user_id=? AND device_id=?").bind(selected).bind(g.owner).bind(&id).execute(&mut *tx).await?;
            }
            publications::sync_device_address(&mut tx, g.owner, &g.node, &id).await?;
            sqlx::query("UPDATE discovery_endpoint_events SET operation_id=? WHERE operation_id IS NULL AND owner_user_id=? AND device_id IN (?,?) AND seq>?").bind(op).bind(g.owner).bind(&id).bind(selected).bind(cursor).execute(&mut *tx).await?;
            json!({"row_id":rid,"state":"saved","action":action,"device_id":id,"replaced_device_id":if action=="replace"{selected}else{None}})
        };
        preview["applied"] = result.clone();
        sqlx::query("UPDATE discovery_preview_rows SET applied_json=?,preview_json=? WHERE operation_id=? AND row_id=?").bind(json!({"choice":choice,"result":result}).to_string()).bind(preview.to_string()).bind(op).bind(rid).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
