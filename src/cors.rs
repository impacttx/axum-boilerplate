use anyhow::{Context, Result};
use axum::http::{HeaderValue, Method, header};
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Builds the CORS layer from the configured origin allow-list.
/// An empty list disables cross-origin access; `*` allows any origin (without credentials).
pub fn layer(origins: &[String]) -> Result<CorsLayer> {
    let base = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .expose_headers([header::RETRY_AFTER])
        .max_age(Duration::from_secs(3600));

    if origins.iter().any(|o| o == "*") {
        return Ok(base.allow_origin(AllowOrigin::any()));
    }
    let values = origins
        .iter()
        .map(|o| HeaderValue::from_str(o).with_context(|| format!("invalid CORS origin: {o}")))
        .collect::<Result<Vec<_>>>()?;
    Ok(base.allow_origin(values).allow_credentials(true))
}

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    use super::*;

    async fn preflight(origins: &[&str], origin: &str) -> Option<String> {
        let origins: Vec<String> = origins.iter().map(|s| s.to_string()).collect();
        let app = Router::new().route("/", get(|| async { "ok" })).layer(layer(&origins).unwrap());
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/")
            .header("origin", origin)
            .header("access-control-request-method", "GET")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        res.headers().get("access-control-allow-origin").map(|v| v.to_str().unwrap().to_string())
    }

    #[tokio::test]
    async fn allows_only_listed_origins() {
        let allowed = ["http://localhost:3000"];
        assert_eq!(preflight(&allowed, "http://localhost:3000").await.as_deref(), Some("http://localhost:3000"));
        assert_eq!(preflight(&allowed, "http://evil.example").await, None);
        assert_eq!(preflight(&[], "http://localhost:3000").await, None);
        assert_eq!(preflight(&["*"], "http://any.example").await.as_deref(), Some("*"));
    }
}
