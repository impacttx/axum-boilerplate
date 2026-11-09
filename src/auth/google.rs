//! Verification of Google Sign-In ID tokens against Google's published signing keys.
use std::time::{Duration, Instant};

use async_trait::async_trait;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult};

const CERTS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const ISSUERS: [&str; 2] = ["accounts.google.com", "https://accounts.google.com"];
const CACHE_TTL: Duration = Duration::from_secs(3600);
/// Minimum gap between key refreshes triggered by an unknown `kid`.
const REFRESH_COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleProfile {
    pub email: String,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub picture: Option<String>,
}

/// Seam for the identity provider so the login flow can be unit tested.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait GoogleVerifier: Send + Sync {
    /// Validates the token (signature, issuer, audience, expiry, verified email).
    async fn verify(&self, id_token: &str) -> AppResult<GoogleProfile>;
}

#[derive(Deserialize)]
struct GoogleClaims {
    email: Option<String>,
    #[serde(default)]
    email_verified: bool,
    given_name: Option<String>,
    family_name: Option<String>,
    picture: Option<String>,
}

pub struct GoogleJwksVerifier {
    client_id: String,
    http: reqwest::Client,
    keys: RwLock<Option<(Instant, JwkSet)>>,
}

impl GoogleJwksVerifier {
    pub fn new(client_id: String) -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().expect("reqwest client");
        Self { client_id, http, keys: RwLock::new(None) }
    }

    async fn key_for(&self, kid: &str) -> AppResult<DecodingKey> {
        if let Some((fetched, set)) = &*self.keys.read().await
            && fetched.elapsed() < CACHE_TTL
            && let Some(k) = Self::find(set, kid)
        {
            return Ok(k);
        }
        let mut guard = self.keys.write().await;
        // Another task may have refreshed while we waited, or we may be inside the cooldown.
        let stale = guard.as_ref().is_none_or(|(t, _)| t.elapsed() >= REFRESH_COOLDOWN);
        if stale {
            let set: JwkSet = self
                .http
                .get(CERTS_URL)
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| AppError::Internal(e.into()))?
                .json()
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            *guard = Some((Instant::now(), set));
        }
        guard.as_ref().and_then(|(_, set)| Self::find(set, kid)).ok_or(AppError::Unauthorized)
    }

    fn find(set: &JwkSet, kid: &str) -> Option<DecodingKey> {
        DecodingKey::from_jwk(set.find(kid)?).ok()
    }
}

#[async_trait]
impl GoogleVerifier for GoogleJwksVerifier {
    async fn verify(&self, id_token: &str) -> AppResult<GoogleProfile> {
        let header = decode_header(id_token).map_err(|_| AppError::Unauthorized)?;
        if header.alg != Algorithm::RS256 {
            return Err(AppError::Unauthorized);
        }
        let kid = header.kid.ok_or(AppError::Unauthorized)?;
        let key = self.key_for(&kid).await?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.client_id]);
        validation.set_issuer(&ISSUERS);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        let claims = decode::<GoogleClaims>(id_token, &key, &validation)
            .map_err(|e| {
                tracing::warn!(error = %e, "google id token rejected");
                AppError::Unauthorized
            })?
            .claims;

        match claims.email {
            Some(email) if claims.email_verified && !email.is_empty() => Ok(GoogleProfile {
                email,
                given_name: claims.given_name,
                family_name: claims.family_name,
                picture: claims.picture,
            }),
            _ => Err(AppError::Unauthorized),
        }
    }
}
