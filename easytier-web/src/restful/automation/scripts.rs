use axum::{Extension, Json, extract::Path, http::HeaderMap};
use serde_json::Value;

use super::{common::*, super::users::AuthSession};

pub(super) async fn list(auth: AuthSession, Extension(service): Service, Extension(config): Config, query: OwnerInput, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    Ok(Json(store(&service).list_scripts(owner).await.map_err(store_error)?))
}

pub(super) async fn create(auth: AuthSession, Extension(service): Service, Extension(config): Config, query: OwnerInput, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let payload = body(input)?;
    exact_fields(&payload,&["name","description","steps"])?;
    Ok(Json(store(&service).save_script(owner,None,&payload).await.map_err(store_error)?))
}

pub(super) async fn update(auth: AuthSession, Extension(service): Service, Extension(config): Config, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap, input: Body) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let id = id(&raw_id)?;
    let payload = body(input)?;
    exact_fields(&payload,&["name","description","steps","revision"])?;
    Ok(Json(store(&service).save_script(owner,Some(&id),&payload).await.map_err(store_error)?))
}

/// T46: first-party floating UI uses an explicit revision-bearing POST.
/// No stale tab can delete a newer script, even if its GET raced with edits.
pub(super) async fn safe_remove(
    auth: AuthSession, Extension(service): Service, Extension(config): Config,
    query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap, input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let id=id(&raw_id)?;
    let payload=body(input)?;
    exact_fields(&payload,&["revision"])?;
    let revision=payload["revision"].as_i64().filter(|n|*n>0)
        .ok_or_else(||error(axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_revision","请提供有效的脚本修订号"))?;
    Ok(Json(store(&service).delete_script_if_revision(owner,&id,revision).await.map_err(store_error)?))
}

pub(super) async fn remove(auth: AuthSession, Extension(service): Service, Extension(config): Config, query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner = owner(query)?;
    authorize(&auth,&config,&service,owner).await?;
    let id = id(&raw_id)?;
    Ok(Json(store(&service).delete_script(owner,&id).await.map_err(store_error)?))
}
