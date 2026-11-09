//! Shared fixtures for unit tests.
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::{
    auth::jwt::{JwtKeys, tests::test_config},
    entities::users,
    rate_limit::RateLimit,
    repositories::{
        blacklist::MockTokenBlacklist, device::MockDeviceRepository, item::MockItemRepository, otp::MockOtpRepository,
        rbac::MockRbacRepository, user::MockUserRepository,
    },
    services::{auth::OtpSettings, storage::MockFileStore},
    state::{AppState, Deps, Settings},
};

pub fn jwt() -> Arc<JwtKeys> {
    Arc::new(JwtKeys::new(&test_config()).unwrap())
}

pub fn user_model(email: &str, is_admin: bool) -> users::Model {
    let now = Utc::now().fixed_offset();
    users::Model {
        id: Uuid::new_v4(),
        email: Some(email.into()),
        mobile_number: None,
        profile_picture: None,
        first_name: Some("Test".into()),
        last_name: Some("User".into()),
        bio: None,
        hashed_password: None,
        is_active: true,
        is_admin,
        last_login: None,
        date_of_birth: None,
        accepted_terms: serde_json::json!({}),
        created_at: now,
        updated_at: None,
        last_activity: None,
    }
}

/// Mock dependencies for building a test `AppState`; set expectations, then `build()`.
#[derive(Default)]
pub struct Mocks {
    pub users: MockUserRepository,
    pub blacklist: MockTokenBlacklist,
    pub otp: MockOtpRepository,
    pub devices: MockDeviceRepository,
    pub rbac: MockRbacRepository,
    pub items: MockItemRepository,
    pub files: MockFileStore,
}

impl Mocks {
    pub fn build(self) -> AppState {
        AppState::new(
            Deps {
                users: Arc::new(self.users),
                blacklist: Arc::new(self.blacklist),
                otp: Arc::new(self.otp),
                devices: Arc::new(self.devices),
                rbac: Arc::new(self.rbac),
                items: Arc::new(self.items),
                files: Arc::new(self.files),
            },
            Settings {
                otp: OtpSettings { expiry_minutes: 5, expose_otp: true },
                staff_email_domains: vec!["example.com".into()],
                max_upload_bytes: 1024 * 1024,
                upload_dir: "uploads-test".into(),
                admin_email: "admin@example.com".into(),
                frontend_base_url: "http://localhost:8082".into(),
            },
            jwt(),
            RateLimit::per_minute(1000, false),
            RateLimit::per_minute(1000, false),
        )
    }
}

pub fn state(users: MockUserRepository, blacklist: MockTokenBlacklist) -> AppState {
    Mocks { users, blacklist, ..Default::default() }.build()
}
