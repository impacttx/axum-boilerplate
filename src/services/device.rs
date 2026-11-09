use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    entities::{sea_orm_active_enums::DevicePlatformEnum, user_devices},
    error::{AppError, AppResult},
    repositories::{DeviceRepository, device::DeviceUpsert},
};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterDeviceRequest {
    pub device_id: String,
    /// `ios` or `android` (case-insensitive).
    pub platform: String,
    pub device_name: Option<String>,
    pub expo_push_token: Option<String>,
    pub voip_push_token: Option<String>,
    pub fcm_token: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct VoipTokenRequest {
    pub device_id: String,
    pub voip_push_token: String,
}

#[derive(Debug, Serialize, PartialEq, utoipa::ToSchema)]
pub struct DeviceResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeviceResponse {
    pub device_id: String,
    pub platform: Option<String>,
    pub device_name: Option<String>,
    pub has_expo_push_token: bool,
    pub has_voip_push_token: bool,
    pub has_fcm_token: bool,
    pub is_active: bool,
    pub last_active_at: DateTime<Utc>,
}

impl From<user_devices::Model> for DeviceResponse {
    fn from(d: user_devices::Model) -> Self {
        Self {
            platform: d.platform.map(|p| match p {
                DevicePlatformEnum::Ios => "ios".to_owned(),
                DevicePlatformEnum::Android => "android".to_owned(),
            }),
            device_id: d.device_id,
            device_name: d.device_name,
            has_expo_push_token: d.expo_push_token.is_some(),
            has_voip_push_token: d.voip_push_token.is_some(),
            has_fcm_token: d.fcm_token.is_some(),
            is_active: d.is_active,
            last_active_at: d.last_active_at.to_utc(),
        }
    }
}

#[derive(Clone)]
pub struct DeviceService {
    devices: Arc<dyn DeviceRepository>,
}

impl DeviceService {
    pub fn new(devices: Arc<dyn DeviceRepository>) -> Self {
        Self { devices }
    }

    pub async fn register(&self, user_id: Uuid, req: RegisterDeviceRequest) -> AppResult<DeviceResult> {
        let platform = match req.platform.trim().to_ascii_lowercase().as_str() {
            "ios" => DevicePlatformEnum::Ios,
            "android" => DevicePlatformEnum::Android,
            other => {
                return Err(AppError::BadRequest(format!("Invalid platform '{other}'. Must be 'ios' or 'android'")));
            }
        };
        if req.device_id.trim().is_empty() {
            return Err(AppError::BadRequest("device_id is required".into()));
        }
        self.devices
            .upsert(DeviceUpsert {
                user_id,
                device_id: req.device_id,
                platform,
                device_name: req.device_name,
                expo_push_token: req.expo_push_token,
                voip_push_token: req.voip_push_token,
                fcm_token: req.fcm_token,
            })
            .await?;
        Ok(DeviceResult { success: true, message: "Device registered successfully".into() })
    }

    pub async fn set_voip_token(&self, user_id: Uuid, req: VoipTokenRequest) -> AppResult<DeviceResult> {
        let mut d = self.owned(user_id, &req.device_id).await?;
        let now = Utc::now().fixed_offset();
        d.voip_push_token = Some(req.voip_push_token);
        d.last_active_at = now;
        d.updated_at = Some(now);
        self.devices.save(d).await?;
        Ok(DeviceResult { success: true, message: "VoIP token updated".into() })
    }

    /// Soft-deletes the device and clears its push tokens.
    pub async fn unregister(&self, user_id: Uuid, device_id: &str) -> AppResult<DeviceResult> {
        let mut d = self.owned(user_id, device_id).await?;
        d.is_active = false;
        d.expo_push_token = None;
        d.voip_push_token = None;
        d.fcm_token = None;
        d.updated_at = Some(Utc::now().fixed_offset());
        self.devices.save(d).await?;
        Ok(DeviceResult { success: true, message: "Device unregistered".into() })
    }

    pub async fn list(&self, user_id: Uuid) -> AppResult<Vec<DeviceResponse>> {
        Ok(self.devices.list_active(user_id).await?.into_iter().map(Into::into).collect())
    }

    async fn owned(&self, user_id: Uuid, device_id: &str) -> AppResult<user_devices::Model> {
        self.devices.find_owned(user_id, device_id).await?.ok_or(AppError::NotFound("Device"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::device::MockDeviceRepository;

    fn req(platform: &str) -> RegisterDeviceRequest {
        RegisterDeviceRequest {
            device_id: "dev-1".into(),
            platform: platform.into(),
            device_name: None,
            expo_push_token: Some("tok".into()),
            voip_push_token: None,
            fcm_token: None,
        }
    }

    fn device(user: Uuid) -> user_devices::Model {
        let now = Utc::now().fixed_offset();
        user_devices::Model {
            user_id: user,
            device_id: "dev-1".into(),
            platform: Some(DevicePlatformEnum::Ios),
            device_name: None,
            expo_push_token: Some("t".into()),
            voip_push_token: Some("v".into()),
            fcm_token: Some("f".into()),
            is_active: true,
            last_active_at: now,
            created_at: now,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn register_normalizes_platform_and_rejects_unknown() {
        let mut repo = MockDeviceRepository::new();
        repo.expect_upsert().withf(|d| d.platform == DevicePlatformEnum::Android).times(1).returning(|_| Ok(()));
        let svc = DeviceService::new(Arc::new(repo));
        assert!(svc.register(Uuid::new_v4(), req("ANDROID")).await.unwrap().success);
        assert!(matches!(svc.register(Uuid::new_v4(), req("windows")).await, Err(AppError::BadRequest(_))));
    }

    #[tokio::test]
    async fn unregister_clears_tokens_and_requires_ownership() {
        let uid = Uuid::new_v4();
        let mut repo = MockDeviceRepository::new();
        repo.expect_find_owned()
            .withf(move |u, d| *u == uid && d == "dev-1")
            .returning(move |u, _| Ok(Some(device(u))));
        repo.expect_find_owned().withf(|_, d| d == "other").returning(|_, _| Ok(None));
        repo.expect_save()
            .withf(|d| {
                !d.is_active && d.expo_push_token.is_none() && d.voip_push_token.is_none() && d.fcm_token.is_none()
            })
            .times(1)
            .returning(|_| Ok(()));
        let svc = DeviceService::new(Arc::new(repo));
        svc.unregister(uid, "dev-1").await.unwrap();
        assert!(matches!(svc.unregister(uid, "other").await, Err(AppError::NotFound(_))));
    }
}
