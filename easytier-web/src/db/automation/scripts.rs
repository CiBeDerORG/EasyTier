use serde_json::{Value,json};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use super::{AutomationStore,AutomationResult,AutomationError,active_owner,conflict,digest,not_found,now,parse_json,validation};

fn script_json(row:&SqliteRow)->AutomationResult<Value> {
    Ok(json!({"id":row.try_get::<String,_>("id")?,"name":row.try_get::<String,_>("name")?,
        "description":row.try_get::<String,_>("description")?,"revision":row.try_get::<i64,_>("revision")?,
        "steps":parse_json(&row.try_get::<String,_>("steps_json")?)?,
        "created_at":row.try_get::<String,_>("created_at")?,"updated_at":row.try_get::<String,_>("updated_at")?}))
}

impl AutomationStore {
    pub(crate) async fn list_scripts(&self,owner:i32)->AutomationResult<Value> {
        let mut conn=self.pool.acquire().await?;active_owner(&mut conn,owner).await?;
        let rows=sqlx::query("SELECT * FROM automation_scripts WHERE owner_user_id=? ORDER BY updated_at DESC,id DESC").bind(owner).fetch_all(&mut *conn).await?;
        Ok(json!({"items":rows.iter().map(script_json).collect::<AutomationResult<Vec<_>>>()?}))
    }

    pub(crate) async fn save_script(&self,owner:i32,id:Option<&str>,body:&Value)->AutomationResult<Value> {
        validation::keys(body,&["name","description","steps","revision","request_id"])?;
        let name=validation::text(body,"name",128,true)?;
        let description=validation::text(body,"description",2000,false)?;
        // Empty scripts can be saved/edited; create_run still requires 1–100 steps.
        let steps=validation::saved_steps(&body["steps"])?;
        let identity=if id.is_none() {
            match body.get("request_id") {
                Some(_) => {
                    let request=validation::text(body,"request_id",64,true)?;
                    if Uuid::parse_str(&request).is_err(){
                        return Err(AutomationError::field("request_id","Invalid create request UUID."));
                    }
                    Some((request,digest(&json!({
                        "name":name.clone(),"description":description.clone(),"steps":steps.clone()
                    }))))
                }
                None => None, // Only the internal legacy DB fixture uses this path.
            }
        } else {
            if body.get("request_id").is_some(){
                return Err(AutomationError::field("request_id","Updates use revision CAS, not a create UUID."));
            }
            None
        };
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;active_owner(&mut tx,owner).await?;
        if let Some((request,hash))=&identity {
            let old=sqlx::query("SELECT request_hash,response_json FROM automation_script_mutations WHERE owner_user_id=? AND request_id=?")
                .bind(owner).bind(request).fetch_optional(&mut *tx).await?;
            if let Some(old)=old {
                if old.try_get::<String,_>("request_hash")?!=*hash {
                    return Err(conflict("idempotency_conflict","This script creation UUID was already used with different contents."));
                }
                let value=parse_json(&old.try_get::<String,_>("response_json")?)?;
                tx.commit().await?;
                return Ok(value);
            }
        }
        let id=if let Some(id)=id {
            let revision=body["revision"].as_i64().filter(|v|*v>0).ok_or_else(||AutomationError::field("revision","The current draft revision is required."))?;
            let changed=sqlx::query("UPDATE automation_scripts SET name=?,description=?,steps_json=?,revision=revision+1,updated_at=? WHERE id=? AND owner_user_id=? AND revision=?")
                .bind(name).bind(description).bind(steps.to_string()).bind(now()).bind(id).bind(owner).bind(revision).execute(&mut *tx).await?;
            if changed.rows_affected()==0 {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_scripts WHERE id=? AND owner_user_id=?)").bind(id).bind(owner).fetch_one(&mut *tx).await?;
                if !exists{return Err(not_found());}
                return Err(AutomationError::new(409,"script_revision_changed","The draft was changed in another page. Refresh before saving."));
            }
            id.to_owned()
        } else {
            if body.get("revision").is_some(){return Err(AutomationError::field("revision","A new script does not have an existing revision."));}
            let id=Uuid::new_v4().to_string();let timestamp=now();
            sqlx::query("INSERT INTO automation_scripts(id,owner_user_id,name,description,revision,steps_json,created_at,updated_at) VALUES(?,?,?,?,1,?,?,?)")
                .bind(&id).bind(owner).bind(name).bind(description).bind(steps.to_string()).bind(&timestamp).bind(&timestamp).execute(&mut *tx).await?;
            id
        };
        let row=sqlx::query("SELECT * FROM automation_scripts WHERE id=? AND owner_user_id=?").bind(&id).bind(owner).fetch_one(&mut *tx).await?;
        let result=script_json(&row)?;
        if let Some((request,hash))=&identity {
            sqlx::query("INSERT INTO automation_script_mutations(owner_user_id,request_id,request_hash,script_id,response_json,created_at) VALUES(?,?,?,?,?,?)")
                .bind(owner).bind(request).bind(hash).bind(&id).bind(result.to_string()).bind(now())
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;Ok(result)
    }

    /// Read-only recovery of an uncertain POST, including after script deletion.
    /// This is the exact server receipt for one immutable owner/request UUID.
    pub(crate) async fn script_creation_receipt(&self,owner:i32,request:&str)->AutomationResult<Value> {
        let mut conn=self.pool.acquire().await?;
        active_owner(&mut conn,owner).await?;
        let raw:Option<String>=sqlx::query_scalar(
            "SELECT response_json FROM automation_script_mutations WHERE owner_user_id=? AND request_id=?"
        ).bind(owner).bind(request).fetch_optional(&mut *conn).await?;
        parse_json(&raw.ok_or_else(not_found)?)
    }

    /// T46: a versioned delete for floating workspaces. The previous
    /// DELETE endpoint stays for older clients, but all first-party UI now
    /// uses this CAS entry so a stale dialog cannot delete a newer revision.
    pub(crate) async fn delete_script_if_revision(&self,owner:i32,id:&str,revision:i64)->AutomationResult<Value> {
        if revision<1{return Err(AutomationError::field("revision","A positive script revision is required."));}
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        active_owner(&mut tx,owner).await?;
        let changed=sqlx::query("DELETE FROM automation_scripts WHERE id=? AND owner_user_id=? AND revision=?")
            .bind(id).bind(owner).bind(revision).execute(&mut *tx).await?;
        if changed.rows_affected()==0 {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_scripts WHERE id=? AND owner_user_id=?)")
                .bind(id).bind(owner).fetch_one(&mut *tx).await?;
            if !exists{return Err(not_found());}
            return Err(AutomationError::new(409,"script_revision_changed","The script revision changed before deletion. Refresh and review."));
        }
        tx.commit().await?;
        Ok(json!({"deleted":true,"id":id,"revision":revision}))
    }

    pub(crate) async fn delete_script(&self,owner:i32,id:&str)->AutomationResult<Value> {
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;active_owner(&mut tx,owner).await?;
        let changed=sqlx::query("DELETE FROM automation_scripts WHERE id=? AND owner_user_id=?").bind(id).bind(owner).execute(&mut *tx).await?;
        if changed.rows_affected()==0 {return Err(not_found());}
        tx.commit().await?;Ok(json!({"deleted":true}))
    }
}
