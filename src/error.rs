use axum::{
    Json,
    http::{StatusCode, header::RETRY_AFTER},
    response::{IntoResponse, Response},
};
use sea_orm::{DbErr, SqlErr};
use serde_json::json;

/// JSON error envelope returned by every failing endpoint.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ErrorDetail {
    /// Machine-readable code, e.g. `unauthorized`.
    pub code: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("invalid or missing credentials")]
    Unauthorized,
    #[error("insufficient permissions")]
    Forbidden,
    /// 403 with a specific, user-facing reason.
    #[error("{0}")]
    Denied(String),
    #[error("{0}")]
    Unprocessable(String),
    #[error("{0}")]
    PaymentRequired(String),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(String),
    #[error("too many requests")]
    TooManyRequests { retry_after_secs: u64 },
    #[error("{0}")]
    Unavailable(String),
    #[error("internal server error")]
    Internal(#[source] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<DbErr> for AppError {
    fn from(err: DbErr) -> Self {
        if let Some(SqlErr::UniqueConstraintViolation(_)) = err.sql_err() {
            return AppError::Conflict("resource already exists".into());
        }
        AppError::Internal(err.into())
    }
}

impl AppError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::Denied(_) => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::PaymentRequired(_) => (StatusCode::PAYMENT_REQUIRED, "payment_required"),
            AppError::Unprocessable(_) => (StatusCode::UNPROCESSABLE_ENTITY, "unprocessable"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            AppError::TooManyRequests { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            AppError::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        if let AppError::Internal(e) = &self {
            tracing::error!(error = ?e, "internal error");
        }
        // Display of Internal is a generic message; details are only logged.
        let body = Json(json!({ "error": { "code": code, "message": self.to_string() } }));
        let mut response = (status, body).into_response();
        if let AppError::TooManyRequests { retry_after_secs } = self {
            response.headers_mut().insert(RETRY_AFTER, retry_after_secs.into());
        }
        response
    }
}
