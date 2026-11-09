use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};
use uuid::Uuid;

use crate::{
    entities::{sea_orm_active_enums::DevicePlatformEnum, user_devices},
    error::AppResult,
};

#[derive(Debug, Clone)]
pub struct DeviceUpsert {
    pub user_id: Uuid,
    pub device_id: String,
    pub platform: DevicePlatformEnum,
    pub device_name: Option<String>,
    pub expo_push_token: Option<String>,
    pub voip_push_token: Option<String>,
    pub fcm_token: Option<String>,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Registers a device or re-assigns an existing one to the user, marking it active.
    async fn upsert(&self, d: DeviceUpsert) -> AppResult<()>;
    /// Looks a device up only if it belongs to `user_id`.
    async fn find_owned(&self, user_id: Uuid, device_id: &str) -> AppResult<Option<user_devices::Model>>;
    async fn save(&self, device: user_devices::Model) -> AppResult<()>;
    async fn list_active(&self, user_id: Uuid) -> AppResult<Vec<user_devices::Model>>;
}

#[derive(Clone)]
pub struct SeaOrmDeviceRepository {
    db: DatabaseConnection,
}

impl SeaOrmDeviceRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl DeviceRepository for SeaOrmDeviceRepository {
    async fn upsert(&self, d: DeviceUpsert) -> AppResult<()> {
        let now = Utc::now().fixed_offset();
        match user_devices::Entity::find_by_id(d.device_id.clone()).one(&self.db).await? {
            Some(existing) => {
                let mut m: user_devices::ActiveModel = existing.into();
                m.user_id = Set(d.user_id);
                m.platform = Set(Some(d.platform));
                m.device_name = Set(d.device_name);
                m.expo_push_token = Set(d.expo_push_token);
                m.voip_push_token = Set(d.voip_push_token);
                m.fcm_token = Set(d.fcm_token);
                m.is_active = Set(true);
                m.last_active_at = Set(now);
                m.updated_at = Set(Some(now));
                m.update(&self.db).await?;
            }
            None => {
                user_devices::ActiveModel {
                    user_id: Set(d.user_id),
                    device_id: Set(d.device_id),
                    platform: Set(Some(d.platform)),
                    device_name: Set(d.device_name),
                    expo_push_token: Set(d.expo_push_token),
                    voip_push_token: Set(d.voip_push_token),
                    fcm_token: Set(d.fcm_token),
                    is_active: Set(true),
                    last_active_at: Set(now),
                    created_at: Set(now),
                    updated_at: Set(None),
                }
                .insert(&self.db)
                .await?;
            }
        }
        Ok(())
    }

    async fn find_owned(&self, user_id: Uuid, device_id: &str) -> AppResult<Option<user_devices::Model>> {
        Ok(user_devices::Entity::find_by_id(device_id)
            .filter(user_devices::Column::UserId.eq(user_id))
            .one(&self.db)
            .await?)
    }

    async fn save(&self, device: user_devices::Model) -> AppResult<()> {
        let m = user_devices::ActiveModel::from(device).reset_all();
        m.update(&self.db).await?;
        Ok(())
    }

    async fn list_active(&self, user_id: Uuid) -> AppResult<Vec<user_devices::Model>> {
        Ok(user_devices::Entity::find()
            .filter(user_devices::Column::UserId.eq(user_id))
            .filter(user_devices::Column::IsActive.eq(true))
            .order_by_desc(user_devices::Column::LastActiveAt)
            .all(&self.db)
            .await?)
    }
}
