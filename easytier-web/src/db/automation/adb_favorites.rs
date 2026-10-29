use serde_json::{Value,json};
use sqlx::Row;
use uuid::Uuid;
use super::{AutomationStore,AutomationError,AutomationResult,active_owner,conflict,now,validation};

impl AutomationStore {
    /// Account-scoped favorites survive browser cache removal and executor restarts.
    pub(crate) async fn list_adb_favorites(&self,owner:i32)->AutomationResult<Value> {
        let mut conn=self.pool.acquire().await?;
        active_owner(&mut conn,owner).await?;
        let rows=sqlx::query(
            "SELECT id,title,command,created_at,updated_at FROM automation_adb_favorites \
             WHERE owner_user_id=? ORDER BY updated_at DESC,id DESC LIMIT 100"
        ).bind(owner).fetch_all(&mut *conn).await?;
        let items=rows.iter().map(|row|->AutomationResult<Value> {
            Ok(json!({
                "id":row.try_get::<String,_>("id")?,
                "title":row.try_get::<String,_>("title")?,
                "command":row.try_get::<String,_>("command")?,
                "created_at":row.try_get::<String,_>("created_at")?,
                "updated_at":row.try_get::<String,_>("updated_at")?
            }))
        }).collect::<AutomationResult<Vec<_>>>()?;
        Ok(json!({"items":items}))
    }

    pub(crate) async fn save_adb_favorite(&self,owner:i32,body:&Value)->AutomationResult<Value> {
        validation::keys(body,&["title","command"])?;
        let title=validation::text(body,"title",256,true)?;
        if title.chars().count()>80 || title.chars().any(char::is_control) {
            return Err(AutomationError::field("title","Use a name of 1–80 characters."));
        }
        let command=validation::adb_shell_command(&body["command"])?;
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        active_owner(&mut tx,owner).await?;
        let existing:Option<String>=sqlx::query_scalar(
            "SELECT id FROM automation_adb_favorites WHERE owner_user_id=? AND command=?"
        ).bind(owner).bind(&command).fetch_optional(&mut *tx).await?;
        let id=if let Some(id)=existing {id} else {
            let count:i64=sqlx::query_scalar(
                "SELECT COUNT(*) FROM automation_adb_favorites WHERE owner_user_id=?"
            ).bind(owner).fetch_one(&mut *tx).await?;
            if count>=100 {
                return Err(conflict("adb_favorites_limit","每个账号最多保存 100 条常用 ADB 命令。"));
            }
            Uuid::new_v4().to_string()
        };
        let stamp=now();
        sqlx::query(
            "INSERT INTO automation_adb_favorites(id,owner_user_id,title,command,created_at,updated_at) \
             VALUES(?,?,?,?,?,?) ON CONFLICT(owner_user_id,command) \
             DO UPDATE SET title=excluded.title,updated_at=excluded.updated_at"
        ).bind(&id).bind(owner).bind(&title).bind(&command).bind(&stamp).bind(&stamp)
         .execute(&mut *tx).await?;
        let row=sqlx::query(
            "SELECT id,title,command,created_at,updated_at FROM automation_adb_favorites WHERE owner_user_id=? AND command=?"
        ).bind(owner).bind(&command).fetch_one(&mut *tx).await?;
        let value=json!({
            "id":row.try_get::<String,_>("id")?,
            "title":row.try_get::<String,_>("title")?,
            "command":row.try_get::<String,_>("command")?,
            "created_at":row.try_get::<String,_>("created_at")?,
            "updated_at":row.try_get::<String,_>("updated_at")?
        });
        tx.commit().await?;
        Ok(value)
    }

    pub(crate) async fn delete_adb_favorite(&self,owner:i32,id:&str)->AutomationResult<Value> {
        let mut tx=self.pool.begin_with("BEGIN IMMEDIATE").await?;
        active_owner(&mut tx,owner).await?;
        let deleted=sqlx::query(
            "DELETE FROM automation_adb_favorites WHERE owner_user_id=? AND id=?"
        ).bind(owner).bind(id).execute(&mut *tx).await?.rows_affected()==1;
        tx.commit().await?;
        Ok(json!({"deleted":deleted,"id":id}))
    }
}
