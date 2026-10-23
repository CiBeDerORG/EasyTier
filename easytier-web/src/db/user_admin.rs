//! Account retirement preserves ownership and all registered network/assets.

use sea_orm::DbErr;
use serde::Serialize;
use sqlx::{Row, Sqlite, SqliteConnection};

use super::{Db, UserIdInDb, sqlx_db_error};

#[derive(Debug, thiserror::Error)]
pub(crate) enum UserAdminError {
    #[error("Administrator access is required.")]
    Forbidden,
    #[error("User not found.")]
    NotFound,
    #[error("You cannot delete your own account.")]
    DeleteSelf,
    #[error("The last active administrator cannot be deleted.")]
    LastAdministrator,
    #[error("Could not update user management data.")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Serialize)]
pub(crate) struct ManagedUser {
    pub id: UserIdInDb,
    pub username: String,
    pub is_admin: bool,
    pub is_current_user: bool,
    pub deleted_at: Option<String>,
    pub network_instance_count: i64,
    pub disabled_network_instance_count: i64,
    pub node_count: i64,
    pub can_delete: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct UserDeletion {
    pub user_id: UserIdInDb,
    pub deleted_at: String,
    pub already_deleted: bool,
    pub disabled_network_instance_count: i64,
}

async fn administrator_count(connection: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM users u WHERE u.deleted_at IS NULL AND EXISTS (\
         SELECT 1 FROM users_groups ug JOIN groups g ON g.id = ug.group_id \
         WHERE ug.user_id = u.id AND g.name = 'admins')",
    )
    .fetch_one(connection)
    .await
}

async fn require_administrator(
    connection: &mut SqliteConnection,
    actor_id: UserIdInDb,
) -> Result<(), UserAdminError> {
    let allowed: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users u JOIN users_groups ug ON ug.user_id = u.id \
         JOIN groups g ON g.id = ug.group_id \
         WHERE u.id = ? AND u.deleted_at IS NULL AND g.name = 'admins')",
    )
    .bind(actor_id)
    .fetch_one(connection)
    .await?;
    if allowed == 0 {
        return Err(UserAdminError::Forbidden);
    }
    Ok(())
}

impl Db {
    /// Missing and retired accounts cannot authenticate or publish configuration.
    /// This deliberately differs from get_user_id, which still resolves a retired
    /// username so registration/auto-provisioning cannot reclaim its data.
    pub(crate) async fn is_user_active(&self, user_id: UserIdInDb) -> Result<bool, DbErr> {
        let active: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND deleted_at IS NULL)",
        )
        .bind(user_id)
        .fetch_one(&self.inner())
        .await
        .map_err(sqlx_db_error)?;
        Ok(active != 0)
    }

    pub(crate) async fn is_active_administrator(
        &self,
        user_id: UserIdInDb,
    ) -> Result<bool, DbErr> {
        let active: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users u JOIN users_groups ug ON ug.user_id = u.id \
             JOIN groups g ON g.id = ug.group_id \
             WHERE u.id = ? AND u.deleted_at IS NULL AND g.name = 'admins')",
        )
        .bind(user_id)
        .fetch_one(&self.inner())
        .await
        .map_err(sqlx_db_error)?;
        Ok(active != 0)
    }

    pub(crate) async fn list_managed_users(
        &self,
        actor_id: UserIdInDb,
    ) -> Result<Vec<ManagedUser>, UserAdminError> {
        let mut transaction = self.inner().begin().await?;
        require_administrator(&mut transaction, actor_id).await?;
        let administrators = administrator_count(&mut transaction).await?;
        let rows = sqlx::query(
            "SELECT u.id, u.username, u.deleted_at, \
             EXISTS(SELECT 1 FROM users_groups ug JOIN groups g ON g.id = ug.group_id \
                 WHERE ug.user_id = u.id AND g.name = 'admins') AS is_admin, \
             (SELECT COUNT(*) FROM devices d WHERE d.user_id = u.id) AS node_count, \
             (SELECT COUNT(*) FROM user_running_network_configs c WHERE c.user_id = u.id) AS network_instance_count, \
             (SELECT COUNT(*) FROM user_running_network_configs c WHERE c.user_id = u.id AND c.disabled = 1) AS disabled_network_instance_count \
             FROM users u ORDER BY (u.deleted_at IS NOT NULL), u.username, u.id",
        )
        .fetch_all(&mut *transaction)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.try_get::<i32, _>("id")?;
            let deleted_at = row.try_get::<Option<String>, _>("deleted_at")?;
            let is_admin = row.try_get::<i64, _>("is_admin")? != 0;
            items.push(ManagedUser {
                id,
                username: row.try_get("username")?,
                is_admin,
                is_current_user: id == actor_id,
                can_delete: deleted_at.is_none()
                    && id != actor_id
                    && (!is_admin || administrators > 1),
                deleted_at,
                node_count: row.try_get("node_count")?,
                network_instance_count: row.try_get("network_instance_count")?,
                disabled_network_instance_count: row.try_get("disabled_network_instance_count")?,
            });
        }
        transaction.commit().await?;
        Ok(items)
    }

    /// The caller also holds the central service's mutation lock. Re-check the
    /// actor inside the SQLite write transaction: an old authenticated request
    /// must not delete a user after its own administrator account was retired.
    pub(crate) async fn retire_user_and_disable_networks(
        &self,
        actor_id: UserIdInDb,
        user_id: UserIdInDb,
    ) -> Result<UserDeletion, UserAdminError> {
        let mut transaction: sqlx::Transaction<'_, Sqlite> =
            self.inner().begin_with("BEGIN IMMEDIATE").await?;
        require_administrator(&mut transaction, actor_id).await?;
        if actor_id == user_id {
            return Err(UserAdminError::DeleteSelf);
        }
        let target = sqlx::query(
            "SELECT deleted_at, EXISTS(SELECT 1 FROM users_groups ug \
             JOIN groups g ON g.id = ug.group_id \
             WHERE ug.user_id = users.id AND g.name = 'admins') AS is_admin \
             FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(UserAdminError::NotFound)?;
        let prior_deletion: Option<String> = target.try_get("deleted_at")?;
        if prior_deletion.is_none()
            && target.try_get::<i64, _>("is_admin")? != 0
            && administrator_count(&mut transaction).await? <= 1
        {
            return Err(UserAdminError::LastAdministrator);
        }
        let already_deleted = prior_deletion.is_some();
        let now = chrono::Utc::now().to_rfc3339();
        let deleted_at = prior_deletion.unwrap_or_else(|| now.clone());
        sqlx::query("UPDATE users SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(&deleted_at)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "UPDATE user_running_network_configs SET disabled = 1, update_time = ? \
             WHERE user_id = ? AND disabled = 0",
        )
        .bind(&now)
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
        // These are publication cache tokens, not network definitions. Keeping
        // an applied revision would let an old client skip the disabled state.
        sqlx::query("DELETE FROM managed_config_revisions WHERE user_id = ?")
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        let disabled_network_instance_count = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_running_network_configs WHERE user_id = ? AND disabled = 1",
        )
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(UserDeletion {
            user_id,
            deleted_at,
            already_deleted,
            disabled_network_instance_count,
        })
    }
}
