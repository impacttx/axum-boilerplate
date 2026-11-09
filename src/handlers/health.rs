use axum::{Json, extract::State};
use serde_json::{Value, json};

use crate::{error::AppResult, state::AppState};

#[utoipa::path(
    get, path = "/", tag = "Health",
    responses((status = 200, description = "Welcome to Axum Benchmark API"))
)]
pub async fn root() -> Json<Value> {
    Json(json!({
        "service": "axum-performance-containerization",
        "runtime": "Rust 1.99 + Axum 0.8.9 + Hyper + Tokio",
        "status": "online"
    }))
}

#[utoipa::path(get, path = "/health", tag = "Health", responses((status = 200, description = "Healthy")))]
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "framework": "Axum"
    }))
}

#[utoipa::path(
    get, path = "/health/ready", tag = "Health",
    responses((status = 200, description = "Ready"), (status = 500, description = "Database unreachable"))
)]
pub async fn ready(State(state): State<AppState>) -> AppResult<Json<Value>> {
    state.users.ping().await?;
    Ok(Json(json!({ "status": "ok" })))
}
