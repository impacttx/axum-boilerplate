use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{AppResult, ErrorBody},
    services::{
        rbac::{RoleChange, RoleResponse, StatusChange, UserList, UserRoles, UserSearch},
        user::MessageResponse,
    },
    state::AppState,
};

/// All roles (admin only).
#[utoipa::path(
    get, path = "/v1/admin/rbac/roles", tag = "RBAC",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "List of all roles", body = Vec<RoleResponse>),
        (status = 403, description = "Admin access required", body = ErrorBody),
    )
)]
pub async fn roles(State(s): State<AppState>, au: AuthUser) -> AppResult<Json<Vec<RoleResponse>>> {
    au.require_admin()?;
    Ok(Json(s.rbac_service.roles().await?))
}

/// Users with their roles, searchable and filterable by role (admin only).
#[utoipa::path(
    get, path = "/v1/admin/rbac/users", tag = "RBAC",
    params(UserSearch),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Paginated list of users with roles", body = UserList),
        (status = 403, description = "Admin access required", body = ErrorBody),
    )
)]
pub async fn users(State(s): State<AppState>, au: AuthUser, Query(q): Query<UserSearch>) -> AppResult<Json<UserList>> {
    au.require_admin()?;
    Ok(Json(s.rbac_service.users(q).await?))
}

/// Roles of one user (admin only).
#[utoipa::path(
    get, path = "/v1/admin/rbac/users/{user_id}/roles", tag = "RBAC",
    params(("user_id" = Uuid, Path)),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Roles assigned to user", body = UserRoles),
        (status = 403, description = "Admin access required", body = ErrorBody),
    )
)]
pub async fn user_roles(
    State(s): State<AppState>,
    au: AuthUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<UserRoles>> {
    au.require_admin()?;
    Ok(Json(s.rbac_service.user_roles(user_id).await?))
}

/// The caller's own roles.
#[utoipa::path(
    get, path = "/v1/admin/rbac/me/roles", tag = "RBAC",
    security(("bearer" = [])),
    responses((status = 200, description = "Current user's roles", body = UserRoles))
)]
pub async fn my_roles(State(s): State<AppState>, au: AuthUser) -> AppResult<Json<UserRoles>> {
    Ok(Json(s.rbac_service.user_roles(au.user.id).await?))
}

/// Assign a role. Staff/admin roles need an email on an allowed staff domain (admin only).
#[utoipa::path(
    post, path = "/v1/admin/rbac/assign", tag = "RBAC",
    request_body = RoleChange,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Role assigned successfully", body = MessageResponse),
        (status = 400, description = "Email not eligible for staff roles", body = ErrorBody),
        (status = 404, description = "User not found", body = ErrorBody),
    )
)]
pub async fn assign(
    State(s): State<AppState>,
    au: AuthUser,
    Json(req): Json<RoleChange>,
) -> AppResult<Json<MessageResponse>> {
    au.require_admin()?;
    Ok(Json(s.rbac_service.assign(au.user.id, req).await?))
}

/// Remove a role (admin only).
#[utoipa::path(
    post, path = "/v1/admin/rbac/remove", tag = "RBAC",
    request_body = RoleChange,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Role removed successfully", body = MessageResponse),
        (status = 404, description = "Role assignment not found", body = ErrorBody),
    )
)]
pub async fn remove(
    State(s): State<AppState>,
    au: AuthUser,
    Json(req): Json<RoleChange>,
) -> AppResult<Json<MessageResponse>> {
    au.require_admin()?;
    Ok(Json(s.rbac_service.remove(req).await?))
}

/// Activate or deactivate a user (admin only).
#[utoipa::path(
    post, path = "/v1/admin/rbac/users/status", tag = "RBAC",
    request_body = StatusChange,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "User status updated successfully", body = MessageResponse),
        (status = 404, description = "User not found", body = ErrorBody),
    )
)]
pub async fn set_status(
    State(s): State<AppState>,
    au: AuthUser,
    Json(req): Json<StatusChange>,
) -> AppResult<Json<MessageResponse>> {
    au.require_admin()?;
    Ok(Json(s.rbac_service.set_status(req).await?))
}
