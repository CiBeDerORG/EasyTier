//! Evidence of externally missing Docker resources.
//! NEVER delete an application, service, mapping, volume or ownership record
//! from these observations. An Agent-authenticated, complete scan is required
//! for each absence. Only M4 can supply durable, exclusive ownership proof.
use crate::{asset_agent::Error, db::Db};
use serde_json::{Value, json};
use sqlx::{Acquire, Row};
use std::collections::{BTreeMap, BTreeSet};

const MAX_RESOURCES: usize = 10_000;
const MAX_ABSENCES: usize = 10_000;
const CONFIRM_AFTER: i64 = 2;

fn resource_map(snapshot: &Value, expected_agent: &str) -> Result<BTreeMap<String, Value>, Error> {
    if snapshot["complete"] != true
        || snapshot["status"] != "ok"
        || snapshot["agent_id"].as_str() != Some(expected_agent)
        || snapshot["observed_at"].as_str().is_none()
    {
        return Err(Error::Remote);
    }
    let resources = snapshot["resources"].as_array().ok_or(Error::Remote)?;
    if resources.len() > MAX_RESOURCES {
        return Err(Error::Remote);
    }
    let mut map = BTreeMap::new();
    for item in resources {
        let id = item["resource_id"].as_str().ok_or(Error::Remote)?;
        if id.len() > 256
            || !(id.starts_with("compose:") || id.starts_with("container:"))
            || id.ends_with(':')
            || item["containers"].as_array().is_none()
        {
            return Err(Error::Remote);
        }
        let containers = item["containers"].as_array().ok_or(Error::Remote)?;
        if containers.is_empty() || containers.len() > 1_000 {
            return Err(Error::Remote);
        }
        let mut members = BTreeSet::new();
        for member in containers {
            let member_id = member["container_id"].as_str().ok_or(Error::Remote)?;
            if member_id.is_empty() || member_id.len() > 128 || !members.insert(member_id) {
                return Err(Error::Remote);
            }
        }
        if map.insert(id.to_owned(), item.clone()).is_some() {
            return Err(Error::Remote);
        }
    }
    Ok(map)
}

#[derive(Clone)]
struct Absence {
    id: String,
    last_resource: Value,
    last_seen_at: String,
    first_missing_at: String,
    count: i64,
}

fn verdict(row: &Absence) -> Value {
    // Two scans prove only an observed absence, not ownership or permission
    // to remove any linked services or public port publications.
    json!({
        "resource_id":row.id,
        "last_resource":row.last_resource,
        "last_seen_at":row.last_seen_at,
        "first_missing_at":row.first_missing_at,
        "consecutive_complete_absences":row.count,
        "state":if row.count >= CONFIRM_AFTER { "confirmed_missing" } else { "needs_recheck" },
        "cleanup_allowed":false,
        "cleanup_blocked_reason":"Exclusive Agent-managed application and service/publication ownership is not verified"
    })
}

// Reconcile with the last successful inventory IN A SINGLE DATABASE TRANSACTION.
// Missing assets are staged, never destroyed; incomplete/offline scans MUST NOT
// call this function. A resource that reappears clears its pending absence.
pub(crate) async fn reconcile_and_cache(
    db: &Db,
    device_id: &str,
    agent: &str,
    current: &Value,
) -> Result<Vec<Value>, Error> {
    let present = resource_map(current, agent)?;
    let observed_at = current["observed_at"].as_str().ok_or(Error::Remote)?;
    let pool = db.inner();
    let mut tx = pool.begin().await.map_err(|_| Error::Storage)?;

    let previous: Option<(String, String)> = sqlx::query_as(
        "SELECT agent_id,snapshot_json FROM asset_agent_docker_snapshots WHERE asset_device_id=?",
    )
    .bind(device_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| Error::Storage)?;
    let previous_map = if let Some((previous_agent, raw)) = previous {
        if previous_agent == agent {
            let parsed: Value = serde_json::from_str(&raw).map_err(|_| Error::Storage)?;
            let previous_time = parsed["observed_at"].as_str().ok_or(Error::Storage)?.to_owned();
            Some((previous_time, resource_map(&parsed, agent).map_err(|_| Error::Storage)?))
        } else {
            // Different Agent UUIDs must never share disappearance evidence.
            sqlx::query("DELETE FROM asset_agent_docker_absences WHERE asset_device_id=?")
                .bind(device_id).execute(&mut *tx).await.map_err(|_| Error::Storage)?;
            None
        }
    } else {
        None
    };

    let rows = sqlx::query(
        "SELECT resource_id,last_resource_json,last_seen_at,first_missing_at,consecutive_complete_absences FROM asset_agent_docker_absences WHERE asset_device_id=? AND agent_id=?",
    )
    .bind(device_id).bind(agent)
    .fetch_all(&mut *tx).await.map_err(|_| Error::Storage)?;
    if rows.len() > MAX_ABSENCES {
        return Err(Error::Storage);
    }
    let mut absences = BTreeMap::<String, Absence>::new();
    for row in rows {
        let id: String = row.try_get("resource_id").map_err(|_| Error::Storage)?;
        let raw: String = row.try_get("last_resource_json").map_err(|_| Error::Storage)?;
        let last_resource = serde_json::from_str::<Value>(&raw).map_err(|_| Error::Storage)?;
        absences.insert(id.clone(), Absence {
            id,
            last_resource,
            last_seen_at: row.try_get("last_seen_at").map_err(|_| Error::Storage)?,
            first_missing_at: row.try_get("first_missing_at").map_err(|_| Error::Storage)?,
            count: row.try_get("consecutive_complete_absences").map_err(|_| Error::Storage)?,
        });
    }

    // Restored resources have no deletion candidate, even if they had
    // previously reached confirmed_missing.
    for id in present.keys() {
        if absences.remove(id).is_some() {
            sqlx::query(
                "DELETE FROM asset_agent_docker_absences WHERE asset_device_id=? AND agent_id=? AND resource_id=?",
            )
            .bind(device_id).bind(agent).bind(id)
            .execute(&mut *tx).await.map_err(|_| Error::Storage)?;
        }
    }
    // A resource missing from the immediately previous complete inventory
    // enters the evidence table. Existing evidence survives as the snapshot
    // advances, so the second complete scan can confirm it.
    if let Some((old_seen_at, ref old)) = previous_map {
        for (id, item) in old {
            if !present.contains_key(id) && !absences.contains_key(id) {
                absences.insert(id.clone(), Absence {
                    id: id.clone(), last_resource: item.clone(),
                    last_seen_at: old_seen_at.clone(),
                    first_missing_at: observed_at.to_owned(), count: 0,
                });
            }
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut verdicts = Vec::new();
    for row in absences.values_mut() {
        row.count = (row.count + 1).min(1_000_000);
        sqlx::query(
            "INSERT INTO asset_agent_docker_absences (asset_device_id,agent_id,resource_id,last_resource_json,last_seen_at,first_missing_at,last_missing_at,consecutive_complete_absences) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(asset_device_id,agent_id,resource_id) DO UPDATE SET last_missing_at=excluded.last_missing_at,consecutive_complete_absences=excluded.consecutive_complete_absences",
        )
        .bind(device_id).bind(agent).bind(&row.id)
        .bind(row.last_resource.to_string())
        .bind(&row.last_seen_at).bind(&row.first_missing_at).bind(&now).bind(row.count)
        .execute(&mut *tx).await.map_err(|_| Error::Storage)?;
        verdicts.push(verdict(row));
    }
    // The successful snapshot and the absence evidence advance atomically.
    sqlx::query(
        "INSERT INTO asset_agent_docker_snapshots(asset_device_id,agent_id,snapshot_json,observed_at) VALUES(?,?,?,?) ON CONFLICT(asset_device_id) DO UPDATE SET agent_id=excluded.agent_id,snapshot_json=excluded.snapshot_json,observed_at=excluded.observed_at",
    )
    .bind(device_id).bind(agent).bind(current.to_string()).bind(observed_at)
    .execute(&mut *tx).await.map_err(|_| Error::Storage)?;
    tx.commit().await.map_err(|_| Error::Storage)?;
    Ok(verdicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(items: Vec<Value>, complete: bool) -> Value {
        json!({"agent_id":"agent-a","observed_at":"2026-10-28T12:00:00Z",
            "status":"ok","complete":complete,"resources":items})
    }
    fn resource(id: &str, container: &str, state: &str) -> Value {
        json!({"resource_id":id,"kind":"compose_project","name":"app",
            "state":state,"containers":[{"container_id":container,"state":state}]})
    }
    #[test]
    fn incomplete_offline_or_malformed_cannot_be_evidence() {
        assert!(resource_map(&snapshot(vec![], false), "agent-a").is_err());
        assert!(resource_map(&snapshot(vec![], true), "other-agent").is_err());
        let mut invalid = snapshot(vec![], true);
        invalid["status"] = json!("error");
        assert!(resource_map(&invalid, "agent-a").is_err());
        assert!(resource_map(&snapshot(vec![resource("compose:a", "one", "running"),
            resource("compose:a", "two", "running")], true), "agent-a").is_err());
        assert!(resource_map(&snapshot(vec![resource("compose:a", "one", "running"),
            resource("compose:b", "one", "exited")], true), "agent-a").is_ok());
    }
    #[test]
    fn absence_verdict_never_authorizes_unowned_service_cleanup() {
        let mut row = Absence { id:"compose:app".into(),
            last_resource:resource("compose:app","web","exited"),
            last_seen_at:"2026-10-28T11:00:00Z".into(),
            first_missing_at:"2026-10-28T12:00:00Z".into(), count:1 };
        let pending = verdict(&row);
        assert_eq!(pending["state"], "needs_recheck");
        assert_eq!(pending["cleanup_allowed"], false);
        row.count = 2;
        let confirmed = verdict(&row);
        assert_eq!(confirmed["state"], "confirmed_missing");
        assert_eq!(confirmed["cleanup_allowed"], false);
    }
    #[test]
    fn stopped_and_running_resources_remain_present() {
        let stopped = resource("compose:sub2api", "db", "exited");
        let running = resource("compose:web", "web", "running");
        let entries = resource_map(&snapshot(vec![stopped,running],true),"agent-a").unwrap();
        assert_eq!(entries.len(), 2);
    }
}
