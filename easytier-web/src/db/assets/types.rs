use serde_json::{Value, json};

use super::{AssetsStore, common::{AssetError, AssetResult, ensure_owner, ensure_type, now, type_json}, idempotency::Operation, validation};

impl AssetsStore {
    pub(crate) async fn list_types(&self, owner: i32) -> AssetResult<Value> {
        let mut connection = self.pool.acquire().await?;
        ensure_owner(&mut connection, owner).await?;
        let rows = sqlx::query("SELECT * FROM asset_device_types WHERE owner_user_id = ? ORDER BY name, id")
            .bind(owner).fetch_all(&mut *connection).await?;
        Ok(json!({"items":rows.iter().map(type_json).collect::<AssetResult<Vec<_>>>()?}))
    }

    pub(crate) async fn initialize_types(&self, owner: i32) -> AssetResult<Value> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_owner(&mut transaction, owner).await?;
        let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_catalog_state WHERE owner_user_id = ?)")
            .bind(owner).fetch_one(&mut *transaction).await?;
        if exists == 0 {
            let timestamp = now();
            for name in ["OpenWrt", "手机", "电脑", "NAS", "智能家居", "其他"] {
                sqlx::query("INSERT INTO asset_device_types(id,owner_user_id,name,created_at,updated_at) VALUES(?,?,?,?,?)")
                    .bind(uuid::Uuid::new_v4().to_string()).bind(owner).bind(name).bind(&timestamp).bind(&timestamp)
                    .execute(&mut *transaction).await?;
            }
            sqlx::query("INSERT INTO asset_catalog_state(owner_user_id,initialized_at) VALUES(?,?)")
                .bind(owner).bind(&timestamp).execute(&mut *transaction).await?;
        }
        let rows = sqlx::query("SELECT * FROM asset_device_types WHERE owner_user_id = ? ORDER BY name,id")
            .bind(owner).fetch_all(&mut *transaction).await?;
        let response = json!({"items":rows.iter().map(type_json).collect::<AssetResult<Vec<_>>>()?});
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn create_type(&self, actor: i32, owner: i32, body: Value) -> AssetResult<Value> {
        let payload = validation::type_payload(&body, true)?;
        let operation = Operation::new(actor, owner, "create_type", json!({"owner_user_id":owner}), &body, &payload)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(response) = operation.replay(&mut transaction).await? {
            transaction.commit().await?;
            return Ok(response);
        }
        ensure_owner(&mut transaction, owner).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        sqlx::query("INSERT INTO asset_device_types(id,owner_user_id,name,created_at,updated_at) VALUES(?,?,?,?,?)")
            .bind(&id).bind(owner).bind(payload["name"].as_str()).bind(&timestamp).bind(&timestamp)
            .execute(&mut *transaction).await?;
        let row = sqlx::query("SELECT * FROM asset_device_types WHERE id = ? AND owner_user_id = ?")
            .bind(&id).bind(owner).fetch_one(&mut *transaction).await?;
        let response = type_json(&row)?;
        operation.finish(&mut transaction, "type", &id, &response).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn update_type(&self, owner: i32, id: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::type_payload(&body, false)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_type(&mut transaction, owner, id).await?;
        sqlx::query("UPDATE asset_device_types SET name = ?, updated_at = ? WHERE id = ? AND owner_user_id = ?")
            .bind(payload["name"].as_str()).bind(now()).bind(id).bind(owner).execute(&mut *transaction).await?;
        let row = sqlx::query("SELECT * FROM asset_device_types WHERE id = ? AND owner_user_id = ?")
            .bind(id).bind(owner).fetch_one(&mut *transaction).await?;
        let response = type_json(&row)?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn delete_type(&self, owner: i32, id: &str) -> AssetResult<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_type(&mut transaction, owner, id).await?;
        let used: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices WHERE owner_user_id = ? AND device_type_id = ?)")
            .bind(owner).bind(id).fetch_one(&mut *transaction).await?;
        if used != 0 { return Err(AssetError::conflict("device_type_in_use", "This type is used by a device and cannot be deleted.")); }
        sqlx::query("DELETE FROM asset_device_types WHERE id = ? AND owner_user_id = ?")
            .bind(id).bind(owner).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }
}
