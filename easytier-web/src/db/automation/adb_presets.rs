use serde_json::{Value,json};
use super::{AutomationStore,AutomationError,AutomationResult,active_owner,now};

const BUILTIN_IDS:&[&str]=&[
    "home","back","recent","wake","screen-size","density","battery",
    "model","version","packages","screen-timeout",
];

fn validate_preset_id(id:&str)->AutomationResult<()> {
    if !BUILTIN_IDS.contains(&id) {
        return Err(AutomationError::field("id","Unknown built-in ADB command."));
    }
    Ok(())
}

impl AutomationStore {
    /// Built-in presets can be hidden per account without changing the global preset catalog.
    pub(crate) async fn list_hidden_adb_presets(&self,owner:i32)->AutomationResult<Value> {
        let mut conn=self.pool.acquire().await?;
        active_owner(&mut conn,owner).await?;
        let items:Vec<String>=sqlx::query_scalar(
            "SELECT preset_id FROM automation_adb_hidden_presets WHERE owner_user_id=? ORDER BY preset_id"
        ).bind(owner).fetch_all(&mut *conn).await?;
        Ok(json!({"items":items}))
    }

    pub(crate) async fn hide_adb_preset(&self,owner:i32,id:&str)->AutomationResult<Value> {
        validate_preset_id(id)?;
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        active_owner(&mut tx,owner).await?;
        sqlx::query(
            "INSERT INTO automation_adb_hidden_presets(owner_user_id,preset_id,created_at) VALUES(?,?,?) \
             ON CONFLICT(owner_user_id,preset_id) DO NOTHING"
        ).bind(owner).bind(id).bind(now()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"preset_id":id,"hidden":true}))
    }

    pub(crate) async fn restore_adb_preset(&self,owner:i32,id:&str)->AutomationResult<Value> {
        validate_preset_id(id)?;
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        active_owner(&mut tx,owner).await?;
        sqlx::query(
            "DELETE FROM automation_adb_hidden_presets WHERE owner_user_id=? AND preset_id=?"
        ).bind(owner).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"preset_id":id,"hidden":false}))
    }
}
