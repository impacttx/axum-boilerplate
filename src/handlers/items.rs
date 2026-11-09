use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    entities::items,
    error::{AppError, AppResult},
    repositories::{CreateItem, UpdateItem},
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateItemRequest {
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub is_completed: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateItemRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub is_completed: Option<bool>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub skip: u64,
}

fn default_limit() -> u64 {
    20
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ItemCountResponse {
    pub total: u64,
}

#[utoipa::path(
    post,
    path = "/api/v1/items",
    request_body = CreateItemRequest,
    responses(
        (status = 201, description = "Item created", body = items::Model)
    )
)]
pub async fn create_item(
    State(state): State<AppState>,
    Json(payload): Json<CreateItemRequest>,
) -> AppResult<(StatusCode, Json<items::Model>)> {
    let item = state
        .deps
        .items
        .create(CreateItem {
            title: payload.title,
            description: payload.description,
            is_completed: payload.is_completed,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(item)))
}

#[utoipa::path(
    get,
    path = "/api/v1/items",
    params(ListParams),
    responses(
        (status = 200, description = "List of items", body = Vec<items::Model>)
    )
)]
pub async fn list_items(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> AppResult<Json<Vec<items::Model>>> {
    let items = state.deps.items.list(params.limit, params.skip).await?;
    Ok(Json(items))
}

#[utoipa::path(
    get,
    path = "/api/v1/items/count",
    responses(
        (status = 200, description = "Total count of items", body = ItemCountResponse)
    )
)]
pub async fn count_items(State(state): State<AppState>) -> AppResult<Json<ItemCountResponse>> {
    let total = state.deps.items.count().await?;
    Ok(Json(ItemCountResponse { total }))
}

#[utoipa::path(
    get,
    path = "/api/v1/items/{id}",
    params(
        ("id" = Uuid, Path, description = "Item ID")
    ),
    responses(
        (status = 200, description = "Item details", body = items::Model),
        (status = 404, description = "Item not found")
    )
)]
pub async fn get_item(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<items::Model>> {
    let item = state.deps.items.find_by_id(id).await?.ok_or(AppError::NotFound("item not found"))?;
    Ok(Json(item))
}

#[utoipa::path(
    put,
    path = "/api/v1/items/{id}",
    request_body = UpdateItemRequest,
    params(
        ("id" = Uuid, Path, description = "Item ID")
    ),
    responses(
        (status = 200, description = "Item updated", body = items::Model),
        (status = 404, description = "Item not found")
    )
)]
pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateItemRequest>,
) -> AppResult<Json<items::Model>> {
    let item = state
        .deps
        .items
        .update(
            id,
            UpdateItem { title: payload.title, description: payload.description, is_completed: payload.is_completed },
        )
        .await?
        .ok_or(AppError::NotFound("item not found"))?;
    Ok(Json(item))
}

#[utoipa::path(
    delete,
    path = "/api/v1/items/{id}",
    params(
        ("id" = Uuid, Path, description = "Item ID")
    ),
    responses(
        (status = 204, description = "Item deleted"),
        (status = 404, description = "Item not found")
    )
)]
pub async fn delete_item(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let deleted = state.deps.items.delete(id).await?;
    if !deleted {
        return Err(AppError::NotFound("item not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
