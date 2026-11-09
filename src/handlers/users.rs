use axum::{
    Json,
    extract::{Multipart, Query, State},
    http::StatusCode,
};

use crate::{
    auth::{AuthUser, Role},
    error::{AppError, AppResult, ErrorBody},
    services::user::{
        ImageUploaded, ListQuery, MessageResponse, Page, ProfilePatch, ProfileResponse, ProfileUpdate, TermsRequest,
        UserDetailsResponse,
    },
    state::AppState,
};

/// Record acceptance of the latest terms and conditions.
#[utoipa::path(
    post, path = "/v1/users/tnc", tag = "Users",
    request_body = TermsRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Terms and conditions accepted", body = MessageResponse),
        (status = 403, description = "Terms not accepted", body = ErrorBody),
        (status = 404, description = "No terms published", body = ErrorBody),
    )
)]
pub async fn accept_terms(
    State(s): State<AppState>,
    au: AuthUser,
    Json(req): Json<TermsRequest>,
) -> AppResult<Json<MessageResponse>> {
    Ok(Json(s.user_service.accept_terms(au.user, req).await?))
}

/// The caller's profile.
#[utoipa::path(
    get, path = "/v1/users/profile", tag = "Users",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Current user profile", body = ProfileResponse),
    )
)]
pub async fn get_profile(State(s): State<AppState>, au: AuthUser) -> AppResult<Json<ProfileResponse>> {
    Ok(Json(s.user_service.profile(au.user)?))
}

/// Replace the profile.
#[utoipa::path(
    put, path = "/v1/users/profile", tag = "Users",
    request_body = ProfileUpdate,
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Profile replaced successfully", body = ProfileResponse),
        (status = 422, description = "Invalid field", body = ErrorBody),
    )
)]
pub async fn put_profile(
    State(s): State<AppState>,
    au: AuthUser,
    Json(req): Json<ProfileUpdate>,
) -> AppResult<(StatusCode, Json<ProfileResponse>)> {
    Ok((StatusCode::ACCEPTED, Json(s.user_service.replace_profile(au.user, req).await?)))
}

/// Update only the provided profile fields.
#[utoipa::path(
    patch, path = "/v1/users/profile", tag = "Users",
    request_body = ProfilePatch,
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Profile updated successfully", body = ProfileResponse),
        (status = 422, description = "Invalid field", body = ErrorBody),
    )
)]
pub async fn patch_profile(
    State(s): State<AppState>,
    au: AuthUser,
    Json(req): Json<ProfilePatch>,
) -> AppResult<(StatusCode, Json<ProfileResponse>)> {
    Ok((StatusCode::ACCEPTED, Json(s.user_service.patch_profile(au.user, req).await?)))
}

/// Delete the caller's account.
#[utoipa::path(
    delete, path = "/v1/users/profile", tag = "Users",
    security(("bearer" = [])),
    responses((status = 200, description = "Account deleted successfully", body = MessageResponse))
)]
pub async fn delete_profile(State(s): State<AppState>, au: AuthUser) -> AppResult<Json<MessageResponse>> {
    Ok(Json(s.user_service.delete_account(&au.user).await?))
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ProfilePictureForm {
    /// JPG, PNG or WebP image.
    #[schema(format = Binary)]
    image: String,
}

/// Upload a profile picture (multipart field `image`).
#[utoipa::path(
    put, path = "/v1/users/profile-picture", tag = "Users",
    request_body(content = ProfilePictureForm, content_type = "multipart/form-data"),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Profile picture uploaded successfully", body = ImageUploaded),
        (status = 422, description = "Unsupported or invalid image", body = ErrorBody),
    )
)]
pub async fn put_profile_picture(
    State(s): State<AppState>,
    au: AuthUser,
    mut form: Multipart,
) -> AppResult<Json<ImageUploaded>> {
    let bad = |_| AppError::Unprocessable("Invalid multipart upload".into());
    while let Some(field) = form.next_field().await.map_err(bad)? {
        if field.name() != Some("image") {
            continue;
        }
        let content_type = field.content_type().unwrap_or_default().to_owned();
        let data = field.bytes().await.map_err(bad)?;
        return Ok(Json(s.user_service.upload_picture(au.user, &content_type, data.to_vec()).await?));
    }
    Err(AppError::Unprocessable("Missing 'image' field".into()))
}

/// Request an account information report.
#[utoipa::path(
    post, path = "/v1/users/account-report", tag = "Users",
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Account report request accepted", body = MessageResponse),
        (status = 403, description = "Terms not accepted", body = ErrorBody),
    )
)]
pub async fn account_report(State(s): State<AppState>, au: AuthUser) -> AppResult<(StatusCode, Json<MessageResponse>)> {
    Ok((StatusCode::ACCEPTED, Json(s.user_service.request_account_report(&au.user).await?)))
}

/// Admin-only user listing (example of an RBAC-guarded route).
#[utoipa::path(
    get, path = "/v1/admin/users", tag = "Admin",
    params(ListQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Paginated list of users", body = Page<UserDetailsResponse>),
        (status = 401, description = "Unauthorized", body = ErrorBody),
        (status = 403, description = "Admin role required", body = ErrorBody),
    )
)]
pub async fn list(
    auth: AuthUser,
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Page<UserDetailsResponse>>> {
    auth.require_any(&[Role::Admin])?;
    Ok(Json(s.user_service.list(q).await?))
}
