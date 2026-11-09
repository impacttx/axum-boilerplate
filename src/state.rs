use std::sync::Arc;

use crate::{
    auth::{google::GoogleVerifier, jwt::JwtKeys},
    rate_limit::RateLimit,
    repositories::{DeviceRepository, ItemRepository, OtpRepository, RbacRepository, TokenBlacklist, UserRepository},
    services::{
        auth::{AuthService, OtpSettings},
        device::DeviceService,
        rbac::RbacService,
        storage::FileStore,
        user::UserService,
    },
};

/// Persistence and storage backends; every field is a trait object so tests can mock it.
#[derive(Clone)]
pub struct Deps {
    pub users: Arc<dyn UserRepository>,
    pub blacklist: Arc<dyn TokenBlacklist>,
    pub otp: Arc<dyn OtpRepository>,
    pub devices: Arc<dyn DeviceRepository>,
    pub rbac: Arc<dyn RbacRepository>,
    pub items: Arc<dyn ItemRepository>,
    pub files: Arc<dyn FileStore>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub otp: OtpSettings,
    pub staff_email_domains: Vec<String>,
    pub max_upload_bytes: usize,
    pub upload_dir: String,
    pub admin_email: String,
    pub frontend_base_url: String,
}

#[derive(Clone)]
pub struct AppState {
    pub deps: Deps,
    pub users: Arc<dyn UserRepository>,
    pub files: Arc<dyn FileStore>,
    pub auth_service: AuthService,
    pub user_service: UserService,
    pub device_service: DeviceService,
    pub rbac_service: RbacService,
    pub max_upload_bytes: usize,
    pub upload_dir: Arc<str>,
    pub auth_limiter: RateLimit,
    pub general_limiter: RateLimit,
}

impl AppState {
    pub fn with_google(mut self, google: Arc<dyn GoogleVerifier>) -> Self {
        self.auth_service = self.auth_service.with_google(google);
        self
    }

    pub fn new(
        deps: Deps,
        settings: Settings,
        jwt: Arc<JwtKeys>,
        auth_limiter: RateLimit,
        general_limiter: RateLimit,
    ) -> Self {
        Self {
            auth_service: AuthService::new(
                deps.users.clone(),
                deps.blacklist.clone(),
                jwt,
                deps.otp.clone(),
                settings.otp,
                settings.staff_email_domains.clone(),
                settings.admin_email.clone(),
            ),
            user_service: UserService::new(deps.users.clone(), deps.files.clone()),
            device_service: DeviceService::new(deps.devices.clone()),
            rbac_service: RbacService::new(deps.users.clone(), deps.rbac.clone(), settings.staff_email_domains),
            files: deps.files.clone(),
            max_upload_bytes: settings.max_upload_bytes,
            upload_dir: settings.upload_dir.into(),
            users: deps.users.clone(),
            deps,
            auth_limiter,
            general_limiter,
        }
    }
}
