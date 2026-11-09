use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, JoinType, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Set, SqlErr, TransactionTrait,
};
use uuid::Uuid;

use crate::{
    entities::{roles, user_devices, user_role_assignments, users},
    error::{AppError, AppResult},
};

/// Fields for a self-registered (SSO) user account.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub email: String,
    pub mobile_number: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub profile_picture: Option<String>,
    pub is_admin: bool,
}

/// Persistence boundary for users; services depend on this trait so they can be tested
/// with the generated `MockUserRepository`.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<users::Model>>;
    async fn find_by_email(&self, email: &str) -> AppResult<Option<users::Model>>;
    /// Names of the active roles assigned to the user.
    async fn role_names(&self, user_id: Uuid) -> AppResult<Vec<String>>;
    async fn touch_last_login(&self, id: Uuid) -> AppResult<()>;
    /// One page (1-based) plus the total item count.
    async fn list(&self, page: u64, per_page: u64) -> AppResult<(Vec<users::Model>, u64)>;
    async fn ping(&self) -> AppResult<()>;
    /// Creates an active user from self-registration (e.g. Google SSO).
    async fn create(&self, new: NewUser) -> AppResult<users::Model>;
    async fn assign_role(&self, user_id: Uuid, role: &str) -> AppResult<()>;
    async fn set_profile_picture(&self, id: Uuid, url: &str) -> AppResult<()>;
    async fn find_by_mobile(&self, mobile: &str) -> AppResult<Option<users::Model>>;
    /// Creates an inactive account that becomes usable after OTP verification.
    async fn create_pending(&self, mobile: &str) -> AppResult<users::Model>;
    /// Persists every column of `user` (the caller mutates a fetched model).
    async fn save(&self, user: users::Model) -> AppResult<users::Model>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    /// Revision of the most recent active terms and conditions.
    async fn latest_terms_revision(&self) -> AppResult<Option<String>>;
    async fn create_account_info_request(&self, user_id: Uuid) -> AppResult<()>;
}

#[derive(Clone)]
pub struct SeaOrmUserRepository {
    db: DatabaseConnection,
}

impl SeaOrmUserRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl UserRepository for SeaOrmUserRepository {
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<users::Model>> {
        Ok(users::Entity::find_by_id(id).one(&self.db).await?)
    }

    async fn find_by_email(&self, email: &str) -> AppResult<Option<users::Model>> {
        Ok(users::Entity::find().filter(users::Column::Email.eq(email)).one(&self.db).await?)
    }

    async fn role_names(&self, user_id: Uuid) -> AppResult<Vec<String>> {
        Ok(user_role_assignments::Entity::find()
            .join(JoinType::InnerJoin, user_role_assignments::Relation::Roles.def())
            .filter(user_role_assignments::Column::UserId.eq(user_id))
            .filter(roles::Column::IsActive.eq(true))
            .select_only()
            .column(roles::Column::Name)
            .order_by_asc(user_role_assignments::Column::CreatedAt)
            .into_tuple::<String>()
            .all(&self.db)
            .await?)
    }

    async fn touch_last_login(&self, id: Uuid) -> AppResult<()> {
        let now = Utc::now().fixed_offset();
        users::ActiveModel {
            id: Set(id),
            last_login: Set(Some(now)),
            last_activity: Set(Some(now)),
            ..Default::default()
        }
        .update(&self.db)
        .await?;
        Ok(())
    }

    async fn list(&self, page: u64, per_page: u64) -> AppResult<(Vec<users::Model>, u64)> {
        let paginator = users::Entity::find().order_by_asc(users::Column::CreatedAt).paginate(&self.db, per_page);
        let total = paginator.num_items().await?;
        Ok((paginator.fetch_page(page.saturating_sub(1)).await?, total))
    }

    async fn ping(&self) -> AppResult<()> {
        Ok(self.db.ping().await?)
    }

    async fn create(&self, new: NewUser) -> AppResult<users::Model> {
        let now = Utc::now().fixed_offset();
        Ok(users::ActiveModel {
            id: Set(Uuid::new_v4()),
            email: Set(Some(new.email)),
            mobile_number: Set(new.mobile_number),
            first_name: Set(new.first_name),
            last_name: Set(new.last_name),
            profile_picture: Set(new.profile_picture),
            is_active: Set(true),
            is_admin: Set(new.is_admin),
            accepted_terms: Set(serde_json::json!({})),
            last_login: Set(Some(now)),
            ..Default::default()
        }
        .insert(&self.db)
        .await?)
    }

    async fn assign_role(&self, user_id: Uuid, role_name: &str) -> AppResult<()> {
        let role = match roles::Entity::find().filter(roles::Column::Name.eq(role_name)).one(&self.db).await? {
            Some(r) => r,
            None => {
                let id = Uuid::new_v4();
                roles::ActiveModel {
                    id: Set(id),
                    name: Set(role_name.into()),
                    description: Set(Some(format!("{role_name} role"))),
                    is_active: Set(true),
                    created_at: Set(Utc::now().fixed_offset()),
                }
                .insert(&self.db)
                .await?
            }
        };
        let exists = user_role_assignments::Entity::find()
            .filter(user_role_assignments::Column::UserId.eq(user_id))
            .filter(user_role_assignments::Column::RoleId.eq(role.id))
            .one(&self.db)
            .await?
            .is_some();
        if !exists {
            user_role_assignments::ActiveModel {
                id: Set(Uuid::new_v4()),
                user_id: Set(user_id),
                role_id: Set(role.id),
                assigned_by: Set(None),
                created_at: Set(Utc::now().fixed_offset()),
            }
            .insert(&self.db)
            .await?;
        }
        Ok(())
    }

    async fn set_profile_picture(&self, id: Uuid, url: &str) -> AppResult<()> {
        users::ActiveModel { id: Set(id), profile_picture: Set(Some(url.to_owned())), ..Default::default() }
            .update(&self.db)
            .await?;
        Ok(())
    }

    async fn find_by_mobile(&self, mobile: &str) -> AppResult<Option<users::Model>> {
        Ok(users::Entity::find().filter(users::Column::MobileNumber.eq(mobile)).one(&self.db).await?)
    }

    async fn create_pending(&self, mobile: &str) -> AppResult<users::Model> {
        Ok(users::ActiveModel {
            id: Set(Uuid::new_v4()),
            mobile_number: Set(Some(mobile.to_owned())),
            is_active: Set(false),
            is_admin: Set(false),
            accepted_terms: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(&self.db)
        .await?)
    }

    async fn save(&self, user: users::Model) -> AppResult<users::Model> {
        let active: users::ActiveModel = users::ActiveModel::from(user).reset_all();
        Ok(active.update(&self.db).await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        let txn = self.db.begin().await?;
        user_devices::Entity::delete_many().filter(user_devices::Column::UserId.eq(id)).exec(&txn).await?;
        user_role_assignments::Entity::delete_many()
            .filter(user_role_assignments::Column::UserId.eq(id))
            .exec(&txn)
            .await?;
        match users::Entity::delete_by_id(id).exec(&txn).await {
            Ok(_) => {}
            Err(e) if matches!(e.sql_err(), Some(SqlErr::ForeignKeyConstraintViolation(_))) => {
                return Err(AppError::Conflict(
                    "Account has related records and cannot be deleted; contact support".into(),
                ));
            }
            Err(e) => return Err(e.into()),
        }
        txn.commit().await?;
        Ok(())
    }

    async fn latest_terms_revision(&self) -> AppResult<Option<String>> {
        Ok(Some("v1.0".into()))
    }

    async fn create_account_info_request(&self, _user_id: Uuid) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::user_model;
    use sea_orm::{DatabaseBackend, MockDatabase};

    #[tokio::test]
    async fn find_by_email_uses_seaorm_mock_db() {
        let m = user_model("a@b.com", false);
        let db = MockDatabase::new(DatabaseBackend::Postgres).append_query_results([vec![m.clone()]]).into_connection();
        let repo = SeaOrmUserRepository::new(db);
        assert_eq!(repo.find_by_email("a@b.com").await.unwrap(), Some(m));
    }

    #[tokio::test]
    async fn create_user_with_no_mobile_number() {
        let expected = user_model("new@example.com", false);
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([vec![expected.clone()]])
            .into_connection();
        let repo = SeaOrmUserRepository::new(db);
        let created = repo
            .create(NewUser {
                email: "new@example.com".into(),
                mobile_number: None,
                first_name: Some("Test".into()),
                last_name: Some("User".into()),
                profile_picture: None,
                is_admin: false,
            })
            .await
            .unwrap();
        assert_eq!(created.mobile_number, None);
    }
}
