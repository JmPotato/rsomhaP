use std::fmt::{self, Display};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use tracing::info;

use crate::{
    utils::{sort_out_tags, Editable, EditorForm},
    Error,
};

#[derive(FromRow, Serialize, Deserialize, Default)]
pub struct Article {
    id: Option<i32>,
    slug: String,
    title: String,
    pub content: String,
    pub tags: String,
    pub created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Article {
    // TODO: refine the error result handling.
    pub async fn get_all(db: &sqlx::MySqlPool) -> Vec<Self> {
        sqlx::query_as("SELECT id, slug, title, content, tags, created_at, updated_at FROM articles ORDER BY id DESC")
            .fetch_all(db)
            .await
            .unwrap_or_default()
    }

    pub async fn get_recent(db: &sqlx::MySqlPool, limit: u32) -> Vec<Self> {
        sqlx::query_as("SELECT id, slug, title, content, tags, created_at, updated_at FROM articles ORDER BY id DESC LIMIT ?")
            .bind(limit)
            .fetch_all(db)
            .await
            .unwrap_or_default()
    }

    pub async fn get_on_page(db: &sqlx::MySqlPool, page: u32, article_per_page: u32) -> Vec<Self> {
        sqlx::query_as("SELECT id, slug, title, content, tags, created_at, updated_at FROM articles ORDER BY id DESC LIMIT ? OFFSET ?")
            .bind(article_per_page)
            .bind((page - 1) * article_per_page)
            .fetch_all(db)
            .await
            .unwrap_or_default()
    }

    pub async fn get_total_count(db: &sqlx::MySqlPool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM articles")
            .fetch_one(db)
            .await
            .unwrap_or_default()
    }

    pub async fn get_by_id(db: &sqlx::MySqlPool, id: i32) -> Option<Self> {
        sqlx::query_as("SELECT id, slug, title, content, tags, created_at, updated_at FROM articles WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await
            .ok()
    }

    pub async fn get_by_slug(db: &sqlx::MySqlPool, slug: &str) -> Option<Self> {
        sqlx::query_as("SELECT id, slug, title, content, tags, created_at, updated_at FROM articles WHERE slug = ?")
            .bind(slug)
            .fetch_one(db)
            .await
            .ok()
    }

    pub async fn get_by_tag(db: &sqlx::MySqlPool, tag: &str) -> Vec<Self> {
        sqlx::query_as(
            "SELECT a.id, a.slug, a.title, a.content, a.tags, a.created_at, a.updated_at
             FROM articles AS a
             INNER JOIN tags AS t ON a.id = t.article_id
             WHERE t.name = ?
             ORDER BY a.id DESC",
        )
        .bind(tag)
        .fetch_all(db)
        .await
        .unwrap_or_default()
    }

    pub async fn get_latest_updated(db: &sqlx::MySqlPool) -> Option<DateTime<Utc>> {
        sqlx::query_scalar("SELECT MAX(updated_at) FROM articles")
            .fetch_one(db)
            .await
            .ok()
    }

    async fn clear_tags(&self, tx: &mut sqlx::Transaction<'_, sqlx::MySql>) -> Result<(), Error> {
        sqlx::query("DELETE FROM tags WHERE article_id = ?")
            .bind(self.id)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }
}

impl Display for Article {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "article")?;
        if let Some(id) = self.id {
            write!(f, " {}", id)?;
        }
        if !self.title.is_empty() {
            write!(f, " <{}>", self.title)?;
        }
        if !self.tags.is_empty() {
            write!(f, " [{}]", self.tags)?;
        }
        Ok(())
    }
}

impl Editable for Article {
    fn get_redirect_url(&self) -> String {
        if !self.slug.is_empty() {
            format!("/article/{}", self.slug)
        } else if let Some(id) = self.id {
            format!("/article/{}", id)
        } else {
            "/".to_string()
        }
    }

    async fn update(&self, db: &sqlx::MySqlPool) -> Result<Self, Error> {
        let id = match self.id {
            Some(id) => id,
            None => return Err(sqlx::Error::RowNotFound.into()),
        };

        let mut tx = db.begin().await?;

        // update the articles table
        sqlx::query(
            "UPDATE articles SET slug = ?, title = ?, content = ?, tags = ?, updated_at = NOW() WHERE id = ?",
        )
        .bind(&self.slug)
        .bind(&self.title)
        .bind(&self.content)
        .bind(&self.tags)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        info!("updated article {} with id {}", self.title, id);
        // update the tags table
        self.clear_tags(&mut tx).await?;
        info!("cleared tags for article {}", id);
        Tags::insert_tags(&mut tx, &self.tags, id).await?;
        info!("inserted tags {} for article {}", self.tags, id);

        tx.commit().await?;

        Ok(Self::get_by_id(db, id)
            .await
            .ok_or(sqlx::Error::RowNotFound)?)
    }

    async fn insert(&self, db: &sqlx::MySqlPool) -> Result<Self, Error> {
        let mut tx = db.begin().await?;

        // insert into the articles table
        sqlx::query(
            "INSERT INTO articles (slug, title, content, tags, created_at, updated_at) VALUES (?, ?, ?, ?, NOW(), NOW())",
        )
        .bind(&self.slug)
        .bind(&self.title)
        .bind(&self.content)
        .bind(&self.tags)
        .execute(&mut *tx)
        .await?;
        // get the last inserted id
        let id = sqlx::query_scalar::<_, u64>("SELECT LAST_INSERT_ID()")
            .fetch_one(&mut *tx)
            .await? as i32;
        info!("inserted article {} with id {}", self.title, id);
        // insert into the tags table
        Tags::insert_tags(&mut tx, &self.tags, id).await?;
        info!("inserted tags: {}", self.tags);

        tx.commit().await?;

        Ok(Self::get_by_id(db, id)
            .await
            .ok_or(sqlx::Error::RowNotFound)?)
    }

    async fn delete(&self, db: &sqlx::MySqlPool) -> Result<(), Error> {
        let id = match self.id {
            Some(id) => id,
            None => return Err(sqlx::Error::RowNotFound.into()),
        };

        let mut tx = db.begin().await?;

        // delete the article
        sqlx::query("DELETE FROM articles WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        info!("deleted article: {}", id);
        // delete the tags
        self.clear_tags(&mut tx).await?;
        info!("cleared tags for article {}", id);

        tx.commit().await.map_err(|e| e.into())
    }
}

impl From<EditorForm> for Article {
    fn from(from: EditorForm) -> Self {
        // Normalize the slug by replacing the whitespace with hyphen.
        let slug = from
            .slug
            .unwrap_or_default()
            .trim()
            .chars()
            .map(|c| {
                if c.is_whitespace() {
                    '-'
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect();
        Article {
            id: from.id,
            slug,
            title: from.title.unwrap_or_default().trim().to_string(),
            tags: sort_out_tags(&from.tags.unwrap_or_default()),
            content: from.content.unwrap_or_default(),
            ..Default::default()
        }
    }
}

#[derive(FromRow, Serialize)]
pub struct Tags {
    name: String,
    num: i32,
}

impl Tags {
    pub async fn get_all_with_count(db: &sqlx::MySqlPool) -> Vec<Self> {
        sqlx::query_as("SELECT name, COUNT(name) AS num FROM tags GROUP BY name ORDER BY num DESC")
            .fetch_all(db)
            .await
            .unwrap_or_default()
    }

    async fn insert_tags(
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
        tags: &str,
        article_id: i32,
    ) -> Result<(), Error> {
        for tag in tags.split(',').map(|s| s.trim()) {
            if tag.is_empty() {
                continue;
            }
            sqlx::query("INSERT INTO tags (name, article_id) VALUES (?, ?)")
                .bind(tag)
                .bind(article_id)
                .execute(&mut **tx)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::create_tables_within_transaction;
    use crate::utils::EditorForm;

    async fn get_test_pool() -> Option<sqlx::MySqlPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Some(sqlx::MySqlPool::connect(&url).await.unwrap())
    }

    /// Ensures tables exist and clears article/tag data for a clean test.
    async fn setup(pool: &sqlx::MySqlPool) {
        create_tables_within_transaction(pool).await.unwrap();
        sqlx::query("DELETE FROM tags")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM articles")
            .execute(pool)
            .await
            .unwrap();
    }

    fn make_article(slug: Option<&str>, title: &str, tags: &str, content: &str) -> Article {
        Article::from(EditorForm {
            id: None,
            slug: slug.map(|s| s.to_string()),
            title: Some(title.to_string()),
            tags: Some(tags.to_string()),
            content: Some(content.to_string()),
        })
    }

    // --- insert slug handling ---

    #[tokio::test]
    async fn test_insert_preserves_slug() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let article = make_article(Some("my-first-post"), "First Post", "rust", "Hello");
        let inserted = article.insert(&pool).await.unwrap();

        // Slug should be persisted.
        let fetched = Article::get_by_slug(&pool, "my-first-post").await;
        assert!(fetched.is_some(), "should be retrievable by slug");
        assert_eq!(fetched.unwrap().id, inserted.id);
    }

    #[tokio::test]
    async fn test_insert_empty_slug() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let article = make_article(None, "No Slug Post", "rust", "Content");
        let inserted = article.insert(&pool).await.unwrap();

        // Should still succeed with empty slug; retrievable by id.
        assert!(inserted.id.is_some());
        let fetched = Article::get_by_id(&pool, inserted.id.unwrap()).await;
        assert!(fetched.is_some());
    }

    // --- update/delete error handling ---

    #[tokio::test]
    async fn test_update_without_id_returns_error() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        let article = make_article(None, "No ID", "", "");
        let result = article.update(&pool).await;
        assert!(result.is_err(), "update with no id should return Error");
    }

    #[tokio::test]
    async fn test_delete_without_id_returns_error() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        let article = make_article(None, "No ID", "", "");
        let result = article.delete(&pool).await;
        assert!(result.is_err(), "delete with no id should return Error");
    }

    // --- insert/update happy path ---

    #[tokio::test]
    async fn test_insert_then_update_returns_ok() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let article = make_article(Some("slug-v1"), "Title v1", "rust", "Content v1");
        let inserted = article.insert(&pool).await.unwrap();
        assert!(inserted.id.is_some());

        // Build an updated version with the same id.
        let updated_form = Article::from(EditorForm {
            id: inserted.id,
            slug: Some("slug-v2".to_string()),
            title: Some("Title v2".to_string()),
            tags: Some("rust, wasm".to_string()),
            content: Some("Content v2".to_string()),
        });
        // EditorForm -> Article sets id from form.id, but we need to verify it's set.
        assert_eq!(updated_form.id, inserted.id);

        let updated = updated_form.update(&pool).await.unwrap();
        assert_eq!(updated.id, inserted.id);
        // Verify changes persisted.
        let fetched = Article::get_by_slug(&pool, "slug-v2").await.unwrap();
        assert_eq!(fetched.content, "Content v2");
    }

    // --- get_total_count ---

    #[tokio::test]
    async fn test_get_total_count() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        assert_eq!(Article::get_total_count(&pool).await, 0);

        make_article(None, "A", "", "a")
            .insert(&pool)
            .await
            .unwrap();
        assert_eq!(Article::get_total_count(&pool).await, 1);

        make_article(None, "B", "", "b")
            .insert(&pool)
            .await
            .unwrap();
        make_article(None, "C", "", "c")
            .insert(&pool)
            .await
            .unwrap();
        assert_eq!(Article::get_total_count(&pool).await, 3);
    }
}
