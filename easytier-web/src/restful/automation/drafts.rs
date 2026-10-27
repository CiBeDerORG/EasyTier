//! Authenticated, account-scoped draft workspaces.
//! All writes require revision fencing and canonical per-request UUIDs.

use axum::{Extension, Json, extract::Path, http::HeaderMap};
use serde_json::Value;

use super::{common::*, super::users::AuthSession};

pub(super) async fn list(
    auth: AuthSession, Extension(service): Service, Extension(config): Config,
    query: OwnerInput, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    let actor=authorize(&auth,&config,&service,owner).await?;
    Ok(Json(store(&service).list_drafts(actor,owner).await.map_err(store_error)?))
}

pub(super) async fn get_one(
    auth: AuthSession, Extension(service): Service, Extension(config): Config,
    query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap,
) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    let actor=authorize(&auth,&config,&service,owner).await?;
    let id=id(&raw_id)?;
    Ok(Json(store(&service).get_draft(actor,owner,&id).await.map_err(store_error)?))
}

pub(super) async fn create(
    auth: AuthSession, Extension(service): Service, Extension(config): Config,
    query: OwnerInput, headers: HeaderMap, input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    let actor=authorize(&auth,&config,&service,owner).await?;
    let payload=body(input)?;
    exact_fields(&payload,&["request_id","name","description","steps","script_id","script_revision"])?;
    Ok(Json(store(&service).save_draft(actor,owner,None,&payload).await.map_err(store_error)?))
}

pub(super) async fn update(
    auth: AuthSession, Extension(service): Service, Extension(config): Config,
    query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap, input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    let actor=authorize(&auth,&config,&service,owner).await?;
    let id=id(&raw_id)?;
    let payload=body(input)?;
    exact_fields(&payload,&["request_id","revision","name","description","steps","script_id","script_revision"])?;
    Ok(Json(store(&service).save_draft(actor,owner,Some(&id),&payload).await.map_err(store_error)?))
}

pub(super) async fn discard(
    auth: AuthSession, Extension(service): Service, Extension(config): Config,
    query: OwnerInput, Path(raw_id): Path<String>, headers: HeaderMap, input: Body,
) -> HttpResult<Json<Value>> {
    require_origin(&headers,false)?;
    let owner=owner(query)?;
    let actor=authorize(&auth,&config,&service,owner).await?;
    let id=id(&raw_id)?;
    let payload=body(input)?;
    exact_fields(&payload,&["request_id","revision"])?;
    Ok(Json(store(&service).discard_draft(actor,owner,&id,&payload).await.map_err(store_error)?))
}
