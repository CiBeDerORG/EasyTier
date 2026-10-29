use axum::{Extension,Json,extract::Path,http::HeaderMap};
use serde_json::Value;
use super::{common::*,super::users::AuthSession};

pub(super) async fn list_hidden(
    auth:AuthSession,Extension(service):Service,Extension(config):Config,
    query:OwnerInput,headers:HeaderMap
)->HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    Ok(Json(store(&service).list_hidden_adb_presets(owner).await.map_err(store_error)?))
}

pub(super) async fn hide(
    auth:AuthSession,Extension(service):Service,Extension(config):Config,
    query:OwnerInput,Path(raw_id):Path<String>,headers:HeaderMap
)->HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    Ok(Json(store(&service).hide_adb_preset(owner,&raw_id).await.map_err(store_error)?))
}

pub(super) async fn restore(
    auth:AuthSession,Extension(service):Service,Extension(config):Config,
    query:OwnerInput,Path(raw_id):Path<String>,headers:HeaderMap
)->HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    Ok(Json(store(&service).restore_adb_preset(owner,&raw_id).await.map_err(store_error)?))
}
