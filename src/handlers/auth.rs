use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    auth::AccessClaims,
    error::{AppResult, ErrorBody},
    services::auth::{
        GoogleLoginRequest, LoginPayload, LogoutRequest, MobileRequest, OtpIssued, TokenResponse, VerifyAccountRequest,
    },
    state::AppState,
};

/// Log in with mobile number + OTP (apps) or email + password (staff and admin accounts).
#[utoipa::path(
    post, path = "/v1/auth/login", tag = "Auth",
    request_body = LoginPayload,
    responses(
        (status = 200, description = "Token pair", body = TokenResponse),
        (status = 400, description = "Invalid or expired OTP", body = ErrorBody),
        (status = 401, description = "Invalid credentials", body = ErrorBody),
        (status = 404, description = "No active user for that mobile number", body = ErrorBody),
        (status = 429, description = "Rate limited", body = ErrorBody),
    )
)]
pub async fn login(State(s): State<AppState>, Json(req): Json<LoginPayload>) -> AppResult<Json<TokenResponse>> {
    Ok(Json(s.auth_service.login_any(req).await?))
}

/// Send a login OTP to an existing account's mobile number.
#[utoipa::path(
    post, path = "/v1/auth/get_otp", tag = "Auth",
    request_body = MobileRequest,
    responses(
        (status = 200, description = "OTP issued successfully", body = OtpIssued),
        (status = 404, description = "No active user for that mobile number", body = ErrorBody),
        (status = 422, description = "Invalid mobile number", body = ErrorBody),
        (status = 429, description = "Rate limited", body = ErrorBody),
    )
)]
pub async fn get_otp(State(s): State<AppState>, Json(req): Json<MobileRequest>) -> AppResult<Json<OtpIssued>> {
    Ok(Json(s.auth_service.get_otp(req).await?))
}

/// Start sign-up with a mobile number; an OTP is sent for `verify_account`.
#[utoipa::path(
    post, path = "/v1/auth/register", tag = "Auth",
    request_body = MobileRequest,
    responses(
        (status = 201, description = "Account registration started; OTP issued", body = OtpIssued),
        (status = 409, description = "Mobile number already registered", body = ErrorBody),
        (status = 422, description = "Invalid mobile number", body = ErrorBody),
        (status = 429, description = "Rate limited", body = ErrorBody),
    )
)]
pub async fn register(
    State(s): State<AppState>,
    Json(req): Json<MobileRequest>,
) -> AppResult<(StatusCode, Json<OtpIssued>)> {
    Ok((StatusCode::CREATED, Json(s.auth_service.register(req).await?)))
}

/// Verify the sign-up OTP, accept the terms and activate the account.
#[utoipa::path(
    post, path = "/v1/auth/verify_account", tag = "Auth",
    request_body = VerifyAccountRequest,
    responses(
        (status = 200, description = "Token pair", body = TokenResponse),
        (status = 400, description = "Invalid or expired OTP", body = ErrorBody),
        (status = 404, description = "No pending registration", body = ErrorBody),
        (status = 429, description = "Rate limited", body = ErrorBody),
    )
)]
pub async fn verify_account(
    State(s): State<AppState>,
    Json(req): Json<VerifyAccountRequest>,
) -> AppResult<Json<TokenResponse>> {
    Ok(Json(s.auth_service.verify_account(req).await?))
}

/// Log in or register with a Google ID token (Google Identity Services `credential`),
/// or mock tokens for local testing (`mock_sales_token`, `mock_admin_token`, `mock_*`).
#[utoipa::path(
    post, path = "/v1/auth/google", tag = "Auth",
    request_body = GoogleLoginRequest,
    responses(
        (status = 200, description = "Token pair", body = TokenResponse),
        (status = 400, description = "Missing token", body = ErrorBody),
        (status = 401, description = "Invalid Google token or inactive account", body = ErrorBody),
        (status = 429, description = "Rate limited", body = ErrorBody),
        (status = 503, description = "Google login not configured", body = ErrorBody),
    )
)]
pub async fn google(State(s): State<AppState>, Json(req): Json<GoogleLoginRequest>) -> AppResult<Json<TokenResponse>> {
    Ok(Json(s.auth_service.google_login(req).await?))
}

#[derive(Deserialize, Default, utoipa::ToSchema)]
pub struct RefreshRequest {
    #[serde(default)]
    pub refresh_token: Option<String>,
}

/// Exchange a refresh token for a new pair; the presented token is revoked (rotation).
#[utoipa::path(
    post, path = "/v1/auth/refresh_token", tag = "Auth",
    request_body = Option<RefreshRequest>,
    security(("bearer" = [])),
    responses(
        (status = 201, description = "New token pair", body = TokenResponse),
        (status = 401, description = "Invalid, expired or revoked token", body = ErrorBody),
        (status = 429, description = "Rate limited", body = ErrorBody),
    )
)]
pub async fn refresh_token(
    State(s): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<RefreshRequest>>,
) -> AppResult<(StatusCode, Json<TokenResponse>)> {
    let token_from_body = body.and_then(|Json(r)| r.refresh_token);
    let token_from_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(|s| s.trim().to_string());

    let token =
        token_from_body.or(token_from_header).filter(|t| !t.is_empty()).ok_or(crate::error::AppError::Unauthorized)?;

    Ok((StatusCode::CREATED, Json(s.auth_service.refresh(&token).await?)))
}

/// Revoke the current access token (and optionally its paired refresh token).
#[utoipa::path(
    post, path = "/v1/auth/logout", tag = "Auth",
    request_body = Option<LogoutRequest>,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Logged out"),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    )
)]
pub async fn logout(
    State(s): State<AppState>,
    AccessClaims(claims): AccessClaims,
    body: Option<Json<LogoutRequest>>,
) -> AppResult<Json<Value>> {
    s.auth_service.logout(&claims, body.map(|b| b.0).unwrap_or_default()).await?;
    Ok(Json(json!({ "message": "Logged Out Successfully" })))
}
