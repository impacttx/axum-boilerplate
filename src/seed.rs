//! Idempotent startup seeding: the system roles and the super-admin account.

use anyhow::{Context, Result};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, TransactionTrait};
use uuid::Uuid;

use crate::{
    auth::{Role, password},
    config::AdminConfig,
    entities::{roles, user_role_assignments, users},
};

fn needs_hash_upgrade(stored: Option<&str>) -> bool {
    !stored.is_some_and(password::is_hash)
}

pub async fn run(db: &DatabaseConnection, admin: &AdminConfig) -> Result<()> {
    let password_hash =
        password::hash(admin.password.clone()).await.map_err(|e| anyhow::anyhow!("hashing admin password: {e}"))?;
    let txn = db.begin().await?;
    let now = Utc::now().fixed_offset();

    // 1. Roles
    let mut admin_role_id = None;
    for role in Role::ALL {
        let existing = roles::Entity::find().filter(roles::Column::Name.eq(role.as_str())).one(&txn).await?;
        let id = match existing {
            Some(r) => r.id,
            None => {
                let id = Uuid::new_v4();
                roles::ActiveModel {
                    id: Set(id),
                    name: Set(role.as_str().into()),
                    description: Set(Some(format!("{} role", capitalize(role.as_str())))),
                    is_active: Set(true),
                    created_at: Set(now),
                }
                .insert(&txn)
                .await?;
                tracing::info!(role = role.as_str(), "seeded role");
                id
            }
        };
        if role == Role::Admin {
            admin_role_id = Some(id);
        }
    }
    let admin_role_id = admin_role_id.context("admin role missing")?;

    // 2. Super admin
    let existing = users::Entity::find().filter(users::Column::Email.eq(&admin.email)).one(&txn).await?;
    let admin_id = match existing {
        Some(user) => {
            if needs_hash_upgrade(user.hashed_password.as_deref()) {
                let id = user.id;
                let mut active: users::ActiveModel = user.into();
                active.hashed_password = Set(Some(password_hash));
                active.updated_at = Set(Some(now));
                active.update(&txn).await?;
                tracing::warn!("super-admin had no valid password hash; set it from ADMIN_PASSWORD");
                id
            } else {
                user.id
            }
        }
        None => {
            let id = Uuid::new_v4();
            users::ActiveModel {
                id: Set(id),
                email: Set(Some(admin.email.clone())),
                mobile_number: Set(Some(admin.mobile.clone())),
                first_name: Set(Some("Admin".into())),
                last_name: Set(Some("User".into())),
                bio: Set(Some("Super admin user for initial setup".into())),
                hashed_password: Set(Some(password_hash)),
                is_active: Set(true),
                is_admin: Set(true),
                accepted_terms: Set(serde_json::json!({})),
                created_at: Set(now),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .context("creating super admin")?;
            tracing::info!(email = %admin.email, "seeded super admin");
            id
        }
    };

    // 3. Admin role assignment
    let assigned = user_role_assignments::Entity::find()
        .filter(user_role_assignments::Column::UserId.eq(admin_id))
        .filter(user_role_assignments::Column::RoleId.eq(admin_role_id))
        .one(&txn)
        .await?;
    if assigned.is_none() {
        user_role_assignments::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(admin_id),
            role_id: Set(admin_role_id),
            assigned_by: Set(None),
            created_at: Set(now),
        }
        .insert(&txn)
        .await?;
    }

    txn.commit().await?;
    Ok(())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_only_non_hash_passwords() {
        assert!(needs_hash_upgrade(Some("legacy-plaintext")));
        assert!(needs_hash_upgrade(None));
        assert!(!needs_hash_upgrade(Some("$argon2id$v=19$m=65536,t=2,p=2$abc$def")));
    }
}
