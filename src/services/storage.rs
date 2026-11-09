use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignedUrlInfo {
    pub url: String,
    #[schema(value_type = Object)]
    pub fields: serde_json::Value,
    pub public_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssetInfo {
    pub key: String,
    pub url: String,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait FileStore: Send + Sync {
    async fn put(&self, name: &str, content_type: &str, data: Vec<u8>) -> AppResult<String>;
    async fn put_with_key(&self, key: &str, content_type: &str, data: Vec<u8>) -> AppResult<String>;
    async fn list(&self, prefix: &str) -> AppResult<Vec<String>>;
    async fn delete(&self, key: &str) -> AppResult<()>;
    fn get_cdn_url(&self, key: &str) -> String;
}

fn valid_name(name: &str) -> AppResult<()> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError::BadRequest("invalid filename".into()));
    }
    Ok(())
}

fn safe_key_join(root: &Path, key: &str) -> AppResult<PathBuf> {
    let clean = key.trim_start_matches('/');
    if clean.is_empty() || clean.contains("..") || clean.starts_with('.') {
        return Err(AppError::BadRequest("invalid file key".into()));
    }
    Ok(root.join(clean))
}

pub struct LocalFileStore {
    root: PathBuf,
}

impl LocalFileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl FileStore for LocalFileStore {
    async fn put(&self, name: &str, content_type: &str, data: Vec<u8>) -> AppResult<String> {
        valid_name(name)?;
        self.put_with_key(name, content_type, data).await
    }

    async fn put_with_key(&self, key: &str, _content_type: &str, data: Vec<u8>) -> AppResult<String> {
        let target = safe_key_join(&self.root, key)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| AppError::Internal(e.into()))?;
        }
        tokio::fs::write(&target, data).await.map_err(|e| AppError::Internal(e.into()))?;
        Ok(self.get_cdn_url(key))
    }

    async fn list(&self, prefix: &str) -> AppResult<Vec<String>> {
        let dir = safe_key_join(&self.root, prefix).unwrap_or_else(|_| self.root.clone());
        let mut keys = Vec::new();
        if dir.exists() {
            let mut read_dir = tokio::fs::read_dir(&dir).await.map_err(|e| AppError::Internal(e.into()))?;
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if let Ok(rel) = entry.path().strip_prefix(&self.root) {
                    keys.push(rel.to_string_lossy().to_string());
                }
            }
        }
        Ok(keys)
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let target = safe_key_join(&self.root, key)?;
        if target.exists() {
            tokio::fs::remove_file(&target).await.map_err(|e| AppError::Internal(e.into()))?;
        }
        Ok(())
    }

    fn get_cdn_url(&self, key: &str) -> String {
        format!("/uploads/{}", key.trim_start_matches('/'))
    }
}
