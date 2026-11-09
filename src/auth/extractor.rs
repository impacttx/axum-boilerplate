use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};

use super::{Role, jwt::Claims};
use crate::{
    entities::users,
    error::{AppError, AppResult},
    state::AppState,
};

fn bearer(parts: &Parts) -> AppResult<&str> {
    parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)
}

/// Authenticated caller: valid, non-revoked access token for an existing active user.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: users::Model,
    pub roles: Vec<Role>,
}

impl AuthUser {
    /// RBAC check: admins (the `is_admin` flag or the admin role) always pass.
    pub fn has_any_role(&self, allowed: &[Role]) -> bool {
        self.user.is_admin || self.roles.contains(&Role::Admin) || self.roles.iter().any(|r| allowed.contains(r))
    }

    pub fn require_any(&self, allowed: &[Role]) -> AppResult<()> {
        if self.has_any_role(allowed) { Ok(()) } else { Err(AppError::Forbidden) }
    }

    fn require_msg(&self, allowed: &[Role], msg: &str) -> AppResult<()> {
        if self.has_any_role(allowed) { Ok(()) } else { Err(AppError::Denied(msg.into())) }
    }

    /// `is_admin` flag or the admin role.
    pub fn require_admin(&self) -> AppResult<()> {
        self.require_msg(&[], "Admin access required")
    }

    /// Sales, operations, marketing or admin.
    pub fn require_staff(&self) -> AppResult<()> {
        self.require_msg(&[Role::Sales, Role::Operations, Role::Marketing], "Staff access required")
    }

    /// Sales or admin.
    pub fn require_sales(&self) -> AppResult<()> {
        self.require_msg(&[Role::Sales], "Sales access required")
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        Ok(state.auth_service.authenticate(bearer(parts)?).await?.1)
    }
}

/// Optional authenticated caller: returns Some(AuthUser) if valid Bearer token provided, None otherwise.
#[derive(Debug, Clone)]
pub struct OptionalAuthUser(pub Option<AuthUser>);

impl FromRequestParts<AppState> for OptionalAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let auth = match bearer(parts) {
            Ok(token) => state.auth_service.authenticate(token).await.ok().map(|(_, u)| u),
            Err(_) => None,
        };
        Ok(OptionalAuthUser(auth))
    }
}

/// Verified access-token claims (for logout, which needs the `jti`).
pub struct AccessClaims(pub Claims);

impl FromRequestParts<AppState> for AccessClaims {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        Ok(AccessClaims(state.auth_service.authenticate(bearer(parts)?).await?.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::user_model;

    fn auth(is_admin: bool, roles: Vec<Role>) -> AuthUser {
        AuthUser { user: user_model("a@b.com", is_admin), roles }
    }

    #[test]
    fn rbac_rules() {
        assert!(auth(false, vec![Role::Sales]).has_any_role(&[Role::Sales, Role::Operations]));
        assert!(!auth(false, vec![Role::Customer]).has_any_role(&[Role::Sales]));
        assert!(!auth(false, vec![]).has_any_role(&[Role::Sales]));
        assert!(auth(true, vec![]).has_any_role(&[Role::Marketing]));
        assert!(auth(false, vec![Role::Admin]).has_any_role(&[Role::Marketing]));
        assert!(matches!(auth(false, vec![]).require_any(&[Role::Sales]), Err(AppError::Forbidden)));
        assert!(auth(false, vec![Role::Marketing]).require_staff().is_ok());
        assert!(auth(false, vec![Role::Customer]).require_staff().is_err());
        assert!(auth(false, vec![Role::Sales]).require_admin().is_err());
        assert!(auth(true, vec![]).require_admin().is_ok());
    }
}
