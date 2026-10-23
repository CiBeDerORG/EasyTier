use serde_json::{Value, json};

use super::{AssetsStore, binding_ops, common::{AssetError, AssetResult, ensure_group, ensure_node, group_json, group_warnings, now}, idempotency::Operation, validation};

impl AssetsStore {
    pub(crate) async fn list_groups(&self, owner: i32, node: &str) -> AssetResult<Value> {
        let mut connection = self.pool.acquire().await?;
        ensure_node(&mut connection, owner, node).await?;
        let rows = sqlx::query("SELECT * FROM asset_gateway_groups WHERE owner_user_id = ? AND machine_id = ? ORDER BY created_at DESC,id DESC")
            .bind(owner).bind(node).fetch_all(&mut *connection).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows { items.push(group_json(&mut connection, &row, &self.runtime_observations).await?); }
        Ok(json!({"items":items}))
    }

    pub(crate) async fn create_group(&self, actor: i32, owner: i32, node: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::group_payload(&body, true)?;
        let operation = Operation::new(actor, owner, "create_group", json!({"machine_id":node}), &body, &payload)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(response) = operation.replay(&mut transaction).await? {
            transaction.commit().await?;
            return Ok(response);
        }
        ensure_node(&mut transaction, owner, node).await?;
        let preview = binding_ops::prepare_binding(&mut transaction, owner, node,
            &json!({"resource_kind":"gateway","id":null}),
            &json!({"operation":"attach","instance_id":payload["binding"]["instance_id"]}), &self.runtime_observations).await?;
        binding_ops::assert_version(&payload, &preview)?;
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        sqlx::query("INSERT INTO asset_gateway_groups(id,owner_user_id,machine_id,name,remarks,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
            .bind(&id).bind(owner).bind(node).bind(payload["name"].as_str()).bind(payload["remarks"].as_str())
            .bind(&timestamp).bind(&timestamp).execute(&mut *transaction).await?;
        binding_ops::save_snapshot(&mut transaction, owner, node, "gateway", &id, &preview["snapshot"]).await?;
        let row = sqlx::query("SELECT * FROM asset_gateway_groups WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(&id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = group_json(&mut transaction, &row, &self.runtime_observations).await?;
        operation.finish(&mut transaction, "group", &id, &response).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn update_group(&self, owner: i32, node: &str, id: &str, body: Value) -> AssetResult<Value> {
        let payload = validation::group_payload(&body, false)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let old = binding_ops::subject_row(&mut transaction, owner, node, "gateway", id).await?;
        let legacy = binding_ops::stored_snapshot(&old).is_ok_and(|snapshot| snapshot.is_none());
        if !legacy && payload.get("subnets").is_some() {
            return Err(AssetError::field("subnets", "Bound gateway subnets come from its instance. Use the binding review action."));
        }
        sqlx::query("UPDATE asset_gateway_groups SET name = ?, remarks = ?, updated_at = ? WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(payload["name"].as_str()).bind(payload["remarks"].as_str()).bind(now()).bind(id).bind(owner).bind(node)
            .execute(&mut *transaction).await?;
        if let Some(subnets) = payload["subnets"].as_array() {
            sqlx::query("DELETE FROM asset_gateway_subnets WHERE group_id = ? AND owner_user_id = ? AND machine_id = ?")
                .bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
            for subnet in subnets {
                sqlx::query("INSERT INTO asset_gateway_subnets(group_id,owner_user_id,machine_id,cidr) VALUES(?,?,?,?)")
                    .bind(id).bind(owner).bind(node).bind(subnet.as_str()).execute(&mut *transaction).await?;
            }
        }
        let row = sqlx::query("SELECT * FROM asset_gateway_groups WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).fetch_one(&mut *transaction).await?;
        let response = group_json(&mut transaction, &row, &self.runtime_observations).await?;
        transaction.commit().await?;
        Ok(response)
    }

    pub(crate) async fn delete_group(&self, owner: i32, node: &str, id: &str) -> AssetResult<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_group(&mut transaction, owner, node, id).await?;
        let used: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_devices WHERE owner_user_id = ? AND machine_id = ? AND gateway_group_id = ?)")
            .bind(owner).bind(node).bind(id).fetch_one(&mut *transaction).await?;
        if used != 0 { return Err(AssetError::conflict("gateway_group_not_empty", "Delete the attached devices before deleting this gateway device.")); }
        sqlx::query("DELETE FROM asset_gateway_groups WHERE id = ? AND owner_user_id = ? AND machine_id = ?")
            .bind(id).bind(owner).bind(node).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn preview_subnets(&self, owner: i32, node: &str, body: Value) -> AssetResult<Value> {
        validation::keys(&body, &["gateway_group_id", "subnets"])?;
        let mut connection = self.pool.acquire().await?;
        ensure_node(&mut connection, owner, node).await?;
        let group = validation::optional_id(&body, "gateway_group_id")?;
        if let Some(id) = &group { ensure_group(&mut connection, owner, node, id).await?; }
        let subnets = match validation::subnets(&body) {
            Ok(subnets) => subnets,
            Err(error) if error.status == 422 => return Ok(json!({"field_errors":error.field_errors,"warnings":[]})),
            Err(error) => return Err(error),
        };
        let warnings = if let Some(id) = group { group_warnings(&mut connection, owner, node, &id, &subnets).await? } else { Vec::new() };
        Ok(json!({"field_errors":{},"warnings":warnings}))
    }
}
