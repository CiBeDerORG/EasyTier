use axum::{Extension,Json,extract::Path,http::HeaderMap};
use serde_json::Value;
use super::{common::*,super::users::AuthSession};

pub(super) async fn list(
    auth:AuthSession,Extension(service):Service,Extension(config):Config,
    query:OwnerInput,headers:HeaderMap
)->HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    Ok(Json(store(&service).list_adb_favorites(owner).await.map_err(store_error)?))
}

pub(super) async fn save(
    auth:AuthSession,Extension(service):Service,Extension(config):Config,
    query:OwnerInput,headers:HeaderMap,input:Body
)->HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let payload=body(input)?;
    exact_fields(&payload,&["title","command"])?;
    Ok(Json(store(&service).save_adb_favorite(owner,&payload).await.map_err(store_error)?))
}

pub(super) async fn remove(
    auth:AuthSession,Extension(service):Service,Extension(config):Config,
    query:OwnerInput,Path(raw_id):Path<String>,headers:HeaderMap
)->HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let id=id(&raw_id)?;
    Ok(Json(store(&service).delete_adb_favorite(owner,&id).await.map_err(store_error)?))
}
