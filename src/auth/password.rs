use std::sync::OnceLock;

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

use crate::error::{AppError, AppResult};

/// Same parameters as the legacy service (OWASP Argon2id guidance); verification of
/// existing hashes uses the parameters embedded in each hash.
fn hasher() -> Argon2<'static> {
    let params = Params::new(64 * 1024, 2, 2, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn is_hash(stored: &str) -> bool {
    stored.starts_with("$argon2")
}

fn hash_sync(password: &str) -> AppResult<String> {
    hasher()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash: {e}")))
}

fn verify_sync(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored)
        .map(|parsed| hasher().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// Argon2 is CPU/memory heavy, so it runs on the blocking thread pool.
pub async fn hash(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || hash_sync(&password)).await.map_err(|e| AppError::Internal(e.into()))?
}

/// Returns `false` for wrong passwords and for stored values that are not valid hashes
/// (null, legacy plaintext, ...).
pub async fn verify(password: String, stored: String) -> AppResult<bool> {
    tokio::task::spawn_blocking(move || verify_sync(&password, &stored)).await.map_err(|e| AppError::Internal(e.into()))
}

/// Burns the same CPU as a real verification so unknown accounts are not distinguishable by timing.
pub async fn verify_dummy(password: String) -> AppResult<()> {
    static DUMMY: OnceLock<String> = OnceLock::new();
    tokio::task::spawn_blocking(move || {
        let stored = DUMMY.get_or_init(|| hash_sync("dummy-password").unwrap_or_default());
        verify_sync(&password, stored);
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_and_verify() {
        let h = hash("s3cret-pass".into()).await.unwrap();
        assert!(h.starts_with("$argon2id$") && is_hash(&h));
        assert!(verify("s3cret-pass".into(), h.clone()).await.unwrap());
        assert!(!verify("wrong".into(), h).await.unwrap());
    }

    #[tokio::test]
    async fn non_hash_values_never_verify() {
        assert!(!verify("plain".into(), "plain".into()).await.unwrap());
        assert!(!verify("x".into(), String::new()).await.unwrap());
        assert!(!is_hash("plain"));
    }
}
