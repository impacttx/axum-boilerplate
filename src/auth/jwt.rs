use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::JwtConfig,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Access,
    Refresh,
}

/// Claim names are compatible with the legacy FastAPI service (`refresh`, `role`, `jti`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub jti: String,
    pub refresh: bool,
    pub role: String,
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub nbf: i64,
    pub exp: i64,
}

impl Claims {
    pub fn expires_at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.exp, 0).unwrap_or_else(Utc::now)
    }
}

pub struct IssuedToken {
    pub token: String,
    pub claims: Claims,
}

pub struct JwtKeys {
    algorithm: Algorithm,
    encoding: EncodingKey,
    decoding: DecodingKey,
    issuer: String,
    audience: String,
    access_ttl_secs: i64,
    refresh_ttl_secs: i64,
}

impl JwtKeys {
    pub fn new(cfg: &JwtConfig) -> Result<Self> {
        let algorithm: Algorithm = cfg.algorithm.parse().context("invalid JWT_ALGORITHM")?;
        let (encoding, decoding) = match algorithm {
            Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::PS256
            | Algorithm::PS384
            | Algorithm::PS512 => (
                EncodingKey::from_rsa_pem(cfg.private_key.as_bytes()),
                DecodingKey::from_rsa_pem(cfg.public_key.as_bytes()),
            ),
            Algorithm::ES256 | Algorithm::ES384 => (
                EncodingKey::from_ec_pem(cfg.private_key.as_bytes()),
                DecodingKey::from_ec_pem(cfg.public_key.as_bytes()),
            ),
            Algorithm::EdDSA => (
                EncodingKey::from_ed_pem(cfg.private_key.as_bytes()),
                DecodingKey::from_ed_pem(cfg.public_key.as_bytes()),
            ),
            other => bail!("{other:?} is symmetric; an asymmetric JWT_ALGORITHM is required"),
        };
        Ok(Self {
            algorithm,
            encoding: encoding.context("JWT_PRIVATE_KEY is not a valid PEM key")?,
            decoding: decoding.context("JWT_PUBLIC_KEY is not a valid PEM key")?,
            issuer: cfg.issuer.clone(),
            audience: cfg.audience.clone(),
            access_ttl_secs: cfg.access_ttl_secs,
            refresh_ttl_secs: cfg.refresh_ttl_secs,
        })
    }

    pub fn access_ttl_secs(&self) -> i64 {
        self.access_ttl_secs
    }

    pub fn issue(&self, sub: Uuid, role: &str, kind: TokenKind) -> AppResult<IssuedToken> {
        let ttl = match kind {
            TokenKind::Access => self.access_ttl_secs,
            TokenKind::Refresh => self.refresh_ttl_secs,
        };
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub,
            jti: Uuid::new_v4().to_string(),
            refresh: kind == TokenKind::Refresh,
            role: role.to_string(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            iat: now,
            nbf: now,
            exp: now + ttl,
        };
        let token =
            encode(&Header::new(self.algorithm), &claims, &self.encoding).map_err(|e| AppError::Internal(e.into()))?;
        Ok(IssuedToken { token, claims })
    }

    /// Verifies signature, expiry, not-before, issuer, audience and token kind.
    pub fn verify(&self, token: &str, expected: TokenKind) -> AppResult<Claims> {
        let mut validation = Validation::new(self.algorithm);
        validation.leeway = 0;
        validation.validate_nbf = true;
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.set_required_spec_claims(&["exp", "nbf", "sub", "iss", "aud"]);
        let claims = decode::<Claims>(token, &self.decoding, &validation).map_err(|_| AppError::Unauthorized)?.claims;
        if claims.refresh != (expected == TokenKind::Refresh) {
            return Err(AppError::Unauthorized);
        }
        Ok(claims)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn test_config() -> JwtConfig {
        JwtConfig {
            private_key: include_str!("../../tests/fixtures/test_rsa_private.pem").into(),
            public_key: include_str!("../../tests/fixtures/test_rsa_public.pem").into(),
            algorithm: "RS256".into(),
            access_ttl_secs: 60,
            refresh_ttl_secs: 600,
            audience: "http://aud/".into(),
            issuer: "http://iss/".into(),
        }
    }

    fn keys() -> JwtKeys {
        JwtKeys::new(&test_config()).unwrap()
    }

    #[test]
    fn issue_and_verify() {
        let id = Uuid::new_v4();
        let t = keys().issue(id, "sales", TokenKind::Access).unwrap();
        let c = keys().verify(&t.token, TokenKind::Access).unwrap();
        assert_eq!((c.sub, c.role.as_str(), c.jti.as_str()), (id, "sales", t.claims.jti.as_str()));
    }

    #[test]
    fn rejects_wrong_kind() {
        let t = keys().issue(Uuid::new_v4(), "admin", TokenKind::Refresh).unwrap();
        assert!(keys().verify(&t.token, TokenKind::Access).is_err());
        let t = keys().issue(Uuid::new_v4(), "admin", TokenKind::Access).unwrap();
        assert!(keys().verify(&t.token, TokenKind::Refresh).is_err());
    }

    #[test]
    fn rejects_expired() {
        let mut cfg = test_config();
        cfg.access_ttl_secs = -10;
        let k = JwtKeys::new(&cfg).unwrap();
        let t = k.issue(Uuid::new_v4(), "admin", TokenKind::Access).unwrap();
        assert!(k.verify(&t.token, TokenKind::Access).is_err());
    }

    #[test]
    fn rejects_wrong_issuer_or_audience() {
        let t = keys().issue(Uuid::new_v4(), "admin", TokenKind::Access).unwrap();
        for mutate in [(|c: &mut JwtConfig| c.issuer = "http://other/".into()) as fn(&mut JwtConfig), |c| {
            c.audience = "http://other/".into()
        }] {
            let mut cfg = test_config();
            mutate(&mut cfg);
            assert!(JwtKeys::new(&cfg).unwrap().verify(&t.token, TokenKind::Access).is_err());
        }
    }

    #[test]
    fn rejects_tampered_token_and_symmetric_alg() {
        let t = keys().issue(Uuid::new_v4(), "admin", TokenKind::Access).unwrap();
        let mut bad = t.token.clone();
        bad.push('x');
        assert!(keys().verify(&bad, TokenKind::Access).is_err());

        let mut cfg = test_config();
        cfg.algorithm = "HS256".into();
        assert!(JwtKeys::new(&cfg).is_err());
    }
}
