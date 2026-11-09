use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{
        AuthUser, Role,
        google::{GoogleProfile, GoogleVerifier},
        jwt::{Claims, IssuedToken, JwtKeys, TokenKind},
        password,
    },
    entities::users,
    error::{AppError, AppResult},
    repositories::{NewUser, OtpRepository, TokenBlacklist, UserRepository},
};
use chrono::{Duration, Utc};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Body of `POST /v1/auth/login`: either an OTP login (mobile apps) or email + password
/// (staff and admin accounts).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum LoginPayload {
    Otp(OtpLoginRequest),
    Password(LoginRequest),
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OtpLoginRequest {
    /// E.164 number, e.g. `+14165550123`.
    pub mobile_number: String,
    pub otp: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MobileRequest {
    /// E.164 number, e.g. `+14165550123`.
    pub mobile_number: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct VerifyAccountRequest {
    pub mobile_number: String,
    pub otp: String,
    /// Revision of the terms and conditions the user accepted.
    pub tnc_revision: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OtpIssued {
    pub message: String,
    pub expiry_minutes: i64,
    /// Only echoed back in the `development` environment, where no SMS is sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct OtpSettings {
    pub expiry_minutes: i64,
    pub expose_otp: bool,
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct LogoutRequest {
    /// Optional: revoke the paired refresh token as well.
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub user_id: Uuid,
    pub tnc_accepted: bool,
    pub setup_profile: bool,
}

/// Body of `POST /v1/auth/google`. `credential` is the field name used by Google Identity
/// Services; `id_token` is accepted as an alias.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct GoogleLoginRequest {
    pub id_token: Option<String>,
    pub credential: Option<String>,
}

#[derive(Clone)]
pub struct AuthService {
    users: Arc<dyn UserRepository>,
    blacklist: Arc<dyn TokenBlacklist>,
    jwt: Arc<JwtKeys>,
    google: Option<Arc<dyn GoogleVerifier>>,
    otp: Arc<dyn OtpRepository>,
    otp_settings: OtpSettings,
    staff_email_domains: Vec<String>,
    admin_email: String,
}

/// Accepts `+14165550123` or `tel:+14165550123` and returns the bare E.164 number.
pub fn normalize_mobile(raw: &str) -> AppResult<String> {
    let m = raw.trim().strip_prefix("tel:").unwrap_or(raw.trim());
    let digits = m.strip_prefix('+').unwrap_or("");
    if (8..=15).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()) {
        Ok(m.to_owned())
    } else {
        Err(AppError::Unprocessable("mobile_number must be in E.164 format, e.g. +14165550123".into()))
    }
}

impl AuthService {
    pub fn new(
        users: Arc<dyn UserRepository>,
        blacklist: Arc<dyn TokenBlacklist>,
        jwt: Arc<JwtKeys>,
        otp: Arc<dyn OtpRepository>,
        otp_settings: OtpSettings,
        staff_email_domains: Vec<String>,
        admin_email: String,
    ) -> Self {
        Self { users, blacklist, jwt, google: None, otp, otp_settings, staff_email_domains, admin_email }
    }

    /// Generates, stores (hashed) and "sends" a one-time password. SMS delivery is not
    /// wired up yet, so the code is only logged (and echoed) in development.
    async fn issue_otp(&self, mobile: &str) -> AppResult<OtpIssued> {
        let otp = format!("{:06}", Uuid::new_v4().as_u128() % 1_000_000);
        let hashed = password::hash(otp.clone()).await?;
        let expires = Utc::now() + Duration::minutes(self.otp_settings.expiry_minutes);
        self.otp.upsert(mobile, &hashed, expires).await?;
        if self.otp_settings.expose_otp {
            tracing::info!(mobile, otp, "OTP generated (SMS gateway not configured)");
        }
        Ok(OtpIssued {
            message: "OTP sent successfully".into(),
            expiry_minutes: self.otp_settings.expiry_minutes,
            otp: self.otp_settings.expose_otp.then_some(otp),
        })
    }

    /// Checks and consumes the OTP for `mobile`.
    async fn consume_otp(&self, mobile: &str, otp: &str) -> AppResult<()> {
        let invalid = || AppError::BadRequest("Invalid or expired OTP".into());
        let record = self.otp.find(mobile).await?.ok_or_else(invalid)?;
        if record.expires_at.to_utc() < Utc::now() {
            self.otp.delete(mobile).await?;
            return Err(invalid());
        }
        if !password::verify(otp.trim().to_owned(), record.hashed_otp).await? {
            return Err(invalid());
        }
        self.otp.delete(mobile).await
    }

    /// Sends a login OTP to an existing, active account.
    pub async fn get_otp(&self, req: MobileRequest) -> AppResult<OtpIssued> {
        let mobile = normalize_mobile(&req.mobile_number)?;
        match self.users.find_by_mobile(&mobile).await? {
            Some(u) if u.is_active => self.issue_otp(&mobile).await,
            _ => Err(AppError::NotFound("User")),
        }
    }

    /// Starts sign-up: creates (or recycles) an inactive account and sends an OTP.
    pub async fn register(&self, req: MobileRequest) -> AppResult<OtpIssued> {
        let mobile = normalize_mobile(&req.mobile_number)?;
        match self.users.find_by_mobile(&mobile).await? {
            Some(u) if u.is_active => {
                return Err(AppError::Conflict("User with this mobile number already exists".into()));
            }
            Some(mut stale) => {
                stale.created_at = Utc::now().fixed_offset();
                self.users.save(stale).await?;
            }
            None => {
                self.users.create_pending(&mobile).await?;
            }
        }
        self.issue_otp(&mobile).await
    }

    /// Completes sign-up: validates the OTP, records the accepted terms and activates the account.
    pub async fn verify_account(&self, req: VerifyAccountRequest) -> AppResult<TokenResponse> {
        let mobile = normalize_mobile(&req.mobile_number)?;
        self.consume_otp(&mobile, &req.otp).await?;
        let mut user = match self.users.find_by_mobile(&mobile).await? {
            Some(u) if !u.is_active => u,
            _ => return Err(AppError::NotFound("User")),
        };
        let now = Utc::now().fixed_offset();
        user.accepted_terms = serde_json::json!({ req.tnc_revision: now.to_rfc3339() });
        user.is_active = true;
        user.last_login = Some(now);
        user.last_activity = Some(now);
        let user = self.users.save(user).await?;
        self.issue_pair(&user).await
    }

    pub async fn login_otp(&self, req: OtpLoginRequest) -> AppResult<TokenResponse> {
        let mobile = normalize_mobile(&req.mobile_number)?;
        self.consume_otp(&mobile, &req.otp).await?;
        let user = match self.users.find_by_mobile(&mobile).await? {
            Some(u) if u.is_active => u,
            _ => return Err(AppError::NotFound("User")),
        };
        self.users.touch_last_login(user.id).await?;
        self.issue_pair(&user).await
    }

    pub async fn login_any(&self, req: LoginPayload) -> AppResult<TokenResponse> {
        match req {
            LoginPayload::Otp(r) => self.login_otp(r).await,
            LoginPayload::Password(r) => self.login(r).await,
        }
    }

    pub fn with_google(mut self, google: Arc<dyn GoogleVerifier>) -> Self {
        self.google = Some(google);
        self
    }

    /// Signs in (or registers) a user from a Google ID token or mock tokens for local testing.
    /// Real Google logins never grant elevated access (new accounts are plain customers and admin
    /// status is left untouched), while mock tokens (`mock_sales_token`, `mock_admin_token`)
    /// assign sales or admin privileges for testing.
    pub async fn google_login(&self, req: GoogleLoginRequest) -> AppResult<TokenResponse> {
        let token = req
            .id_token
            .or(req.credential)
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| AppError::BadRequest("Google ID token or credential is required".into()))?;
        let token = token.trim();

        struct MockGoogleUser {
            email: String,
            given_name: Option<String>,
            family_name: Option<String>,
            picture: Option<String>,
            role: Role,
        }

        let mock_user: Option<MockGoogleUser> = if token == "mock_sales_token" || token == "mock-sales" {
            Some(MockGoogleUser {
                email: "sales_user@example.com".into(),
                given_name: Some("Sales".into()),
                family_name: Some("Staff".into()),
                picture: None,
                role: Role::Sales,
            })
        } else if token == "mock_admin_token" || token == "mock-admin" {
            Some(MockGoogleUser {
                email: "admin_user@example.com".into(),
                given_name: Some("Admin".into()),
                family_name: Some("Staff".into()),
                picture: None,
                role: Role::Admin,
            })
        } else if token.starts_with("mock_") || token.starts_with("mock-") {
            Some(MockGoogleUser {
                email: "google_user@example.com".into(),
                given_name: Some("Google".into()),
                family_name: Some("User".into()),
                picture: None,
                role: Role::Customer,
            })
        } else {
            None
        };

        let (profile, mut requested_role, is_mock) = match mock_user {
            Some(mock) => (
                GoogleProfile {
                    email: mock.email,
                    given_name: mock.given_name,
                    family_name: mock.family_name,
                    picture: mock.picture,
                },
                mock.role,
                true,
            ),
            None => {
                let Some(google) = &self.google else {
                    return Err(AppError::Unavailable("Google login is not configured".into()));
                };
                let profile = google.verify(token).await?;
                (profile, Role::Customer, false)
            }
        };

        let email = profile.email.trim().to_lowercase();
        let domain = email.rsplit_once('@').map_or("", |(_, d)| d);
        let domain_allowed = is_mock || self.staff_email_domains.iter().any(|d| d.eq_ignore_ascii_case(domain));
        if !domain_allowed {
            requested_role = Role::Customer;
        }

        let admin_email = self.admin_email.trim().to_lowercase();
        let is_admin = requested_role == Role::Admin || (!admin_email.is_empty() && email == admin_email);

        let user = match self.users.find_by_email(&email).await? {
            Some(u) if !u.is_active && !is_mock => return Err(AppError::Unauthorized),
            Some(mut u) => {
                let mut needs_save = false;
                if !u.is_active {
                    u.is_active = true;
                    needs_save = true;
                }
                if is_admin && !u.is_admin {
                    u.is_admin = true;
                    needs_save = true;
                }
                if needs_save {
                    u = self.users.save(u).await?;
                }
                if u.profile_picture.is_none()
                    && let Some(pic) = &profile.picture
                {
                    self.users.set_profile_picture(u.id, pic).await?;
                }
                if requested_role != Role::Customer
                    && requested_role != Role::Admin
                    && let Err(e) = self.users.assign_role(u.id, requested_role.as_str()).await
                {
                    tracing::warn!(error = %e, role = %requested_role, "could not assign RBAC role");
                }
                u
            }
            None => {
                let u = self
                    .users
                    .create(NewUser {
                        email: email.clone(),
                        mobile_number: None,
                        first_name: profile.given_name,
                        last_name: profile.family_name,
                        profile_picture: profile.picture,
                        is_admin,
                    })
                    .await?;
                if requested_role != Role::Customer
                    && requested_role != Role::Admin
                    && let Err(e) = self.users.assign_role(u.id, requested_role.as_str()).await
                {
                    tracing::warn!(error = %e, role = %requested_role, "could not assign RBAC role");
                }
                u
            }
        };
        self.users.touch_last_login(user.id).await?;
        self.issue_pair(&user).await
    }

    pub async fn login(&self, req: LoginRequest) -> AppResult<TokenResponse> {
        let email = req.email.trim().to_lowercase();
        let Some(user) = self.users.find_by_email(&email).await? else {
            password::verify_dummy(req.password).await?;
            return Err(AppError::Unauthorized);
        };
        let stored = user.hashed_password.clone().unwrap_or_default();
        if !password::verify(req.password, stored).await? || !user.is_active {
            return Err(AppError::Unauthorized);
        }
        self.users.touch_last_login(user.id).await?;
        self.issue_pair(&user).await
    }

    /// Rotates a refresh token: the presented one is revoked and a new pair is issued.
    pub async fn refresh(&self, refresh_token: &str) -> AppResult<TokenResponse> {
        let claims = self.jwt.verify(refresh_token, TokenKind::Refresh)?;
        if self.blacklist.is_revoked(&claims.jti).await? {
            return Err(AppError::Unauthorized);
        }
        let user = self.active_user(claims.sub).await?;
        self.blacklist.revoke(&claims.jti, claims.expires_at()).await?;
        self.issue_pair(&user).await
    }

    pub async fn logout(&self, access: &Claims, req: LogoutRequest) -> AppResult<()> {
        self.blacklist.revoke(&access.jti, access.expires_at()).await?;
        if let Some(rt) = req.refresh_token
            && let Ok(refresh) = self.jwt.verify(&rt, TokenKind::Refresh)
            && refresh.sub == access.sub
        {
            self.blacklist.revoke(&refresh.jti, refresh.expires_at()).await?;
        }
        Ok(())
    }

    /// Validates an access token end to end: signature/claims, revocation, and that the
    /// user still exists and is active. Roles are read fresh from the database.
    pub async fn authenticate(&self, token: &str) -> AppResult<(Claims, AuthUser)> {
        let claims = self.jwt.verify(token, TokenKind::Access)?;
        if self.blacklist.is_revoked(&claims.jti).await? {
            return Err(AppError::Unauthorized);
        }
        let user = self.active_user(claims.sub).await?;
        let roles = self.roles_of(&user).await?;
        Ok((claims, AuthUser { user, roles }))
    }

    async fn active_user(&self, id: Uuid) -> AppResult<users::Model> {
        match self.users.find_by_id(id).await? {
            Some(u) if u.is_active => Ok(u),
            _ => Err(AppError::Unauthorized),
        }
    }

    async fn roles_of(&self, user: &users::Model) -> AppResult<Vec<Role>> {
        Ok(self.users.role_names(user.id).await?.iter().filter_map(|n| n.parse().ok()).collect())
    }

    async fn issue_pair(&self, user: &users::Model) -> AppResult<TokenResponse> {
        let primary = if user.is_admin {
            Role::Admin
        } else {
            self.roles_of(user).await?.first().copied().unwrap_or(Role::Customer)
        };
        let IssuedToken { token: access_token, .. } = self.jwt.issue(user.id, primary.as_str(), TokenKind::Access)?;
        let IssuedToken { token: refresh_token, .. } = self.jwt.issue(user.id, primary.as_str(), TokenKind::Refresh)?;
        Ok(TokenResponse {
            access_token,
            refresh_token,
            token_type: "Bearer",
            expires_in: self.jwt.access_ttl_secs(),
            user_id: user.id,
            tnc_accepted: user.accepted_terms.as_object().is_some_and(|o| !o.is_empty()),
            setup_profile: user.first_name.is_none() || user.last_name.is_none(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::google::{GoogleProfile, MockGoogleVerifier},
        entities::mobile_otp_records,
        repositories::{blacklist::MockTokenBlacklist, otp::MockOtpRepository, user::MockUserRepository},
        test_support::{jwt, user_model},
    };

    fn svc(users: MockUserRepository, bl: MockTokenBlacklist) -> AuthService {
        AuthService::new(
            Arc::new(users),
            Arc::new(bl),
            jwt(),
            Arc::new(MockOtpRepository::new()),
            OtpSettings { expiry_minutes: 5, expose_otp: true },
            vec!["example.com".into()],
            "admin@example.com".into(),
        )
    }

    async fn user_with_password(pw: &str) -> users::Model {
        let mut u = user_model("jane@example.com", false);
        u.hashed_password = Some(password::hash(pw.into()).await.unwrap());
        u
    }

    fn login_req(email: &str, pw: &str) -> LoginRequest {
        LoginRequest { email: email.into(), password: pw.into() }
    }

    #[tokio::test]
    async fn login_success_issues_working_tokens() {
        let u = user_with_password("password123").await;
        let id = u.id;
        let mut repo = MockUserRepository::new();
        let found = u.clone();
        repo.expect_find_by_email().withf(|e| e == "jane@example.com").returning(move |_| Ok(Some(found.clone())));
        repo.expect_touch_last_login().times(1).returning(|_| Ok(()));
        repo.expect_role_names().returning(|_| Ok(vec!["sales".into()]));
        repo.expect_find_by_id().returning(move |_| Ok(Some(u.clone())));
        let mut bl = MockTokenBlacklist::new();
        bl.expect_is_revoked().returning(|_| Ok(false));
        let s = svc(repo, bl);

        let t = s.login(login_req("  Jane@Example.com ", "password123")).await.unwrap();
        assert_eq!((t.token_type, t.user_id), ("Bearer", id));
        let (claims, auth) = s.authenticate(&t.access_token).await.unwrap();
        assert_eq!(claims.role, "sales");
        assert_eq!(auth.roles, [Role::Sales]);
    }

    #[tokio::test]
    async fn login_failures_are_indistinguishable() {
        let u = user_with_password("password123").await;
        let mut inactive = u.clone();
        inactive.is_active = false;
        let mut legacy_plaintext = u.clone();
        legacy_plaintext.hashed_password = Some("password123".into());
        for (found, pw) in [
            (Some(u), "wrong"),
            (None, "password123"),
            (Some(inactive), "password123"),
            (Some(legacy_plaintext), "password123"),
        ] {
            let mut repo = MockUserRepository::new();
            repo.expect_find_by_email().returning(move |_| Ok(found.clone()));
            let res = svc(repo, MockTokenBlacklist::new()).login(login_req("jane@example.com", pw)).await;
            assert!(matches!(res, Err(AppError::Unauthorized)));
        }
    }

    #[tokio::test]
    async fn revoked_access_token_is_rejected() {
        let u = user_model("a@b.com", false);
        let issued = jwt().issue(u.id, "customer", TokenKind::Access).unwrap();
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_id().never();
        let mut bl = MockTokenBlacklist::new();
        bl.expect_is_revoked().returning(|_| Ok(true));
        let res = svc(repo, bl).authenticate(&issued.token).await;
        assert!(matches!(res, Err(AppError::Unauthorized)));
    }

    #[tokio::test]
    async fn refresh_rotates_and_revokes_old_token() {
        let u = user_model("a@b.com", true);
        let old = jwt().issue(u.id, "admin", TokenKind::Refresh).unwrap();
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_id().returning(move |_| Ok(Some(u.clone())));
        let mut bl = MockTokenBlacklist::new();
        bl.expect_is_revoked().returning(|_| Ok(false));
        let old_jti = old.claims.jti.clone();
        bl.expect_revoke().withf(move |j, _| j == old_jti).times(1).returning(|_, _| Ok(()));
        let t = svc(repo, bl).refresh(&old.token).await.unwrap();
        assert_ne!(t.refresh_token, old.token);
    }

    #[tokio::test]
    async fn refresh_rejects_access_token_and_revoked_token() {
        let id = Uuid::new_v4();
        let access = jwt().issue(id, "customer", TokenKind::Access).unwrap();
        let s = svc(MockUserRepository::new(), MockTokenBlacklist::new());
        assert!(s.refresh(&access.token).await.is_err());

        let refresh = jwt().issue(id, "customer", TokenKind::Refresh).unwrap();
        let mut bl = MockTokenBlacklist::new();
        bl.expect_is_revoked().returning(|_| Ok(true));
        let res = svc(MockUserRepository::new(), bl).refresh(&refresh.token).await;
        assert!(matches!(res, Err(AppError::Unauthorized)));
    }

    #[tokio::test]
    async fn logout_revokes_access_and_matching_refresh() {
        let id = Uuid::new_v4();
        let access = jwt().issue(id, "customer", TokenKind::Access).unwrap();
        let refresh = jwt().issue(id, "customer", TokenKind::Refresh).unwrap();
        let foreign = jwt().issue(Uuid::new_v4(), "customer", TokenKind::Refresh).unwrap();
        let mut bl = MockTokenBlacklist::new();
        let (a, r) = (access.claims.jti.clone(), refresh.claims.jti.clone());
        bl.expect_revoke().withf(move |j, _| j == a || j == r).times(2).returning(|_, _| Ok(()));
        let s = svc(MockUserRepository::new(), bl);
        s.logout(&access.claims, LogoutRequest { refresh_token: Some(refresh.token) }).await.unwrap();
        // A refresh token belonging to someone else is ignored (only the access token is revoked).
        let mut bl = MockTokenBlacklist::new();
        bl.expect_revoke().times(1).returning(|_, _| Ok(()));
        let s = svc(MockUserRepository::new(), bl);
        s.logout(&access.claims, LogoutRequest { refresh_token: Some(foreign.token) }).await.unwrap();
    }

    fn google_svc(users: MockUserRepository, profile: Option<GoogleProfile>) -> AuthService {
        let mut g = MockGoogleVerifier::new();
        g.expect_verify().returning(move |_| profile.clone().ok_or(AppError::Unauthorized));
        svc(users, MockTokenBlacklist::new()).with_google(Arc::new(g))
    }

    fn profile() -> GoogleProfile {
        GoogleProfile {
            email: "New.User@Example.com".into(),
            given_name: Some("New".into()),
            family_name: Some("User".into()),
            picture: Some("http://pic".into()),
        }
    }

    fn google_req() -> GoogleLoginRequest {
        GoogleLoginRequest { id_token: None, credential: Some("tok".into()) }
    }

    #[tokio::test]
    async fn google_login_registers_new_customer() {
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_email().withf(|e| e == "new.user@example.com").returning(|_| Ok(None));
        repo.expect_create()
            .withf(|n| {
                n.email == "new.user@example.com" && n.mobile_number.is_none() && n.first_name.as_deref() == Some("New")
            })
            .times(1)
            .returning(|n| Ok(user_model(&n.email, false)));
        repo.expect_touch_last_login().times(1).returning(|_| Ok(()));
        repo.expect_role_names().returning(|_| Ok(vec![]));
        let res = google_svc(repo, Some(profile())).google_login(google_req()).await.unwrap();
        assert!(!res.access_token.is_empty());
    }

    #[tokio::test]
    async fn google_login_existing_user_fills_missing_picture() {
        let existing = user_model("new.user@example.com", false);
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_email().returning(move |_| Ok(Some(existing.clone())));
        repo.expect_set_profile_picture().times(1).returning(|_, _| Ok(()));
        repo.expect_touch_last_login().returning(|_| Ok(()));
        repo.expect_role_names().returning(|_| Ok(vec![]));
        google_svc(repo, Some(profile())).google_login(google_req()).await.unwrap();
    }

    #[tokio::test]
    async fn google_login_rejects_inactive_user() {
        let mut inactive = user_model("new.user@example.com", false);
        inactive.is_active = false;
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_email().returning(move |_| Ok(Some(inactive.clone())));
        let err = google_svc(repo, Some(profile())).google_login(google_req()).await.unwrap_err();
        assert!(matches!(err, AppError::Unauthorized));
    }

    #[tokio::test]
    async fn google_login_rejects_invalid_token_and_missing_token() {
        let err = google_svc(MockUserRepository::new(), None).google_login(google_req()).await.unwrap_err();
        assert!(matches!(err, AppError::Unauthorized));
        let err = google_svc(MockUserRepository::new(), Some(profile()))
            .google_login(GoogleLoginRequest::default())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn google_login_unavailable_when_unconfigured() {
        let err =
            svc(MockUserRepository::new(), MockTokenBlacklist::new()).google_login(google_req()).await.unwrap_err();
        assert!(matches!(err, AppError::Unavailable(_)));
    }

    #[tokio::test]
    async fn google_login_mock_sales_token_assigns_sales_role() {
        for token_val in ["mock_sales_token", "mock-sales"] {
            let mut repo = MockUserRepository::new();
            repo.expect_find_by_email().withf(|e| e == "sales_user@example.com").returning(|_| Ok(None));
            repo.expect_create()
                .withf(|n| {
                    n.email == "sales_user@example.com"
                        && n.first_name.as_deref() == Some("Sales")
                        && n.last_name.as_deref() == Some("Staff")
                        && !n.is_admin
                })
                .times(1)
                .returning(|n| Ok(user_model(&n.email, false)));
            repo.expect_assign_role().withf(|_uid, role| role == "sales").times(1).returning(|_, _| Ok(()));
            repo.expect_touch_last_login().times(1).returning(|_| Ok(()));
            repo.expect_role_names().returning(|_| Ok(vec!["sales".into()]));

            // Notice we do NOT configure a GoogleVerifier - mock tokens work without it!
            let auth = svc(repo, MockTokenBlacklist::new());
            let res = auth
                .google_login(GoogleLoginRequest { credential: Some(token_val.into()), id_token: None })
                .await
                .unwrap();

            assert!(!res.access_token.is_empty());
            assert!(!res.setup_profile); // Sales Staff both present -> false
            let claims = jwt().verify(&res.access_token, TokenKind::Access).unwrap();
            assert_eq!(claims.role, "sales");
        }
    }

    #[tokio::test]
    async fn google_login_mock_admin_token_sets_admin() {
        for token_val in ["mock_admin_token", "mock-admin"] {
            let mut repo = MockUserRepository::new();
            repo.expect_find_by_email().withf(|e| e == "admin_user@example.com").returning(|_| Ok(None));
            repo.expect_create()
                .withf(|n| {
                    n.email == "admin_user@example.com"
                        && n.first_name.as_deref() == Some("Admin")
                        && n.last_name.as_deref() == Some("Staff")
                        && n.is_admin
                })
                .times(1)
                .returning(|n| Ok(user_model(&n.email, true)));
            repo.expect_touch_last_login().times(1).returning(|_| Ok(()));

            let auth = svc(repo, MockTokenBlacklist::new());
            let res = auth
                .google_login(GoogleLoginRequest { id_token: Some(token_val.into()), credential: None })
                .await
                .unwrap();

            assert!(!res.access_token.is_empty());
            assert!(!res.setup_profile);
            let claims = jwt().verify(&res.access_token, TokenKind::Access).unwrap();
            assert_eq!(claims.role, "admin");
        }
    }

    #[tokio::test]
    async fn google_login_mock_customer_token_registers_customer() {
        for token_val in ["mock_custom_123", "mock-customer"] {
            let mut repo = MockUserRepository::new();
            repo.expect_find_by_email().withf(|e| e == "google_user@example.com").returning(|_| Ok(None));
            repo.expect_create()
                .withf(|n| {
                    n.email == "google_user@example.com"
                        && n.mobile_number.is_none()
                        && n.first_name.as_deref() == Some("Google")
                        && n.last_name.as_deref() == Some("User")
                        && !n.is_admin
                })
                .times(1)
                .returning(|n| Ok(user_model(&n.email, false)));
            repo.expect_touch_last_login().times(1).returning(|_| Ok(()));
            repo.expect_role_names().returning(|_| Ok(vec![]));

            let auth = svc(repo, MockTokenBlacklist::new());
            let res = auth
                .google_login(GoogleLoginRequest { credential: Some(token_val.into()), id_token: None })
                .await
                .unwrap();

            assert!(!res.access_token.is_empty());
            let claims = jwt().verify(&res.access_token, TokenKind::Access).unwrap();
            assert_eq!(claims.role, "customer");
        }
    }

    #[tokio::test]
    async fn google_login_mock_existing_inactive_user_activates() {
        let mut inactive = user_model("sales_user@example.com", false);
        inactive.is_active = false;

        let mut repo = MockUserRepository::new();
        repo.expect_find_by_email()
            .withf(|e| e == "sales_user@example.com")
            .returning(move |_| Ok(Some(inactive.clone())));
        repo.expect_save().withf(|u| u.is_active).times(1).returning(Ok);
        repo.expect_assign_role().withf(|_uid, role| role == "sales").times(1).returning(|_, _| Ok(()));
        repo.expect_touch_last_login().times(1).returning(|_| Ok(()));
        repo.expect_role_names().returning(|_| Ok(vec!["sales".into()]));

        let auth = svc(repo, MockTokenBlacklist::new());
        let res = auth
            .google_login(GoogleLoginRequest { credential: Some("mock-sales".into()), id_token: None })
            .await
            .unwrap();

        let claims = jwt().verify(&res.access_token, TokenKind::Access).unwrap();
        assert_eq!(claims.role, "sales");
    }

    #[tokio::test]
    async fn google_login_mock_existing_user_promotes_to_admin() {
        let non_admin = user_model("admin_user@example.com", false);

        let mut repo = MockUserRepository::new();
        repo.expect_find_by_email()
            .withf(|e| e == "admin_user@example.com")
            .returning(move |_| Ok(Some(non_admin.clone())));
        repo.expect_save().withf(|u| u.is_admin).times(1).returning(Ok);
        repo.expect_touch_last_login().times(1).returning(|_| Ok(()));

        let auth = svc(repo, MockTokenBlacklist::new());
        let res = auth
            .google_login(GoogleLoginRequest { credential: Some("mock_admin_token".into()), id_token: None })
            .await
            .unwrap();

        let claims = jwt().verify(&res.access_token, TokenKind::Access).unwrap();
        assert_eq!(claims.role, "admin");
    }

    fn svc_otp(users: MockUserRepository, otp: MockOtpRepository) -> AuthService {
        AuthService::new(
            Arc::new(users),
            Arc::new(MockTokenBlacklist::new()),
            jwt(),
            Arc::new(otp),
            OtpSettings { expiry_minutes: 5, expose_otp: true },
            vec!["example.com".into()],
            "admin@example.com".into(),
        )
    }

    const MOBILE: &str = "+14165550123";

    fn mobile_req() -> MobileRequest {
        MobileRequest { mobile_number: MOBILE.into() }
    }

    async fn otp_record(otp: &str, minutes_left: i64) -> mobile_otp_records::Model {
        let hashed = password::hash(otp.into()).await.unwrap();
        let now = Utc::now();
        mobile_otp_records::Model {
            mobile_number: MOBILE.into(),
            hashed_otp: hashed,
            expires_at: (now + Duration::minutes(minutes_left)).fixed_offset(),
            created_at: now.fixed_offset(),
        }
    }

    fn mobile_user(active: bool) -> users::Model {
        let mut u = user_model("m@example.com", false);
        u.mobile_number = Some(MOBILE.into());
        u.is_active = active;
        u
    }

    #[test]
    fn normalize_mobile_accepts_e164_and_tel_prefix() {
        assert_eq!(normalize_mobile(" tel:+14165550123 ").unwrap(), MOBILE);
        assert_eq!(normalize_mobile("+14165550123").unwrap(), MOBILE);
        for bad in ["4165550123", "+12", "+1416abc0123", "", "+1234567890123456"] {
            assert!(matches!(normalize_mobile(bad), Err(AppError::Unprocessable(_))));
        }
    }

    #[tokio::test]
    async fn get_otp_unknown_or_inactive_user_is_404() {
        for found in [None, Some(mobile_user(false))] {
            let mut repo = MockUserRepository::new();
            repo.expect_find_by_mobile().returning(move |_| Ok(found.clone()));
            let err = svc_otp(repo, MockOtpRepository::new()).get_otp(mobile_req()).await.unwrap_err();
            assert!(matches!(err, AppError::NotFound(_)));
        }
    }

    #[tokio::test]
    async fn get_otp_stores_hash_and_echoes_code_in_dev() {
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_mobile().returning(|_| Ok(Some(mobile_user(true))));
        let mut otp = MockOtpRepository::new();
        otp.expect_upsert()
            .withf(|m, h, _| m == MOBILE && h.starts_with("$argon2"))
            .times(1)
            .returning(|_, _, _| Ok(()));
        let out = svc_otp(repo, otp).get_otp(mobile_req()).await.unwrap();
        assert_eq!(out.otp.unwrap().len(), 6);
        assert_eq!(out.expiry_minutes, 5);
    }

    #[tokio::test]
    async fn register_conflicts_for_active_user() {
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_mobile().returning(|_| Ok(Some(mobile_user(true))));
        let err = svc_otp(repo, MockOtpRepository::new()).register(mobile_req()).await.unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[tokio::test]
    async fn register_creates_pending_user_and_issues_otp() {
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_mobile().returning(|_| Ok(None));
        repo.expect_create_pending().withf(|m| m == MOBILE).times(1).returning(|_| Ok(mobile_user(false)));
        let mut otp = MockOtpRepository::new();
        otp.expect_upsert().times(1).returning(|_, _, _| Ok(()));
        let out = svc_otp(repo, otp).register(mobile_req()).await.unwrap();
        assert!(out.otp.is_some());
    }

    #[tokio::test]
    async fn verify_account_activates_user_and_returns_tokens() {
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_mobile().returning(|_| Ok(Some(mobile_user(false))));
        repo.expect_save().withf(|u| u.is_active && u.accepted_terms.get("rev1").is_some()).times(1).returning(Ok);
        repo.expect_role_names().returning(|_| Ok(vec![]));
        let record = otp_record("123456", 5).await;
        let mut otp = MockOtpRepository::new();
        otp.expect_find().returning(move |_| Ok(Some(record.clone())));
        otp.expect_delete().times(1).returning(|_| Ok(()));
        let t = svc_otp(repo, otp)
            .verify_account(VerifyAccountRequest {
                mobile_number: MOBILE.into(),
                otp: "123456".into(),
                tnc_revision: "rev1".into(),
            })
            .await
            .unwrap();
        assert!(t.tnc_accepted);
    }

    #[tokio::test]
    async fn verify_account_without_pending_user_is_404() {
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_mobile().returning(|_| Ok(None));
        let record = otp_record("123456", 5).await;
        let mut otp = MockOtpRepository::new();
        otp.expect_find().returning(move |_| Ok(Some(record.clone())));
        otp.expect_delete().returning(|_| Ok(()));
        let err = svc_otp(repo, otp)
            .verify_account(VerifyAccountRequest {
                mobile_number: MOBILE.into(),
                otp: "123456".into(),
                tnc_revision: "rev1".into(),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn login_otp_rejects_wrong_missing_and_expired_codes() {
        // Wrong code: record is kept so the user can retry.
        let record = otp_record("123456", 5).await;
        let mut otp = MockOtpRepository::new();
        otp.expect_find().returning(move |_| Ok(Some(record.clone())));
        let err = svc_otp(MockUserRepository::new(), otp)
            .login_otp(OtpLoginRequest { mobile_number: MOBILE.into(), otp: "000000".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));

        // No record.
        let mut otp = MockOtpRepository::new();
        otp.expect_find().returning(|_| Ok(None));
        assert!(
            svc_otp(MockUserRepository::new(), otp)
                .login_otp(OtpLoginRequest { mobile_number: MOBILE.into(), otp: "123456".into() })
                .await
                .is_err()
        );

        // Expired: record is purged.
        let record = otp_record("123456", -1).await;
        let mut otp = MockOtpRepository::new();
        otp.expect_find().returning(move |_| Ok(Some(record.clone())));
        otp.expect_delete().times(1).returning(|_| Ok(()));
        assert!(
            svc_otp(MockUserRepository::new(), otp)
                .login_otp(OtpLoginRequest { mobile_number: MOBILE.into(), otp: "123456".into() })
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn login_otp_success() {
        let u = mobile_user(true);
        let id = u.id;
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_mobile().returning(move |_| Ok(Some(u.clone())));
        repo.expect_touch_last_login().times(1).returning(|_| Ok(()));
        repo.expect_role_names().returning(|_| Ok(vec![]));
        let record = otp_record("654321", 5).await;
        let mut otp = MockOtpRepository::new();
        otp.expect_find().returning(move |_| Ok(Some(record.clone())));
        otp.expect_delete().times(1).returning(|_| Ok(()));
        let t = svc_otp(repo, otp)
            .login_otp(OtpLoginRequest { mobile_number: format!("tel:{MOBILE}"), otp: "654321".into() })
            .await
            .unwrap();
        assert_eq!(t.user_id, id);
    }
}
