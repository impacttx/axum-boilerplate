use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, JoinType, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait, Set,
    sea_query::{Alias, Expr, ExprTrait, Query, extension::postgres::PgExpr},
};
use uuid::Uuid;

use crate::{
    entities::{roles, user_role_assignments, users},
    error::AppResult,
};

#[derive(Debug, Clone, Default)]
pub struct UserFilter {
    pub search: Option<String>,
    pub role: Option<String>,
    pub skip: u64,
    pub limit: u64,
}

/// A user together with the names of their active roles.
pub type UserWithRoleNames = (users::Model, Vec<String>);

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait RbacRepository: Send + Sync {
    async fn list_roles(&self) -> AppResult<Vec<roles::Model>>;
    /// Active roles assigned to the user.
    async fn user_roles(&self, user_id: Uuid) -> AppResult<Vec<roles::Model>>;
    /// Newest first, plus the total number of matches.
    async fn list_users(&self, filter: UserFilter) -> AppResult<(Vec<UserWithRoleNames>, u64)>;
    /// Idempotent; creates the role row when it does not exist yet.
    async fn assign(&self, user_id: Uuid, role: &str, assigned_by: Uuid) -> AppResult<()>;
    /// Returns `false` when the assignment did not exist.
    async fn remove(&self, user_id: Uuid, role: &str) -> AppResult<bool>;
}

#[derive(Clone)]
pub struct SeaOrmRbacRepository {
    db: DatabaseConnection,
}

impl SeaOrmRbacRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn role_by_name(&self, name: &str) -> AppResult<Option<roles::Model>> {
        Ok(roles::Entity::find().filter(roles::Column::Name.eq(name)).one(&self.db).await?)
    }
}

/// Escapes `LIKE` wildcards so user input is matched literally.
fn like_pattern(term: &str) -> String {
    let escaped = term.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

#[async_trait]
impl RbacRepository for SeaOrmRbacRepository {
    async fn list_roles(&self) -> AppResult<Vec<roles::Model>> {
        Ok(roles::Entity::find().order_by_asc(roles::Column::Name).all(&self.db).await?)
    }

    async fn user_roles(&self, user_id: Uuid) -> AppResult<Vec<roles::Model>> {
        Ok(roles::Entity::find()
            .join(JoinType::InnerJoin, roles::Relation::UserRoleAssignments.def())
            .filter(user_role_assignments::Column::UserId.eq(user_id))
            .filter(roles::Column::IsActive.eq(true))
            .order_by_asc(roles::Column::Name)
            .all(&self.db)
            .await?)
    }

    async fn list_users(&self, f: UserFilter) -> AppResult<(Vec<UserWithRoleNames>, u64)> {
        let mut query = users::Entity::find();
        if let Some(term) = f.search.as_deref().filter(|s| !s.is_empty()) {
            let pat = like_pattern(term);
            query = query.filter(
                Condition::any()
                    .add(Expr::col(users::Column::FirstName).ilike(&pat))
                    .add(Expr::col(users::Column::LastName).ilike(&pat))
                    .add(Expr::col(users::Column::Email).ilike(&pat))
                    .add(Expr::col(users::Column::MobileNumber).ilike(&pat))
                    .add(Expr::col(users::Column::Id).cast_as(Alias::new("text")).ilike(&pat)),
            );
        }
        if let Some(role) = &f.role {
            let assigned = Query::select()
                .column(user_role_assignments::Column::UserId)
                .from(user_role_assignments::Entity)
                .inner_join(
                    roles::Entity,
                    Expr::col((roles::Entity, roles::Column::Id))
                        .equals((user_role_assignments::Entity, user_role_assignments::Column::RoleId)),
                )
                .and_where(Expr::col((roles::Entity, roles::Column::Name)).eq(role.as_str()))
                .to_owned();
            query = query.filter(users::Column::Id.in_subquery(assigned));
        }
        let total = query.clone().count(&self.db).await?;
        let page = query.order_by_desc(users::Column::CreatedAt).offset(f.skip).limit(f.limit).all(&self.db).await?;

        let ids: Vec<Uuid> = page.iter().map(|u| u.id).collect();
        let mut names: HashMap<Uuid, Vec<String>> = HashMap::new();
        if !ids.is_empty() {
            let rows = user_role_assignments::Entity::find()
                .find_also_related(roles::Entity)
                .filter(user_role_assignments::Column::UserId.is_in(ids))
                .all(&self.db)
                .await?;
            for (assignment, role) in rows {
                if let Some(role) = role.filter(|r| r.is_active) {
                    names.entry(assignment.user_id).or_default().push(role.name);
                }
            }
        }
        let items = page
            .into_iter()
            .map(|u| {
                let mut r = names.remove(&u.id).unwrap_or_default();
                r.sort();
                (u, r)
            })
            .collect();
        Ok((items, total))
    }

    async fn assign(&self, user_id: Uuid, role: &str, assigned_by: Uuid) -> AppResult<()> {
        let now = Utc::now().fixed_offset();
        let role_id = match self.role_by_name(role).await? {
            Some(r) => r.id,
            None => {
                roles::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    name: Set(role.to_owned()),
                    description: Set(None),
                    is_active: Set(true),
                    created_at: Set(now),
                }
                .insert(&self.db)
                .await?
                .id
            }
        };
        let exists = user_role_assignments::Entity::find()
            .filter(user_role_assignments::Column::UserId.eq(user_id))
            .filter(user_role_assignments::Column::RoleId.eq(role_id))
            .count(&self.db)
            .await?
            > 0;
        if !exists {
            user_role_assignments::ActiveModel {
                id: Set(Uuid::new_v4()),
                user_id: Set(user_id),
                role_id: Set(role_id),
                assigned_by: Set(Some(assigned_by)),
                created_at: Set(now),
            }
            .insert(&self.db)
            .await?;
        }
        Ok(())
    }

    async fn remove(&self, user_id: Uuid, role: &str) -> AppResult<bool> {
        let Some(role) = self.role_by_name(role).await? else {
            return Ok(false);
        };
        let res = user_role_assignments::Entity::delete_many()
            .filter(user_role_assignments::Column::UserId.eq(user_id))
            .filter(user_role_assignments::Column::RoleId.eq(role.id))
            .exec(&self.db)
            .await?;
        Ok(res.rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("a%b_c"), "%a\\%b\\_c%");
    }
}
