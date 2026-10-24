use super::super::assets::publications;
use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Only endpoints from the successful scan are candidates; no pairing ports or IPv6 fallback.
pub(crate) fn endpoints(snapshot: &Value) -> Vec<Value> {
    if !luci_client::current_adb(snapshot) {
        return Vec::new();
    }
    let mut entries = BTreeMap::<String, BTreeMap<u64, Value>>::new();
    for row in snapshot["devices"].as_array().into_iter().flatten() {
        if row["service"] != "_adb-tls-connect._tcp" || row["protocol"] != "IPv4" {
            continue;
        }
        let Some(ip) = row["address"]
            .as_str()
            .and_then(|s| s.parse::<Ipv4Addr>().ok())
            .filter(|ip| !ip.is_unspecified() && !ip.is_multicast() && !ip.is_broadcast())
        else {
            continue;
        };
        let Some(port) = row["port"].as_u64().filter(|p| (1..=65535).contains(p)) else {
            continue;
        };
        let ip = ip.to_string();
        let endpoint =
            json!({"endpoint_id":hash(&json!([ip,port])),"ip":ip,"port":port,"source":row});
        entries.entry(ip).or_default().insert(port, endpoint);
    }
    entries.into_iter().map(|(ip,ports)|json!({"row_id":hash(&json!(ip)),"ip":ip,"endpoints":ports.into_values().collect::<Vec<_>>()})).collect()
}
impl Store {
    pub async fn missing_adb_ips(&self, g: &Gateway, snapshot: &Value) -> AssetResult<Vec<String>> {
        let mut missing = Vec::new();
        for row in endpoints(snapshot) {
            let ip = row["ip"].as_str().unwrap();
            let found:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices WHERE owner_user_id=? AND machine_id=? AND gateway_group_id=? AND lan_ip=?)").bind(g.owner).bind(&g.node).bind(&g.group).bind(ip).fetch_one(&self.pool).await?;
            if !found {
                missing.push(ip.into());
            }
        }
        Ok(missing)
    }
    pub async fn reconcile_adb(
        &self,
        g: &Gateway,
        op: &str,
        choices: &[Value],
        _finalize_missing: bool,
    ) -> AssetResult<Value> {
        let operation = self.operation(g, op).await?;
        let snapshot = &operation["snapshot"];
        if operation["kind"] != "adb" {
            return Err(conflict("Select the original ADB discovery operation."));
        }
        if !luci_client::current_adb(snapshot) {
            return Ok(json!({"state":"ignored","code":"adb_snapshot_not_current","rows":[]}));
        }
        // A missing phone waits only while its own supplemental row is still reviewable.
        // Empty/failed supplemental scans and explicit skips are final for this snapshot.
        let mut pending_ips = BTreeSet::new();
        if let Some(supplement) = operation["result"]["supplement_preview_id"].as_str() {
            for raw in sqlx::query_scalar::<_,String>("SELECT preview_json FROM discovery_preview_rows WHERE operation_id=? AND applied_json IS NULL").bind(supplement).fetch_all(&self.pool).await?{
                let row=parse(&raw)?;
                if row["blocked_reason"].is_null(){if let Some(ip)=row["ipv4"].as_str(){pending_ips.insert(ip.to_owned());}}
            }
        }
        let missing = self.missing_adb_ips(g, snapshot).await?;
        let candidates = endpoints(snapshot);
        let mut unique = BTreeSet::new();
        for choice in choices {
            let row = choice["row_id"].as_str().unwrap_or("");
            if !unique.insert(row)
                || !candidates.iter().any(|c| {
                    c["row_id"] == row
                        && c["endpoints"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|e| e["endpoint_id"] == choice["selected_endpoint_id"])
                })
            {
                return Err(AssetError::field(
                    "choices",
                    "Choose one endpoint ID from each saved ADB row.",
                ));
            }
        }
        let mut results = Vec::new();
        for row in candidates {
            let previous = operation["result"]["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["row_id"] == row["row_id"]);
            if previous.is_some_and(|r| r["state"] == "saved" || r["state"] == "skipped") {
                results.push(previous.unwrap().clone());
                continue;
            }
            let choice = choices.iter().find(|c| c["row_id"] == row["row_id"]);
            let options = row["endpoints"].as_array().unwrap();
            let endpoint = if let Some(choice) = choice {
                options
                    .iter()
                    .find(|e| e["endpoint_id"] == choice["selected_endpoint_id"])
            } else if options.len() == 1 {
                options.first()
            } else {
                None
            };
            let result = if missing.iter().any(|ip| row["ip"] == ip.as_str()) {
                json!({"row_id":row["row_id"],"ip":row["ip"],"code":"adb_asset_missing","state":if pending_ips.contains(row["ip"].as_str().unwrap_or("")){"pending_device"}else{"skipped"}})
            } else if let Some(endpoint) = endpoint {
                match self.reconcile_endpoint(g, op, &row, endpoint).await {
                    Ok(v) => v,
                    Err(e) => {
                        let mut value = error_value(e);
                        value["row_id"] = row["row_id"].clone();
                        value["ip"] = row["ip"].clone();
                        if value["code"] == "adb_asset_missing" {
                            value["state"] =
                                json!(if pending_ips.contains(row["ip"].as_str().unwrap_or("")) {
                                    "pending_device"
                                } else {
                                    "skipped"
                                });
                        }
                        value
                    }
                }
            } else {
                json!({"row_id":row["row_id"],"ip":row["ip"],"state":"choose_endpoint","endpoints":options})
            };
            results.push(result);
        }
        let result = json!({"state":"recorded","rows":results,"supplement_preview_id":operation["result"]["supplement_preview_id"],"source_status":snapshot["status"],"errors":snapshot["errors"],"truncated":snapshot["truncated"]});
        self.finish(op, None, &result).await?;
        Ok(result)
    }
    async fn reconcile_endpoint(
        &self,
        g: &Gateway,
        op: &str,
        row: &Value,
        endpoint: &Value,
    ) -> AssetResult<Value> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.assert_operation(&mut tx, g, op).await?;
        let cursor: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq),0) FROM discovery_endpoint_events")
                .fetch_one(&mut *tx)
                .await?;
        let device=sqlx::query("SELECT id,is_gateway_self,device_type_id FROM asset_devices WHERE owner_user_id=? AND machine_id=? AND gateway_group_id=? AND lan_ip=?").bind(g.owner).bind(&g.node).bind(&g.group).bind(row["ip"].as_str()).fetch_optional(&mut *tx).await?.ok_or_else(||AssetError::conflict("adb_asset_missing","No ordinary device matches this IP in the gateway group."))?;
        if device.try_get::<bool, _>("is_gateway_self")? {
            return Err(AssetError::conflict(
                "gateway_self_protected",
                "The gateway itself cannot become a phone.",
            ));
        }
        let did: String = device.try_get("id")?;
        let port = endpoint["port"].as_i64().unwrap();
        let service=sqlx::query("SELECT id,port,connection_mode FROM asset_services WHERE device_id=? AND service_type='adb'").bind(&did).fetch_optional(&mut *tx).await?;
        let sid = service
            .as_ref()
            .map(|s| s.try_get::<String, _>("id"))
            .transpose()?
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_service_bindings WHERE device_id=? AND protocol='tcp' AND port=? AND service_id<>?)").bind(&did).bind(port).bind(&sid).fetch_one(&mut *tx).await?;
        if occupied {
            return Err(AssetError::conflict(
                "adb_port_in_use",
                "Another service uses this TCP port. Confirm its service type in the device service editor.",
            ));
        }
        let mut phone:Option<String>=sqlx::query_scalar("SELECT id FROM asset_device_types WHERE owner_user_id=? AND semantic_kind='phone' ORDER BY id LIMIT 1").bind(g.owner).fetch_optional(&mut *tx).await?;
        if phone.is_none() {
            let id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO asset_device_types(id,owner_user_id,name,semantic_kind,created_at,updated_at) VALUES(?,?,?,'phone',?,?)").bind(&id).bind(g.owner).bind(format!("手机 · {}",&id[..8])).bind(now()).bind(now()).execute(&mut *tx).await?;
            phone = Some(id);
        }
        // Preserve an existing confirmed phone subtype; never choose an unclassified name.
        sqlx::query("UPDATE asset_devices SET device_type_id=?,updated_at=? WHERE id=? AND NOT EXISTS(SELECT 1 FROM asset_device_types WHERE id=asset_devices.device_type_id AND semantic_kind='phone')").bind(phone).bind(now()).bind(&did).execute(&mut *tx).await?;
        let changed = service.as_ref().is_none_or(|s| {
            s.get::<i64, _>("port") != port || s.get::<String, _>("connection_mode") != "wireless"
        });
        if changed {
            if service.is_some() {
                sqlx::query("UPDATE asset_services SET port=?,connection_mode='wireless',updated_at=? WHERE id=?").bind(port).bind(now()).bind(&sid).execute(&mut *tx).await?;
                sqlx::query("DELETE FROM asset_service_bindings WHERE service_id=?")
                    .bind(&sid)
                    .execute(&mut *tx)
                    .await?;
            } else {
                sqlx::query("INSERT INTO asset_services(id,owner_user_id,machine_id,device_id,name,port,service_type,connection_mode,created_at,updated_at) VALUES(?,?,?,?,'ADB',?,'adb','wireless',?,?)").bind(&sid).bind(g.owner).bind(&g.node).bind(&did).bind(port).bind(now()).bind(now()).execute(&mut *tx).await?;
            }
            sqlx::query("INSERT INTO asset_service_bindings(service_id,owner_user_id,machine_id,device_id,protocol,port) VALUES(?,?,?,?,'tcp',?)").bind(&sid).bind(g.owner).bind(&g.node).bind(&did).bind(port).execute(&mut *tx).await?;
            publications::sync_service(
                &mut tx,
                g.owner,
                &g.node,
                &did,
                &sid,
                &json!({"service_type":"adb","port":port,"protocols":["tcp"]}),
                None,
            )
            .await?;
        }
        sqlx::query("UPDATE discovery_endpoint_events SET operation_id=? WHERE operation_id IS NULL AND owner_user_id=? AND device_id=? AND seq>?").bind(op).bind(g.owner).bind(&did).bind(cursor).execute(&mut *tx).await?;
        let revision: i64 =
            sqlx::query_scalar("SELECT endpoint_revision FROM asset_devices WHERE id=?")
                .bind(&did)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(
            json!({"row_id":row["row_id"],"ip":row["ip"],"state":"saved","device_id":did,"adb_service_id":sid,"port":port,"endpoint_revision":revision,"changed":changed,"selected_endpoint_id":endpoint["endpoint_id"]}),
        )
    }
}
