//! OpenAPI document and Swagger UI (the equivalent of FastAPI's `/openapi.json` + `/docs`).
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityRequirement, SecurityScheme},
};

use crate::{entities, error, handlers, services};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Axum Performance & Containerization Showcase",
        description = "High-performance Axum REST API with SeaORM and PostgreSQL"
    ),
    paths(
        handlers::health::root,
        handlers::health::health_check,
        handlers::health::ready,
        handlers::auth::login,
        handlers::auth::get_otp,
        handlers::auth::google,
        handlers::auth::refresh_token,
        handlers::auth::logout,
        handlers::users::list,
        handlers::users::get_profile,
        handlers::users::put_profile,
        handlers::users::patch_profile,
        handlers::users::delete_profile,
        handlers::users::put_profile_picture,
        handlers::devices::register,
        handlers::devices::voip_token,
        handlers::devices::unregister,
        handlers::rbac::assign,
        handlers::rbac::remove,
        handlers::rbac::set_status,
        handlers::items::create_item,
        handlers::items::list_items,
        handlers::items::count_items,
        handlers::items::get_item,
        handlers::items::update_item,
        handlers::items::delete_item,
    ),
    components(
        schemas(
            handlers::items::CreateItemRequest,
            handlers::items::UpdateItemRequest,
            handlers::items::ItemCountResponse,
            entities::items::Model,
            services::auth::LoginPayload,
            services::auth::TokenResponse,
            services::auth::GoogleLoginRequest,
            services::auth::LogoutRequest,
            services::auth::MobileRequest,
            services::auth::OtpIssued,
            services::auth::VerifyAccountRequest,
            services::user::ProfileResponse,
            services::user::ProfileUpdate,
            services::user::ProfilePatch,
            services::user::MessageResponse,
            services::user::ImageUploaded,
            services::device::RegisterDeviceRequest,
            services::device::VoipTokenRequest,
            services::device::DeviceResult,
            services::rbac::RoleChange,
            services::rbac::RoleResponse,
            services::rbac::StatusChange,
            error::ErrorBody,
        )
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).bearer_format("JWT").build()),
        );
        openapi.security = Some(vec![SecurityRequirement::new("bearer_auth", [""])]);
    }
}
