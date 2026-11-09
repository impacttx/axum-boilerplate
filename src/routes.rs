use std::time::Duration;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::StatusCode,
    middleware,
    routing::{MethodRouter, delete, get, post, put},
};
use serde_json::json;
use tower_http::{
    compression::CompressionLayer, cors::CorsLayer, services::ServeDir, timeout::TimeoutLayer, trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{handlers, openapi::ApiDoc, rate_limit, state::AppState};

fn auth_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::auth::login))
        .route("/get_otp", post(handlers::auth::get_otp))
        .route("/register", post(handlers::auth::register))
        .route("/verify_account", post(handlers::auth::verify_account))
        .route("/google", post(handlers::auth::google))
        .route("/refresh_token", post(handlers::auth::refresh_token))
        .route("/logout", post(handlers::auth::logout))
        .layer(middleware::from_fn_with_state(state.auth_limiter.clone(), rate_limit::enforce))
}

fn route_both(router: Router<AppState>, path: &str, m: MethodRouter<AppState>) -> Router<AppState> {
    router.route(path, m.clone()).route(&format!("{path}/"), m)
}

fn api_routes(state: &AppState) -> Router<AppState> {
    let mut r = Router::new()
        .route("/admin/users", get(handlers::users::list))
        .route(
            "/users/profile",
            get(handlers::users::get_profile)
                .put(handlers::users::put_profile)
                .patch(handlers::users::patch_profile)
                .delete(handlers::users::delete_profile),
        )
        .route(
            "/users/profile-picture",
            put(handlers::users::put_profile_picture).layer(DefaultBodyLimit::max(state.max_upload_bytes)),
        )
        .route("/devices/register", post(handlers::devices::register))
        .route("/devices/voip-token", post(handlers::devices::voip_token))
        .route("/devices/{device_id}", delete(handlers::devices::unregister))
        .route("/admin/rbac/assign", post(handlers::rbac::assign))
        .route("/admin/rbac/remove", post(handlers::rbac::remove))
        .route("/admin/rbac/users/status", post(handlers::rbac::set_status));

    // Items CRUD routes (for benchmarking and data manipulation)
    r = route_both(r, "/items", get(handlers::items::list_items).post(handlers::items::create_item));
    r = route_both(r, "/items/count", get(handlers::items::count_items));
    r = route_both(
        r,
        "/items/{id}",
        get(handlers::items::get_item).put(handlers::items::update_item).delete(handlers::items::delete_item),
    );

    r.layer(middleware::from_fn_with_state(state.general_limiter.clone(), rate_limit::enforce))
}

pub fn router_with_docs(state: AppState, cors: CorsLayer, docs_enabled: bool) -> Router {
    let uploads = ServeDir::new(&*state.upload_dir);
    let mut app = Router::new()
        .route("/", get(root))
        .route("/health", get(handlers::health::health_check))
        .route("/health/", get(handlers::health::health_check))
        .nest("/v1/auth", auth_routes(&state))
        .nest("/api/v1/auth", auth_routes(&state))
        .nest("/v1", api_routes(&state))
        .nest("/api/v1", api_routes(&state))
        .nest_service("/uploads", uploads)
        .fallback(not_found);

    if docs_enabled {
        let openapi = ApiDoc::openapi();
        app = app.merge(SwaggerUi::new("/docs").url("/openapi.json", openapi));
    }

    app.with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(cors)
}

async fn root() -> Json<serde_json::Value> {
    Json(json!({
        "service": "axum-performance-containerization",
        "runtime": "Rust 1.99 + Axum 0.8.9 + Hyper + Tokio",
        "status": "online"
    }))
}

async fn not_found() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "Not Found",
            "message": "The requested resource could not be found."
        })),
    )
}
