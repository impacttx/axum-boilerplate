use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::Role,
    entities::{roles, users},
    error::{AppError, AppResult},
    repositories::{RbacRepository, UserRepository, rbac::UserFilter},
    services::user::MessageResponse,
};

#[derive(Debug, Serialize, PartialEq, utoipa::ToSchema)]
pub struct RoleResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
}

impl From<roles::Model> for RoleResponse {
    fn from(r: roles::Model) -> Self {
        Self { id: r.id, name: r.name, description: r.description, is_active: r.is_active }
    }
}

#[derive(Debug, Serialize, PartialEq, utoipa::ToSchema)]
pub struct UserWithRoles {
    pub id: Uuid,
    pub email: Option<String>,
    pub mobile_number: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub profile_picture: Option<String>,
    pub is_active: bool,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub roles: Vec<String>,
}

impl UserWithRoles {
    fn new(u: users::Model, roles: Vec<String>) -> Self {
        Self {
            id: u.id,
            email: u.email,
            mobile_number: u.mobile_number,
            first_name: u.first_name,
            last_name: u.last_name,
            profile_picture: u.profile_picture,
            is_active: u.is_active,
            is_admin: u.is_admin,
            created_at: u.created_at.to_utc(),
            roles,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, utoipa::ToSchema)]
pub struct UserList {
    pub items: Vec<UserWithRoles>,
    pub total: u64,
}

#[derive(Debug, Serialize, PartialEq, utoipa::ToSchema)]
pub struct UserRoles {
    pub user_id: Uuid,
    pub roles: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UserSearch {
    /// Case-insensitive match on name, email, mobile number or id.
    pub search: Option<String>,
    pub role: Option<Role>,
    #[serde(default)]
    pub skip: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_limit() -> u64 {
    100
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RoleChange {
    pub user_id: Uuid,
    pub role: Role,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StatusChange {
    pub user_id: Uuid,
    pub is_active: bool,
}

#[derive(Clone)]
pub struct RbacService {
    users: Arc<dyn UserRepository>,
    rbac: Arc<dyn RbacRepository>,
    staff_domains: Arc<Vec<String>>,
}

impl RbacService {
    pub fn new(users: Arc<dyn UserRepository>, rbac: Arc<dyn RbacRepository>, staff_domains: Vec<String>) -> Self {
        Self { users, rbac, staff_domains: Arc::new(staff_domains) }
    }

    pub async fn roles(&self) -> AppResult<Vec<RoleResponse>> {
        Ok(self.rbac.list_roles().await?.into_iter().map(Into::into).collect())
    }

    pub async fn users(&self, q: UserSearch) -> AppResult<UserList> {
        let (rows, total) = self
            .rbac
            .list_users(UserFilter {
                search: q.search.map(|s| s.trim().to_owned()),
                role: q.role.map(|r| r.as_str().to_owned()),
                skip: q.skip,
                limit: q.limit.clamp(1, 500),
            })
            .await?;
        Ok(UserList { items: rows.into_iter().map(|(u, r)| UserWithRoles::new(u, r)).collect(), total })
    }

    pub async fn user_roles(&self, user_id: Uuid) -> AppResult<UserRoles> {
        Ok(UserRoles { user_id, roles: self.rbac.user_roles(user_id).await?.into_iter().map(|r| r.name).collect() })
    }

    pub async fn assign(&self, admin_id: Uuid, req: RoleChange) -> AppResult<MessageResponse> {
        let user = self.user(req.user_id).await?;
        if req.role != Role::Customer {
            self.check_staff_email(user.email.as_deref())?;
        }
        self.rbac.assign(req.user_id, req.role.as_str(), admin_id).await?;
        Ok(MessageResponse::ok(format!("Role '{}' assigned successfully", req.role)))
    }

    pub async fn remove(&self, req: RoleChange) -> AppResult<MessageResponse> {
        if !self.rbac.remove(req.user_id, req.role.as_str()).await? {
            return Err(AppError::NotFound("Role assignment"));
        }
        Ok(MessageResponse::ok(format!("Role '{}' removed successfully", req.role)))
    }

    pub async fn set_status(&self, req: StatusChange) -> AppResult<MessageResponse> {
        let mut user = self.user(req.user_id).await?;
        user.is_active = req.is_active;
        user.updated_at = Some(Utc::now().fixed_offset());
        self.users.save(user).await?;
        Ok(MessageResponse::ok(format!(
            "User account {} successfully",
            if req.is_active { "activated" } else { "deactivated" }
        )))
    }

    async fn user(&self, id: Uuid) -> AppResult<users::Model> {
        self.users.find_by_id(id).await?.ok_or(AppError::NotFound("User"))
    }

    /// Staff and admin roles are limited to company email domains.
    fn check_staff_email(&self, email: Option<&str>) -> AppResult<()> {
        let Some(email) = email.filter(|e| !e.is_empty()) else {
            return Err(AppError::BadRequest(
                "User must have a registered email address to be assigned a staff/admin role.".into(),
            ));
        };
        let domain = email.rsplit_once('@').map_or("", |(_, d)| d).to_lowercase();
        if self.staff_domains.contains(&domain) {
            return Ok(());
        }
        Err(AppError::BadRequest(format!(
            "Email domain '@{domain}' is not authorized for staff/admin roles. Allowed staff domains: {}",
            self.staff_domains.iter().map(|d| format!("@{d}")).collect::<Vec<_>>().join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        repositories::{rbac::MockRbacRepository, user::MockUserRepository},
        test_support::user_model,
    };

    fn svc(users: MockUserRepository, rbac: MockRbacRepository) -> RbacService {
        RbacService::new(Arc::new(users), Arc::new(rbac), vec!["hrpropertymaintenance.ca".into()])
    }

    fn users_with(email: &str) -> MockUserRepository {
        let u = user_model(email, false);
        let mut users = MockUserRepository::new();
        users.expect_find_by_id().returning(move |_| Ok(Some(u.clone())));
        users
    }

    #[tokio::test]
    async fn staff_roles_require_an_allowed_email_domain() {
        let mut rbac = MockRbacRepository::new();
        rbac.expect_assign().times(1).returning(|_, _, _| Ok(()));
        let ok = svc(users_with("a@HRPropertyMaintenance.ca"), rbac)
            .assign(Uuid::new_v4(), RoleChange { user_id: Uuid::new_v4(), role: Role::Sales })
            .await
            .unwrap();
        assert_eq!(ok.message, "Role 'sales' assigned successfully");

        let err = svc(users_with("a@gmail.com"), MockRbacRepository::new())
            .assign(Uuid::new_v4(), RoleChange { user_id: Uuid::new_v4(), role: Role::Admin })
            .await;
        assert!(matches!(err, Err(AppError::BadRequest(m)) if m.contains("@gmail.com")));
    }

    #[tokio::test]
    async fn customer_role_needs_no_staff_email() {
        let mut rbac = MockRbacRepository::new();
        rbac.expect_assign().times(1).returning(|_, _, _| Ok(()));
        svc(users_with("a@gmail.com"), rbac)
            .assign(Uuid::new_v4(), RoleChange { user_id: Uuid::new_v4(), role: Role::Customer })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn remove_missing_assignment_is_404() {
        let mut rbac = MockRbacRepository::new();
        rbac.expect_remove().returning(|_, _| Ok(false));
        let err = svc(MockUserRepository::new(), rbac)
            .remove(RoleChange { user_id: Uuid::new_v4(), role: Role::Sales })
            .await;
        assert!(matches!(err, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn set_status_persists_and_reports() {
        let mut users = users_with("a@b.com");
        users.expect_save().withf(|u| !u.is_active).times(1).returning(Ok);
        let r = svc(users, MockRbacRepository::new())
            .set_status(StatusChange { user_id: Uuid::new_v4(), is_active: false })
            .await
            .unwrap();
        assert_eq!(r.message, "User account deactivated successfully");
    }
}
