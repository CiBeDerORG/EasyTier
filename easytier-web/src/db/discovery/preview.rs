use super::super::assets::binding_ops;
use super::*;
use std::collections::BTreeMap;

pub(super) async fn device_version(c: &mut SqliteConnection, id: &str) -> AssetResult<String> {
    // Hash all columns, including services/publications, without returning network secrets.
    let device:Option<String>=sqlx::query_scalar("SELECT json_object('updated',updated_at,'binding',binding_snapshot_json,'revision',endpoint_revision,'lan',lan_ip,'type',device_type_id,'name',name,'discovery',discovery_json) FROM asset_devices WHERE id=?").bind(id).fetch_optional(&mut *c).await?;
    let services:Vec<String>=sqlx::query_scalar("SELECT json_object('id',s.id,'updated',s.updated_at,'port',s.port,'type',s.service_type,'mode',s.connection_mode,'publication',p.rules_json,'public_updated',p.updated_at,'config',c.network_config) FROM asset_services s LEFT JOIN asset_service_publications p ON p.service_id=s.id LEFT JOIN user_running_network_configs c ON c.id=p.config_record_id WHERE s.device_id=? ORDER BY s.id").bind(id).fetch_all(&mut *c).await?;
    Ok(hash(&json!([device, services])))
}
pub(super) fn spec_for(g: &Gateway, ip: &str) -> AssetResult<Value> {
    let ip = ip
        .parse::<Ipv4Addr>()
        .map_err(|_| AssetError::field("ipv4", "One valid IPv4 address is required."))?;
    let mut matches = Vec::new();
    for rule in g.snapshot["rules"].as_array().into_iter().flatten() {
        let Some((net, prefix)) = rule["real_cidr"].as_str().and_then(|s| s.split_once('/')) else {
            continue;
        };
        let (Ok(net), Ok(prefix)) = (net.parse::<Ipv4Addr>(), prefix.parse::<u8>()) else {
            continue;
        };
        if prefix > 32 {
            continue;
        }
        let mask = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix)
        };
        if u32::from(ip) & mask == u32::from(net) {
            matches.push(json!({"mode":"gateway_rule","gateway_group_id":g.group,"rule_key":rule["rule_key"],"host_offset":u32::from(ip)-u32::from(net)}));
        }
    }
    if matches.len() != 1 {
        return Err(AssetError::conflict(
            "discovery_rule_ambiguous",
            "This IPv4 needs exactly one current gateway subnet rule.",
        ));
    }
    Ok(matches.remove(0))
}
impl Store {
    pub async fn preview(
        &self,
        g: &Gateway,
        op: &str,
        snapshot: &Value,
        only: Option<&[String]>,
    ) -> AssetResult<Value> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let current = self.gateway_on(&mut tx, g.owner, &g.id).await?;
        if current.context != g.context {
            return Err(conflict("Gateway settings changed during discovery."));
        }
        let devices=sqlx::query("SELECT id,name,lan_ip,discovery_json,is_gateway_self FROM asset_devices WHERE owner_user_id=? AND machine_id=? AND gateway_group_id=? ORDER BY id").bind(g.owner).bind(&g.node).bind(&g.group).fetch_all(&mut *tx).await?;
        let mut candidates = Vec::new();
        for d in &devices {
            let id: String = d.try_get("id")?;
            let services=sqlx::query("SELECT id,name,port,service_type FROM asset_services WHERE device_id=? ORDER BY id").bind(&id).fetch_all(&mut *tx).await?;
            let services=services.iter().map(|s|Ok(json!({"id":s.try_get::<String,_>("id")?,"name":s.try_get::<String,_>("name")?,"port":s.try_get::<i64,_>("port")?,"service_type":s.try_get::<String,_>("service_type")?}))).collect::<AssetResult<Vec<_>>>()?;
            candidates.push(json!({"id":id,"name":d.try_get::<String,_>("name")?,"lan_ip":d.try_get::<Option<String>,_>("lan_ip")?,"is_gateway_self":d.try_get::<bool,_>("is_gateway_self")?,"discovery":d.try_get::<Option<String>,_>("discovery_json")?.map(|s|parse(&s)).transpose()?,"version":device_version(&mut tx,&id).await?,"services":services}));
        }
        let inputs = snapshot["devices"].as_array().cloned().unwrap_or_default();
        if inputs.len() > 256 {
            return Err(AssetError::new(
                502,
                "discovery_limit",
                "Ordinary discovery exceeded 256 rows.",
            ));
        }
        let mut counts = BTreeMap::<String, usize>::new();
        for row in &inputs {
            if let Some(ip) = single_ip(row) {
                *counts.entry(ip).or_default() += 1;
            }
        }
        let mut rows = Vec::new();
        for input in inputs {
            let ip = single_ip(&input);
            if only.is_some_and(|ips| !ip.as_ref().is_some_and(|ip| ips.contains(ip))) {
                continue;
            }
            let row_id = Uuid::new_v4().to_string();
            let matched: Vec<_> = candidates
                .iter()
                .filter(|d| ip.is_some() && d["lan_ip"].as_str() == ip.as_deref())
                .cloned()
                .collect();
            let mut reason = None;
            let mut address = Value::Null;
            let mut spec = Value::Null;
            if !luci_client::usable(snapshot) {
                reason = Some("snapshot_not_current".to_owned());
            } else if ip.is_none() {
                reason = Some("single_ipv4_required".into());
            } else if counts.get(ip.as_ref().unwrap()).copied().unwrap_or(0) > 1 {
                reason = Some("duplicate_source_ipv4".into());
            } else if matched.iter().any(|v| v["is_gateway_self"] == true) {
                reason = Some("gateway_self_protected".into());
            } else {
                match spec_for(g, ip.as_deref().unwrap()) {
                    Err(e) => reason = Some(e.code),
                    Ok(value) => {
                        spec = value;
                        let subject = json!({"resource_kind":"device","id":matched.first().map(|v|v["id"].clone())});
                        match binding_ops::prepare_address(
                            &mut tx,
                            g.owner,
                            &g.node,
                            &subject,
                            &spec,
                            &self.observations,
                            false,
                        )
                        .await
                        {
                            Ok(prepared) => address = prepared.addresses,
                            Err(e) => reason = Some(e.code),
                        }
                    }
                }
            }
            let row = json!({"row_id":row_id,"input":input,"ipv4":ip,"matched":matched,"candidates":candidates.iter().filter(|c|c["is_gateway_self"]!=true).collect::<Vec<_>>(),"address_spec":spec,"addresses":address,"blocked_reason":reason,"suggested_action":if reason.is_some(){"skip"}else if matched.is_empty(){"create"}else{"update"},"observation":input["observation"],"warnings":snapshot["errors"],"applied":null});
            sqlx::query("INSERT INTO discovery_preview_rows(operation_id,row_id,input_json,preview_json) VALUES(?,?,?,?)").bind(op).bind(&row_id).bind(input.to_string()).bind(row.to_string()).execute(&mut *tx).await?;
            rows.push(row);
        }
        tx.commit().await?;
        let result = json!({"state":"preview","rows":rows.len(),"source_status":snapshot["status"],"truncated":snapshot["truncated"],"errors":snapshot["errors"]});
        self.finish(op, Some(snapshot), &result).await?;
        self.operation(g, op).await
    }
}
