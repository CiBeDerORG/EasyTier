//! Per-ADB-service keyguard credentials are never returned by a read endpoint.
use std::sync::Arc;
use axum::{Extension,Json,extract::Path,http::{HeaderMap,StatusCode}};
use serde_json::{Value,json};
use super::{common::*,settings::Settings,super::users::AuthSession};

async fn checked(service:&crate::central_network::service::CentralNetworkService,owner:i32,id:&str)->HttpResult<()>{
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_services s JOIN asset_devices d ON d.id=s.device_id AND d.owner_user_id=s.owner_user_id JOIN asset_device_types t ON t.id=d.device_type_id AND t.owner_user_id=d.owner_user_id WHERE s.id=? AND s.owner_user_id=? AND s.service_type='adb' AND EXISTS (SELECT 1 FROM json_each(t.semantic_kinds) WHERE value='phone') AND d.is_gateway_self=0)")
        .bind(id).bind(owner).fetch_one(&service.db.inner()).await.map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","无法读取 ADB 服务。"))?;
    if !valid{return Err(error(StatusCode::NOT_FOUND,"adb_service_missing","未找到可管理的手机 ADB 服务。"));}
    Ok(())
}
pub(super) async fn get(auth:AuthSession,Extension(service):Service,Extension(config):Config,query:OwnerInput,Path(raw_id):Path<String>,headers:HeaderMap)->HttpResult<Json<Value>>{
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let id=id(&raw_id)?;
    checked(&service,owner,&id).await?;
    let configured:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM adb_service_unlock_secrets WHERE service_id=? AND owner_user_id=?)")
        .bind(&id).bind(owner).fetch_one(&service.db.inner()).await.map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","无法检查密码状态。"))?;
    Ok(Json(json!({"configured":configured})))
}
pub(super) async fn save(auth:AuthSession,Extension(service):Service,Extension(config):Config,Extension(settings):Extension<Arc<Settings>>,query:OwnerInput,Path(raw_id):Path<String>,headers:HeaderMap,input:Body)->HttpResult<Json<Value>>{
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let id=id(&raw_id)?;
    let payload=body(input)?;
    exact_fields(&payload,&["action","password"])?;
    let action=payload["action"].as_str().unwrap_or_default();
    let cipher=match action {
        "replace"=>{
            let password=payload["password"].as_str().ok_or_else(||error(StatusCode::UNPROCESSABLE_ENTITY,"unlock_password_invalid","请输入 4～64 位开屏密码。"))?;
            if !(4..=64).contains(&password.len())||!password.bytes().all(|b|(0x21..=0x7e).contains(&b)){
                return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"unlock_password_invalid","开屏密码只能使用 4～64 位无空格的可打印 ASCII 字符，支持数字 PIN。"));
            }
            Some(settings.seal_unlock_password(owner,&id,password).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_vault_unavailable","凭据保险箱不可用。"))?)
        }
        "clear" if payload.get("password").is_none()=>None,
        _=>return Err(error(StatusCode::UNPROCESSABLE_ENTITY,"unlock_action_invalid","请选择保存或清除密码。"))
    };
    let pool=service.db.inner();
    let mut tx=pool.begin().await.map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","无法保存密码。"))?;
    // Write transaction serializes with service type changes and deletion.
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_services s JOIN asset_devices d ON d.id=s.device_id AND d.owner_user_id=s.owner_user_id JOIN asset_device_types t ON t.id=d.device_type_id AND t.owner_user_id=d.owner_user_id WHERE s.id=? AND s.owner_user_id=? AND s.service_type='adb' AND EXISTS (SELECT 1 FROM json_each(t.semantic_kinds) WHERE value='phone') AND d.is_gateway_self=0)")
        .bind(&id).bind(owner).fetch_one(&mut *tx).await.map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","无法验证 ADB 服务。"))?;
    if !valid{return Err(error(StatusCode::NOT_FOUND,"adb_service_missing","手机 ADB 服务已改变。"));}
    if let Some(cipher)=cipher {
        sqlx::query("INSERT INTO adb_service_unlock_secrets(service_id,owner_user_id,secret_cipher,updated_at) VALUES(?,?,?,?) ON CONFLICT(service_id) DO UPDATE SET owner_user_id=excluded.owner_user_id,secret_cipher=excluded.secret_cipher,updated_at=excluded.updated_at")
            .bind(&id).bind(owner).bind(cipher).bind(chrono::Utc::now().to_rfc3339()).execute(&mut *tx).await.map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","密码保存失败。"))?;
    }else{
        sqlx::query("DELETE FROM adb_service_unlock_secrets WHERE service_id=? AND owner_user_id=?").bind(&id).bind(owner).execute(&mut *tx).await.map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","密码清除失败。"))?;
    }
    tx.commit().await.map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"unlock_storage_error","密码保存失败。"))?;
    Ok(Json(json!({"configured":action=="replace"})))
}
