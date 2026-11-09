use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, Set};

use crate::{entities::jwt_blacklist, error::AppResult};

/// Revoked-token store keyed by JWT ID (`jti`), backed by the `jwt_blacklist` table.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TokenBlacklist: Send + Sync {
    /// Revokes `jti` until `expires_at` (the token's own expiry); idempotent.
    async fn revoke(&self, jti: &str, expires_at: DateTime<Utc>) -> AppResult<()>;
    async fn is_revoked(&self, jti: &str) -> AppResult<bool>;
    /// Deletes entries whose tokens have expired anyway; returns the number removed.
    async fn purge_expired(&self) -> AppResult<u64>;
}

#[derive(Clone)]
pub struct SeaOrmTokenBlacklist {
    db: DatabaseConnection,
}

impl SeaOrmTokenBlacklist {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl TokenBlacklist for SeaOrmTokenBlacklist {
    async fn revoke(&self, jti: &str, expires_at: DateTime<Utc>) -> AppResult<()> {
        let row = jwt_blacklist::ActiveModel {
            jti: Set(jti.to_string()),
            expires_at: Set(expires_at.fixed_offset()),
            created_at: Set(Utc::now().fixed_offset()),
        };
        jwt_blacklist::Entity::insert(row).on_conflict_do_nothing().exec(&self.db).await?;
        Ok(())
    }

    async fn is_revoked(&self, jti: &str) -> AppResult<bool> {
        Ok(jwt_blacklist::Entity::find()
            .filter(jwt_blacklist::Column::Jti.eq(jti))
            .filter(jwt_blacklist::Column::ExpiresAt.gt(Utc::now()))
            .count(&self.db)
            .await?
            > 0)
    }

    async fn purge_expired(&self) -> AppResult<u64> {
        let res = jwt_blacklist::Entity::delete_many()
            .filter(jwt_blacklist::Column::ExpiresAt.lte(Utc::now()))
            .exec(&self.db)
            .await?;
        Ok(res.rows_affected)
    }
}
