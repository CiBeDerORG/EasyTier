//! Server-side work-in-progress scripts. This is deliberately NOT the
//! published automation_scripts table, and never starts Android actions.

use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};
use uuid::Uuid;

use super::{
    AutomationError, AutomationResult, AutomationStore, authorized, conflict, digest,
    not_found, now, parse_json, validation,
};

fn draft_json(row: &SqliteRow) -> AutomationResult<Value> {
    Ok(json!({
        "id": row.try_get::<String,_>("id")?,
        "owner_user_id": row.try_get::<i32,_>("owner_user_id")?,
        "name": row.try_get::<String,_>("name")?,
        "description": row.try_get::<String,_>("description")?,
        "steps": parse_json(&row.try_get::<String,_>("steps_json")?)?,
        "script_id": row.try_get::<Option<String>,_>("script_id")?,
        "script_revision": row.try_get::<Option<i64>,_>("script_revision")?,
        "revision": row.try_get::<i64,_>("revision")?,
        "created_at": row.try_get::<String,_>("created_at")?,
        "updated_at": row.try_get::<String,_>("updated_at")?
    }))
}

fn request_id(body: &Value) -> AutomationResult<String> {
    let id = validation::text(body, "request_id", 64, true)?;
    if !Uuid::parse_str(&id).is_ok_and(|uuid| uuid.to_string() == id) {
        return Err(AutomationError::field("request_id", "Use a canonical UUID for each write request."));
    }
    Ok(id)
}

fn revision(body: &Value) -> AutomationResult<i64> {
    body["revision"].as_i64().filter(|value| *value > 0)
        .ok_or_else(|| AutomationError::field("revision", "The latest positive draft revision is required."))
}

/// Confirmed draft steps use the SAME executable validation as published
/// scripts, with one explicit exception: an empty working draft is valid.
/// Secrets supplied when a script is run do not belong in a draft.
fn safe_steps(body: &Value) -> AutomationResult<Value> {
    let input = body.get("steps").ok_or_else(|| AutomationError::field("steps", "A steps array is required."))?;
    let steps = match input.as_array() {
        Some(items) if items.is_empty() => json!([]),
        _ => validation::steps(input)?,
    };
    if steps.as_array().is_some_and(|items| items.iter().any(|step|
        step["type"] == "input_text" && step.get("text").is_some())) {
        return Err(AutomationError::field("steps", "Draft input steps must use parameter names; do not save literal input or passwords."));
    }
    Ok(steps)
}

fn request_hash(operation: &str, id: Option<&str>, body: &Value) -> String {
    digest(&json!({"operation":operation, "id":id, "body":body}))
}

/// Called INSIDE the same BEGIN IMMEDIATE transaction as the mutation.
/// A historical retry returns the historical response, never writes again.
async fn replay(
    conn: &mut SqliteConnection, owner: i32, actor: i32, request: &str, hash: &str
) -> AutomationResult<Option<Value>> {
    let row = sqlx::query("SELECT request_hash,response_json FROM automation_draft_mutations WHERE owner_user_id=? AND actor_user_id=? AND request_id=?")
        .bind(owner).bind(actor).bind(request).fetch_optional(&mut *conn).await?;
    match row {
        None => Ok(None),
        Some(row) if row.try_get::<String,_>("request_hash")? == hash => {
            Ok(Some(parse_json(&row.try_get::<String,_>("response_json")?)?))
        }
        Some(_) => Err(conflict("draft_request_conflict", "This write request ID was used with different draft data."))
    }
}

async fn receipt(
    conn: &mut SqliteConnection, owner: i32, actor: i32, request: &str,
    hash: &str, id: &str, operation: &str, response: &Value
) -> AutomationResult<()> {
    sqlx::query("INSERT INTO automation_draft_mutations(owner_user_id,actor_user_id,request_id,request_hash,draft_id,operation,response_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(owner).bind(actor).bind(request).bind(hash).bind(id).bind(operation)
        // Keep only a compact operation receipt: discarded draft content
        // must not live forever in the idempotency table.
        .bind(json!({
            "id":id,
            "revision":response["revision"],
            "deleted":operation=="delete",
            "request_replayed":true,
        }).to_string()).bind(now()).execute(&mut *conn).await?;
    Ok(())
}

impl AutomationStore {
    pub(crate) async fn list_drafts(&self, actor: i32, owner: i32) -> AutomationResult<Value> {
        let mut conn = self.pool.acquire().await?;
        authorized(&mut conn, actor, owner).await?;
        let rows = sqlx::query("SELECT * FROM automation_drafts WHERE owner_user_id=? ORDER BY updated_at DESC,id DESC")
            .bind(owner).fetch_all(&mut *conn).await?;
        Ok(json!({"items":rows.iter().map(draft_json).collect::<AutomationResult<Vec<_>>>()?}))
    }

    pub(crate) async fn get_draft(&self, actor: i32, owner: i32, id: &str) -> AutomationResult<Value> {
        let mut conn = self.pool.acquire().await?;
        authorized(&mut conn, actor, owner).await?;
        let row = sqlx::query("SELECT * FROM automation_drafts WHERE owner_user_id=? AND id=?")
            .bind(owner).bind(id).fetch_optional(&mut *conn).await?.ok_or_else(not_found)?;
        draft_json(&row)
    }

    pub(crate) async fn save_draft(
        &self, actor: i32, owner: i32, id: Option<&str>, body: &Value
    ) -> AutomationResult<Value> {
        validation::keys(body, if id.is_some() {
            &["request_id", "revision", "name", "description", "steps", "script_id", "script_revision"]
        } else {
            &["request_id", "name", "description", "steps", "script_id", "script_revision"]
        })?;
        let request = request_id(body)?;
        let name = validation::text(body, "name", 128, false)?;
        let description = validation::text(body, "description", 2000, false)?;
        let steps = safe_steps(body)?;
        let script_id = match body.get("script_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if Uuid::parse_str(value).is_ok_and(|v|v.to_string()==*value) => Some(value.to_owned()),
            _ => return Err(AutomationError::field("script_id", "A source script must have a canonical UUID.")),
        };
        let script_revision = match body.get("script_revision") {
            None | Some(Value::Null) => None,
            Some(value) => value.as_i64().filter(|n|*n>0),
        };
        if script_id.is_some()!=script_revision.is_some() {
            return Err(AutomationError::field("script_revision", "Provide both source script and its positive revision, or neither."));
        }
        let expected = if id.is_some() { Some(revision(body)?) } else { None };
        let operation = if id.is_some() {"update"} else {"create"};
        let hash = request_hash(operation,id,body);
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        authorized(&mut tx,actor,owner).await?;
        if let Some(previous) = replay(&mut tx,owner,actor,&request,&hash).await? {
            return Ok(previous);
        }
        if let Some(script) = &script_id {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM automation_scripts WHERE owner_user_id=? AND id=?)")
                .bind(owner).bind(script).fetch_one(&mut *tx).await?;
            if !exists {return Err(not_found());}
        }
        let id = if let Some(id) = id {
            let changed = sqlx::query("UPDATE automation_drafts SET name=?,description=?,steps_json=?,script_id=?,script_revision=?,revision=revision+1,actor_user_id=?,updated_at=? WHERE owner_user_id=? AND id=? AND revision=?")
                .bind(&name).bind(&description).bind(steps.to_string())
                .bind(&script_id).bind(script_revision).bind(actor).bind(now()).bind(owner).bind(id).bind(expected).execute(&mut *tx).await?;
            if changed.rows_affected()==0 {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_drafts WHERE owner_user_id=? AND id=?)")
                    .bind(owner).bind(id).fetch_one(&mut *tx).await?;
                if !exists {return Err(not_found());}
                return Err(conflict("draft_revision_changed","The draft has changed in another page. Reload and resolve before saving."));
            }
            id.to_owned()
        } else {
            let new_id=Uuid::new_v4().to_string();
            let timestamp=now();
            sqlx::query("INSERT INTO automation_drafts(id,owner_user_id,actor_user_id,name,description,steps_json,script_id,script_revision,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,1,?,?)")
                .bind(&new_id).bind(owner).bind(actor).bind(&name).bind(&description)
                .bind(steps.to_string()).bind(&script_id).bind(script_revision).bind(&timestamp).bind(&timestamp).execute(&mut *tx).await?;
            new_id
        };
        let row=sqlx::query("SELECT * FROM automation_drafts WHERE owner_user_id=? AND id=?")
            .bind(owner).bind(&id).fetch_one(&mut *tx).await?;
        let result=draft_json(&row)?;
        receipt(&mut tx,owner,actor,&request,&hash,&id,operation,&result).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub(crate) async fn discard_draft(
        &self, actor: i32, owner: i32, id: &str, body: &Value
    ) -> AutomationResult<Value> {
        validation::keys(body,&["request_id","revision"])?;
        let request=request_id(body)?;
        let expected=revision(body)?;
        let hash=request_hash("delete",Some(id),body);
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        authorized(&mut tx,actor,owner).await?;
        if let Some(previous)=replay(&mut tx,owner,actor,&request,&hash).await? {
            return Ok(previous);
        }
        let changed=sqlx::query("DELETE FROM automation_drafts WHERE owner_user_id=? AND id=? AND revision=?")
            .bind(owner).bind(id).bind(expected).execute(&mut *tx).await?;
        if changed.rows_affected()==0 {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_drafts WHERE owner_user_id=? AND id=?)")
                .bind(owner).bind(id).fetch_one(&mut *tx).await?;
            if !exists {return Err(not_found());}
            return Err(conflict("draft_revision_changed","Draft deletion requires the latest revision; changes from another page were not discarded."));
        }
        let result=json!({"id":id,"deleted":true,"revision":expected});
        receipt(&mut tx,owner,actor,&request,&hash,id,"delete",&result).await?;
        tx.commit().await?;
        Ok(result)
    }
}
