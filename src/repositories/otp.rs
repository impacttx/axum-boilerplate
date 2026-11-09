use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::{DatabaseConnection, EntityTrait, Set, sea_query::OnConflict};

use crate::{entities::mobile_otp_records, error::AppResult};

/// One-time-password store; at most one record per mobile number.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait OtpRepository: Send + Sync {
    /// Stores the hashed OTP, replacing any existing one for the number.
    async fn upsert(&self, mobile: &str, hashed_otp: &str, expires_at: DateTime<Utc>) -> AppResult<()>;
    async fn find(&self, mobile: &str) -> AppResult<Option<mobile_otp_records::Model>>;
    async fn delete(&self, mobile: &str) -> AppResult<()>;
}

#[derive(Clone)]
pub struct SeaOrmOtpRepository {
    db: DatabaseConnection,
}

impl SeaOrmOtpRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl OtpRepository for SeaOrmOtpRepository {
    async fn upsert(&self, mobile: &str, hashed_otp: &str, expires_at: DateTime<Utc>) -> AppResult<()> {
        let row = mobile_otp_records::ActiveModel {
            mobile_number: Set(mobile.to_owned()),
            hashed_otp: Set(hashed_otp.to_owned()),
            expires_at: Set(expires_at.fixed_offset()),
            created_at: Set(Utc::now().fixed_offset()),
        };
        mobile_otp_records::Entity::insert(row)
            .on_conflict(
                OnConflict::column(mobile_otp_records::Column::MobileNumber)
                    .update_columns([
                        mobile_otp_records::Column::HashedOtp,
                        mobile_otp_records::Column::ExpiresAt,
                        mobile_otp_records::Column::CreatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn find(&self, mobile: &str) -> AppResult<Option<mobile_otp_records::Model>> {
        Ok(mobile_otp_records::Entity::find_by_id(mobile).one(&self.db).await?)
    }

    async fn delete(&self, mobile: &str) -> AppResult<()> {
        mobile_otp_records::Entity::delete_by_id(mobile).exec(&self.db).await?;
        Ok(())
    }
}
