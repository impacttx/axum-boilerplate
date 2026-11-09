use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::Role,
    entities::users,
    error::{AppError, AppResult},
    repositories::UserRepository,
    services::storage::FileStore,
};

#[derive(Debug, Serialize, PartialEq, utoipa::ToSchema)]
pub struct MessageResponse {
    pub message: String,
    pub status: Option<String>,
}

impl MessageResponse {
    pub fn ok(message: impl Into<String>) -> Self {
        Self { message: message.into(), status: Some("success".into()) }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TermsRequest {
    pub revision: String,
    pub accepted: bool,
}

/// The user's own editable profile.
#[derive(Debug, Serialize, PartialEq, utoipa::ToSchema)]
pub struct ProfileResponse {
    pub id: Uuid,
    pub mobile_number: Option<String>,
    pub profile_picture: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub bio: Option<String>,
}

impl From<users::Model> for ProfileResponse {
    fn from(u: users::Model) -> Self {
        Self {
            id: u.id,
            mobile_number: u.mobile_number,
            profile_picture: u.profile_picture,
            first_name: u.first_name,
            last_name: u.last_name,
            email: u.email,
            date_of_birth: u.date_of_birth,
            bio: u.bio,
        }
    }
}

/// Full replacement of the profile (`PUT`).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ProfileUpdate {
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    #[serde(deserialize_with = "crate::date_serde::deserialize_date")]
    pub date_of_birth: NaiveDate,
    pub bio: Option<String>,
}

/// Partial update (`PATCH`); omitted fields are left untouched.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct ProfilePatch {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    #[serde(default, deserialize_with = "crate::date_serde::deserialize_opt_date")]
    pub date_of_birth: Option<NaiveDate>,
    pub bio: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, utoipa::ToSchema)]
pub struct ImageUploaded {
    pub message: String,
    pub image_url: String,
}

fn valid_name(label: &str, v: &str) -> AppResult<String> {
    let v = v.trim();
    if (1..=50).contains(&v.chars().count()) {
        Ok(v.to_owned())
    } else {
        Err(AppError::Unprocessable(format!("{label} must be 1-50 characters")))
    }
}

fn valid_email(v: &str) -> AppResult<String> {
    let v = v.trim().to_lowercase();
    match v.split_once('@') {
        Some((l, d))
            if !l.is_empty()
                && d.contains('.')
                && !d.starts_with('.')
                && !d.ends_with('.')
                && !v.contains(char::is_whitespace) =>
        {
            Ok(v)
        }
        _ => Err(AppError::Unprocessable("email is not valid".into())),
    }
}

fn valid_bio(v: &str) -> AppResult<String> {
    if v.chars().count() <= 500 {
        Ok(v.to_owned())
    } else {
        Err(AppError::Unprocessable("bio must be at most 500 characters".into()))
    }
}

/// Returns the file extension for an accepted image type whose magic bytes match.
fn image_ext(content_type: &str, data: &[u8]) -> AppResult<&'static str> {
    let (ext, ok) = match content_type {
        "image/jpg" | "image/jpeg" => ("jpg", data.starts_with(&[0xFF, 0xD8, 0xFF])),
        "image/png" => ("png", data.starts_with(&[0x89, b'P', b'N', b'G'])),
        "image/webp" => ("webp", data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP"),
        _ => {
            return Err(AppError::Unprocessable("Only JPG, PNG and WebP images are allowed".into()));
        }
    };
    if ok {
        Ok(ext)
    } else {
        Err(AppError::Unprocessable("File content does not match its declared image type".into()))
    }
}

/// Public representation of a user; never exposes the password hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct UserDetailsResponse {
    pub id: Uuid,
    pub email: Option<String>,
    pub mobile_number: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub is_active: bool,
    pub is_admin: bool,
    pub roles: Vec<Role>,
    pub created_at: DateTime<Utc>,
}

impl UserDetailsResponse {
    pub fn new(m: users::Model, roles: Vec<Role>) -> Self {
        Self {
            id: m.id,
            email: m.email,
            mobile_number: m.mobile_number,
            first_name: m.first_name,
            last_name: m.last_name,
            is_active: m.is_active,
            is_admin: m.is_admin,
            roles,
            created_at: m.created_at.to_utc(),
        }
    }
}

pub type UserResponse = UserDetailsResponse;

/// Lightweight user model for list responses with basic identity and contact details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct UserListItemResponse {
    pub id: Uuid,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub mobile_number: Option<String>,
}

pub type UserListitemResponse = UserListItemResponse;

impl From<users::Model> for UserListItemResponse {
    fn from(m: users::Model) -> Self {
        Self {
            id: m.id,
            first_name: m.first_name,
            last_name: m.last_name,
            email: m.email,
            mobile_number: m.mobile_number,
        }
    }
}

impl From<UserDetailsResponse> for UserListItemResponse {
    fn from(u: UserDetailsResponse) -> Self {
        Self {
            id: u.id,
            first_name: u.first_name,
            last_name: u.last_name,
            email: u.email,
            mobile_number: u.mobile_number,
        }
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u64,
    pub per_page: u64,
    pub total: u64,
}

#[derive(Clone)]
pub struct UserService {
    users: Arc<dyn UserRepository>,
    files: Arc<dyn FileStore>,
}

impl UserService {
    pub fn new(users: Arc<dyn UserRepository>, files: Arc<dyn FileStore>) -> Self {
        Self { users, files }
    }

    /// Profile endpoints are locked until the user has accepted terms and conditions.
    fn require_terms(user: &users::Model) -> AppResult<()> {
        if user.accepted_terms.as_object().is_none_or(|o| o.is_empty()) {
            return Err(AppError::Denied("You must accept the terms and conditions to proceed.".into()));
        }
        Ok(())
    }

    pub async fn accept_terms(&self, mut user: users::Model, req: TermsRequest) -> AppResult<MessageResponse> {
        if self.users.latest_terms_revision().await?.is_none() {
            return Err(AppError::NotFound("Terms and conditions"));
        }
        if !req.accepted || req.revision.trim().is_empty() {
            return Err(AppError::Denied("You must accept the terms and conditions to proceed.".into()));
        }
        let now = Utc::now().fixed_offset();
        let mut terms = user.accepted_terms.as_object().cloned().unwrap_or_default();
        terms.insert(req.revision, serde_json::Value::String(now.to_rfc3339()));
        user.accepted_terms = serde_json::Value::Object(terms);
        user.updated_at = Some(now);
        self.users.save(user).await?;
        Ok(MessageResponse::ok("Terms and conditions accepted"))
    }

    pub fn profile(&self, user: users::Model) -> AppResult<ProfileResponse> {
        Ok(user.into())
    }

    pub async fn replace_profile(&self, mut user: users::Model, req: ProfileUpdate) -> AppResult<ProfileResponse> {
        user.first_name = Some(valid_name("first_name", &req.first_name)?);
        user.last_name = Some(valid_name("last_name", &req.last_name)?);
        if let Some(e) = &req.email {
            user.email = Some(valid_email(e)?);
        }
        user.date_of_birth = Some(req.date_of_birth);
        if let Some(b) = &req.bio {
            user.bio = Some(valid_bio(b)?);
        }
        user.updated_at = Some(Utc::now().fixed_offset());
        Ok(self.users.save(user).await?.into())
    }

    pub async fn patch_profile(&self, mut user: users::Model, req: ProfilePatch) -> AppResult<ProfileResponse> {
        let mut changed = false;
        if let Some(v) = &req.first_name {
            user.first_name = Some(valid_name("first_name", v)?);
            changed = true;
        }
        if let Some(v) = &req.last_name {
            user.last_name = Some(valid_name("last_name", v)?);
            changed = true;
        }
        if let Some(v) = &req.email {
            user.email = Some(valid_email(v)?);
            changed = true;
        }
        if let Some(v) = req.date_of_birth {
            user.date_of_birth = Some(v);
            changed = true;
        }
        if let Some(v) = &req.bio {
            user.bio = Some(valid_bio(v)?);
            changed = true;
        }
        if !changed {
            return Ok(user.into());
        }
        user.updated_at = Some(Utc::now().fixed_offset());
        Ok(self.users.save(user).await?.into())
    }

    pub async fn upload_picture(
        &self,
        user: users::Model,
        content_type: &str,
        data: Vec<u8>,
    ) -> AppResult<ImageUploaded> {
        let ext = image_ext(content_type, &data)?;
        let url = self.files.put(&format!("{}.{ext}", user.id), content_type, data).await?;
        self.users.set_profile_picture(user.id, &url).await?;
        Ok(ImageUploaded { message: "Profile picture updated successfully".into(), image_url: url })
    }

    pub async fn delete_account(&self, user: &users::Model) -> AppResult<MessageResponse> {
        self.users.delete(user.id).await?;
        Ok(MessageResponse::ok("Profile deleted successfully"))
    }

    pub async fn request_account_report(&self, user: &users::Model) -> AppResult<MessageResponse> {
        Self::require_terms(user)?;
        self.users.create_account_info_request(user.id).await?;
        Ok(MessageResponse {
            message: "Account info report request received. You will be notified once the report is sent.".into(),
            status: Some("info".into()),
        })
    }

    pub async fn list(&self, q: ListQuery) -> AppResult<Page<UserDetailsResponse>> {
        let page = q.page.unwrap_or(1).max(1);
        let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
        let (items, total) = self.users.list(page, per_page).await?;
        let items = items.into_iter().map(|u| UserDetailsResponse::new(u, vec![])).collect();
        Ok(Page { items, page, per_page, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{repositories::user::MockUserRepository, services::storage::MockFileStore, test_support::user_model};

    fn svc(users: MockUserRepository, files: MockFileStore) -> UserService {
        UserService::new(Arc::new(users), Arc::new(files))
    }

    fn accepted(mut u: users::Model) -> users::Model {
        u.accepted_terms = serde_json::json!({"v1.0": "2026-01-01T00:00:00Z"});
        u
    }

    #[tokio::test]
    async fn list_clamps_pagination() {
        let mut repo = MockUserRepository::new();
        repo.expect_list()
            .withf(|p, pp| *p == 1 && *pp == 100)
            .returning(|_, _| Ok((vec![user_model("a@b.com", false)], 1)));
        let page =
            svc(repo, MockFileStore::new()).list(ListQuery { page: Some(0), per_page: Some(10_000) }).await.unwrap();
        assert_eq!((page.page, page.per_page, page.total, page.items.len()), (1, 100, 1, 1));
    }

    #[tokio::test]
    async fn profile_does_not_require_accepted_terms() {
        let s = svc(MockUserRepository::new(), MockFileStore::new());
        let u = user_model("a@b.com", false);
        assert!(s.profile(u.clone()).is_ok());
        assert!(s.patch_profile(u, ProfilePatch::default()).await.is_ok());
    }

    #[tokio::test]
    async fn accept_terms_records_revision() {
        let mut repo = MockUserRepository::new();
        repo.expect_latest_terms_revision().returning(|| Ok(Some("v1.0".into())));
        repo.expect_save().withf(|u| u.accepted_terms.get("v1.0").is_some()).times(1).returning(Ok);
        let s = svc(repo, MockFileStore::new());
        let r = s
            .accept_terms(user_model("a@b.com", false), TermsRequest { revision: "v1.0".into(), accepted: true })
            .await
            .unwrap();
        assert_eq!(r.status.as_deref(), Some("success"));
        let denied = s
            .accept_terms(user_model("a@b.com", false), TermsRequest { revision: "v1.0".into(), accepted: false })
            .await;
        assert!(matches!(denied, Err(AppError::Denied(_))));
    }

    #[tokio::test]
    async fn patch_only_touches_provided_fields_and_validates() {
        let mut repo = MockUserRepository::new();
        repo.expect_save()
            .withf(|u| u.bio.as_deref() == Some("hi") && u.first_name.as_deref() == Some("Test"))
            .times(1)
            .returning(Ok);
        let s = svc(repo, MockFileStore::new());
        let u = accepted(user_model("a@b.com", false));
        let p = ProfilePatch { bio: Some("hi".into()), ..Default::default() };
        assert_eq!(s.patch_profile(u.clone(), p).await.unwrap().bio.as_deref(), Some("hi"));
        let bad = ProfilePatch { email: Some("nope".into()), ..Default::default() };
        assert!(matches!(s.patch_profile(u, bad).await, Err(AppError::Unprocessable(_))));
    }

    #[tokio::test]
    async fn picture_upload_checks_type_and_magic_bytes() {
        let mut files = MockFileStore::new();
        files
            .expect_put()
            .withf(|n, ct, _| n.ends_with(".png") && ct == "image/png")
            .times(1)
            .returning(|n, _, _| Ok(format!("/uploads/{n}")));
        let mut repo = MockUserRepository::new();
        repo.expect_set_profile_picture().times(1).returning(|_, _| Ok(()));
        let s = svc(repo, files);
        let u = accepted(user_model("a@b.com", false));
        let png = vec![0x89, b'P', b'N', b'G', 0, 0];
        assert!(s.upload_picture(u.clone(), "image/png", png.clone()).await.is_ok());
        assert!(s.upload_picture(u.clone(), "image/gif", png.clone()).await.is_err());
        assert!(s.upload_picture(u, "image/jpeg", png).await.is_err());
    }
}
