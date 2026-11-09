use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, DatabaseConnection, EntityTrait, IntoActiveModel, PaginatorTrait, QueryOrder, QuerySelect, Set,
};
use uuid::Uuid;

use crate::{
    entities::items::{self, Entity as Items},
    error::AppResult,
};

#[derive(Debug, Clone)]
pub struct CreateItem {
    pub title: String,
    pub description: Option<String>,
    pub is_completed: bool,
}

#[derive(Debug, Clone)]
pub struct UpdateItem {
    pub title: Option<String>,
    pub description: Option<String>,
    pub is_completed: Option<bool>,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ItemRepository: Send + Sync {
    async fn create(&self, payload: CreateItem) -> AppResult<items::Model>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<items::Model>>;
    async fn list(&self, limit: u64, offset: u64) -> AppResult<Vec<items::Model>>;
    async fn count(&self) -> AppResult<u64>;
    async fn update(&self, id: Uuid, payload: UpdateItem) -> AppResult<Option<items::Model>>;
    async fn delete(&self, id: Uuid) -> AppResult<bool>;
}

#[derive(Clone)]
pub struct SeaOrmItemRepository {
    db: DatabaseConnection,
}

impl SeaOrmItemRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ItemRepository for SeaOrmItemRepository {
    async fn create(&self, payload: CreateItem) -> AppResult<items::Model> {
        let now = chrono::Utc::now();
        let item = items::ActiveModel {
            id: Set(Uuid::new_v4()),
            title: Set(payload.title),
            description: Set(payload.description),
            is_completed: Set(payload.is_completed),
            created_at: Set(now.into()),
            updated_at: Set(None),
        };
        let res = item.insert(&self.db).await?;
        Ok(res)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<items::Model>> {
        let res = Items::find_by_id(id).one(&self.db).await?;
        Ok(res)
    }

    async fn list(&self, limit: u64, offset: u64) -> AppResult<Vec<items::Model>> {
        let res =
            Items::find().order_by_desc(items::Column::CreatedAt).offset(offset).limit(limit).all(&self.db).await?;
        Ok(res)
    }

    async fn count(&self) -> AppResult<u64> {
        let res = Items::find().count(&self.db).await?;
        Ok(res)
    }

    async fn update(&self, id: Uuid, payload: UpdateItem) -> AppResult<Option<items::Model>> {
        let existing = match Items::find_by_id(id).one(&self.db).await? {
            Some(m) => m,
            None => return Ok(None),
        };
        let mut active = existing.into_active_model();
        if let Some(t) = payload.title {
            active.title = Set(t);
        }
        if let Some(d) = payload.description {
            active.description = Set(Some(d));
        }
        if let Some(c) = payload.is_completed {
            active.is_completed = Set(c);
        }
        active.updated_at = Set(Some(chrono::Utc::now().into()));
        let updated = active.update(&self.db).await?;
        Ok(Some(updated))
    }

    async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let res = Items::delete_by_id(id).exec(&self.db).await?;
        Ok(res.rows_affected > 0)
    }
}
