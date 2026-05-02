use std::{
    collections::HashMap,
    fmt::{self, Display},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use tracing::info;

use crate::{
    models::DbPool,
    utils::{iter_tags, sort_out_tags, Editable, EditorForm},
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

#[derive(Clone, FromRow, Serialize, Deserialize, Default)]
pub struct ArticleSummary {
    id: Option<i32>,
    slug: String,
    title: String,
    tags: String,
    pub created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ArticleSummary {
    pub fn has_tag(&self, tag: &str) -> bool {
        iter_tags(&self.tags).any(|name| name == tag)
    }

    pub fn latest_updated(articles: &[Self]) -> Option<DateTime<Utc>> {
        articles.iter().map(|article| article.updated_at).max()
    }
}

impl Article {
    pub async fn get_all_summaries(db: &DbPool) -> Vec<ArticleSummary> {
        const SQL: &str = "SELECT id, slug, title, tags, created_at, updated_at \
                           FROM articles ORDER BY id DESC";
        match db {
            DbPool::MySql(pool) => sqlx::query_as(SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_as(SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
        }
    }

    #[cfg(test)]
    pub async fn get_all(db: &DbPool) -> Vec<Self> {
        // No parameters → the SQL string is identical on both backends.
        const SQL: &str = "SELECT id, slug, title, content, tags, created_at, updated_at \
                           FROM articles ORDER BY id DESC";
        match db {
            DbPool::MySql(pool) => sqlx::query_as(SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_as(SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
        }
    }

    pub async fn get_recent(db: &DbPool, limit: u32) -> Vec<Self> {
        // sqlx-postgres 0.8 does not implement `Encode<Postgres> for u32`;
        // widen to i64 before bind. Both backends get the same i64.
        let limit = i64::from(limit);
        match db {
            DbPool::MySql(pool) => sqlx::query_as(
                "SELECT id, slug, title, content, tags, created_at, updated_at \
                 FROM articles ORDER BY id DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
            .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_as(
                "SELECT id, slug, title, content, tags, created_at, updated_at \
                 FROM articles ORDER BY id DESC LIMIT $1",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
            .unwrap_or_default(),
        }
    }

    #[cfg(test)]
    pub async fn get_on_page(db: &DbPool, page: u32, article_per_page: u32) -> Vec<Self> {
        // Widen to i64 *before* multiplying: `(page - 1) * article_per_page`
        // on u32 could overflow for large inputs, and sqlx-postgres 0.8 does
        // not accept u32 binds. `saturating_sub(1)` also defends against a
        // `page = 0` underflow even though handlers already filter that out.
        let limit = i64::from(article_per_page);
        let offset = i64::from(page.saturating_sub(1)) * i64::from(article_per_page);
        match db {
            DbPool::MySql(pool) => sqlx::query_as(
                "SELECT id, slug, title, content, tags, created_at, updated_at \
                 FROM articles ORDER BY id DESC LIMIT ? OFFSET ?",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_as(
                "SELECT id, slug, title, content, tags, created_at, updated_at \
                 FROM articles ORDER BY id DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .unwrap_or_default(),
        }
    }

    #[cfg(test)]
    pub async fn get_total_count(db: &DbPool) -> i64 {
        const SQL: &str = "SELECT COUNT(*) FROM articles";
        match db {
            DbPool::MySql(pool) => sqlx::query_scalar(SQL)
                .fetch_one(pool)
                .await
                .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_scalar(SQL)
                .fetch_one(pool)
                .await
                .unwrap_or_default(),
        }
    }

    pub async fn get_by_id(db: &DbPool, id: i32) -> Option<Self> {
        match db {
            DbPool::MySql(pool) => sqlx::query_as(
                "SELECT id, slug, title, content, tags, created_at, updated_at \
                 FROM articles WHERE id = ?",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .ok(),
            DbPool::Postgres(pool) => sqlx::query_as(
                "SELECT id, slug, title, content, tags, created_at, updated_at \
                 FROM articles WHERE id = $1",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .ok(),
        }
    }

    pub async fn get_by_slug(db: &DbPool, slug: &str) -> Option<Self> {
        match db {
            DbPool::MySql(pool) => sqlx::query_as(
                "SELECT id, slug, title, content, tags, created_at, updated_at \
                 FROM articles WHERE slug = ?",
            )
            .bind(slug)
            .fetch_one(pool)
            .await
            .ok(),
            DbPool::Postgres(pool) => sqlx::query_as(
                "SELECT id, slug, title, content, tags, created_at, updated_at \
                 FROM articles WHERE slug = $1",
            )
            .bind(slug)
            .fetch_one(pool)
            .await
            .ok(),
        }
    }

    #[cfg(test)]
    pub async fn get_by_tag(db: &DbPool, tag: &str) -> Vec<Self> {
        match db {
            DbPool::MySql(pool) => sqlx::query_as(
                "SELECT a.id, a.slug, a.title, a.content, a.tags, a.created_at, a.updated_at \
                 FROM articles AS a \
                 INNER JOIN tags AS t ON a.id = t.article_id \
                 WHERE t.name = ? \
                 ORDER BY a.id DESC",
            )
            .bind(tag)
            .fetch_all(pool)
            .await
            .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_as(
                "SELECT a.id, a.slug, a.title, a.content, a.tags, a.created_at, a.updated_at \
                 FROM articles AS a \
                 INNER JOIN tags AS t ON a.id = t.article_id \
                 WHERE t.name = $1 \
                 ORDER BY a.id DESC",
            )
            .bind(tag)
            .fetch_all(pool)
            .await
            .unwrap_or_default(),
        }
    }
}

// --- transaction helpers (kept as free fns so the outer Editable methods
// stay readable; the transaction types are concrete per backend and cannot
// be abstracted across both at the same call site). ---

async fn clear_tags_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    article_id: i32,
) -> Result<(), Error> {
    sqlx::query("DELETE FROM tags WHERE article_id = ?")
        .bind(article_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn clear_tags_postgres(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    article_id: i32,
) -> Result<(), Error> {
    sqlx::query("DELETE FROM tags WHERE article_id = $1")
        .bind(article_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
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
    const REFRESH_ARTICLE_CACHES: bool = true;

    fn get_redirect_url(&self) -> String {
        if !self.slug.is_empty() {
            format!("/article/{}", self.slug)
        } else if let Some(id) = self.id {
            format!("/article/{}", id)
        } else {
            "/".to_string()
        }
    }

    async fn update(&self, db: &DbPool) -> Result<Self, Error> {
        let id = match self.id {
            Some(id) => id,
            None => return Err(sqlx::Error::RowNotFound.into()),
        };

        match db {
            DbPool::MySql(pool) => {
                let mut tx = pool.begin().await?;
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
                clear_tags_mysql(&mut tx, id).await?;
                info!("cleared tags for article {}", id);
                Tags::insert_tags_mysql(&mut tx, &self.tags, id).await?;
                info!("inserted tags {} for article {}", self.tags, id);
                tx.commit().await?;
            }
            DbPool::Postgres(pool) => {
                let mut tx = pool.begin().await?;
                sqlx::query(
                    "UPDATE articles SET slug = $1, title = $2, content = $3, tags = $4, updated_at = NOW() WHERE id = $5",
                )
                .bind(&self.slug)
                .bind(&self.title)
                .bind(&self.content)
                .bind(&self.tags)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                info!("updated article {} with id {}", self.title, id);
                clear_tags_postgres(&mut tx, id).await?;
                info!("cleared tags for article {}", id);
                Tags::insert_tags_postgres(&mut tx, &self.tags, id).await?;
                info!("inserted tags {} for article {}", self.tags, id);
                tx.commit().await?;
            }
        }

        Self::get_by_id(db, id)
            .await
            .ok_or_else(|| sqlx::Error::RowNotFound.into())
    }

    async fn insert(&self, db: &DbPool) -> Result<Self, Error> {
        let id = match db {
            DbPool::MySql(pool) => {
                let mut tx = pool.begin().await?;
                sqlx::query(
                    "INSERT INTO articles (slug, title, content, tags, created_at, updated_at) VALUES (?, ?, ?, ?, NOW(), NOW())",
                )
                .bind(&self.slug)
                .bind(&self.title)
                .bind(&self.content)
                .bind(&self.tags)
                .execute(&mut *tx)
                .await?;
                let id = sqlx::query_scalar::<_, u64>("SELECT LAST_INSERT_ID()")
                    .fetch_one(&mut *tx)
                    .await? as i32;
                info!("inserted article {} with id {}", self.title, id);
                Tags::insert_tags_mysql(&mut tx, &self.tags, id).await?;
                info!("inserted tags: {}", self.tags);
                tx.commit().await?;
                id
            }
            DbPool::Postgres(pool) => {
                let mut tx = pool.begin().await?;
                let id: i32 = sqlx::query_scalar(
                    "INSERT INTO articles (slug, title, content, tags, created_at, updated_at) VALUES ($1, $2, $3, $4, NOW(), NOW()) RETURNING id",
                )
                .bind(&self.slug)
                .bind(&self.title)
                .bind(&self.content)
                .bind(&self.tags)
                .fetch_one(&mut *tx)
                .await?;
                info!("inserted article {} with id {}", self.title, id);
                Tags::insert_tags_postgres(&mut tx, &self.tags, id).await?;
                info!("inserted tags: {}", self.tags);
                tx.commit().await?;
                id
            }
        };

        Self::get_by_id(db, id)
            .await
            .ok_or_else(|| sqlx::Error::RowNotFound.into())
    }

    async fn delete(&self, db: &DbPool) -> Result<(), Error> {
        let id = match self.id {
            Some(id) => id,
            None => return Err(sqlx::Error::RowNotFound.into()),
        };

        match db {
            DbPool::MySql(pool) => {
                let mut tx = pool.begin().await?;
                sqlx::query("DELETE FROM articles WHERE id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                info!("deleted article: {}", id);
                clear_tags_mysql(&mut tx, id).await?;
                info!("cleared tags for article {}", id);
                tx.commit().await?;
            }
            DbPool::Postgres(pool) => {
                let mut tx = pool.begin().await?;
                sqlx::query("DELETE FROM articles WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                info!("deleted article: {}", id);
                clear_tags_postgres(&mut tx, id).await?;
                info!("cleared tags for article {}", id);
                tx.commit().await?;
            }
        }

        Ok(())
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
}

impl Tags {
    pub fn from_article_summaries(articles: &[ArticleSummary]) -> Vec<Self> {
        let mut counts = HashMap::<&str, usize>::new();
        for article in articles {
            for tag in iter_tags(&article.tags) {
                *counts.entry(tag).or_default() += 1;
            }
        }

        let mut tags = counts.into_iter().collect::<Vec<_>>();
        tags.sort_by(|(left_name, left_count), (right_name, right_count)| {
            right_count
                .cmp(left_count)
                .then_with(|| left_name.cmp(right_name))
        });
        tags.into_iter()
            .map(|(name, _)| Self {
                name: name.to_string(),
            })
            .collect()
    }

    #[cfg(test)]
    pub async fn get_all_with_count(db: &DbPool) -> Vec<Self> {
        const SQL: &str = "SELECT name FROM tags GROUP BY name ORDER BY COUNT(name) DESC, name ASC";
        match db {
            DbPool::MySql(pool) => sqlx::query_as(SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_as(SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
        }
    }

    async fn insert_tags_mysql(
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
        tags: &str,
        article_id: i32,
    ) -> Result<(), Error> {
        for tag in iter_tags(tags) {
            sqlx::query("INSERT INTO tags (name, article_id) VALUES (?, ?)")
                .bind(tag)
                .bind(article_id)
                .execute(&mut **tx)
                .await?;
        }
        Ok(())
    }

    async fn insert_tags_postgres(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tags: &str,
        article_id: i32,
    ) -> Result<(), Error> {
        for tag in iter_tags(tags) {
            sqlx::query("INSERT INTO tags (name, article_id) VALUES ($1, $2)")
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
    use crate::models::init_schema;
    use crate::utils::EditorForm;
    use std::time::Duration;

    // ---------- test helpers ----------

    async fn get_test_pool() -> Option<DbPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Some(DbPool::connect(&url).await.unwrap())
    }

    async fn truncate_articles_and_tags(db: &DbPool) {
        match db {
            DbPool::MySql(pool) => {
                sqlx::query("DELETE FROM tags").execute(pool).await.unwrap();
                sqlx::query("DELETE FROM articles")
                    .execute(pool)
                    .await
                    .unwrap();
            }
            DbPool::Postgres(pool) => {
                sqlx::query("DELETE FROM tags").execute(pool).await.unwrap();
                sqlx::query("DELETE FROM articles")
                    .execute(pool)
                    .await
                    .unwrap();
            }
        }
    }

    /// Ensures tables exist and clears article/tag data for a clean test.
    async fn setup(db: &DbPool) {
        init_schema(db).await.unwrap();
        truncate_articles_and_tags(db).await;
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

    fn make_summary(id: i32, title: &str, tags: &str) -> ArticleSummary {
        let now = Utc::now();
        ArticleSummary {
            id: Some(id),
            slug: title.to_string(),
            title: title.to_string(),
            tags: tags.to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    fn make_summary_updated_at(id: i32, updated_at: DateTime<Utc>) -> ArticleSummary {
        ArticleSummary {
            id: Some(id),
            slug: id.to_string(),
            title: id.to_string(),
            tags: String::new(),
            created_at: updated_at,
            updated_at,
        }
    }

    /// Count rows in the tags table for a given `article_id` via raw SQL.
    async fn tags_row_count_for(db: &DbPool, article_id: i32) -> i64 {
        match db {
            DbPool::MySql(pool) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tags WHERE article_id = ?")
                    .bind(article_id)
                    .fetch_one(pool)
                    .await
                    .unwrap()
            }
            DbPool::Postgres(pool) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tags WHERE article_id = $1")
                    .bind(article_id)
                    .fetch_one(pool)
                    .await
                    .unwrap()
            }
        }
    }

    // ---------- pure-logic tests (no DB) ----------

    #[test]
    fn test_article_refreshes_only_article_caches() {
        assert!(<Article as Editable>::REFRESH_ARTICLE_CACHES);
        assert!(!<Article as Editable>::REFRESH_PAGE_TITLES_CACHE);
    }

    #[test]
    fn test_article_from_editor_form_normalizes_slug() {
        let article = Article::from(EditorForm {
            id: None,
            slug: Some("  Hello World  ".to_string()),
            title: Some("t".to_string()),
            tags: None,
            content: None,
        });
        assert_eq!(article.slug, "hello-world");
    }

    #[test]
    fn test_article_from_editor_form_empty_slug_becomes_empty_string() {
        let article = Article::from(EditorForm {
            id: None,
            slug: None,
            title: Some("t".to_string()),
            tags: None,
            content: None,
        });
        assert_eq!(article.slug, "");
    }

    #[test]
    fn test_article_from_editor_form_trims_title() {
        let article = Article::from(EditorForm {
            id: None,
            slug: None,
            title: Some("  Hello  ".to_string()),
            tags: None,
            content: None,
        });
        assert_eq!(article.title, "Hello");
    }

    #[test]
    fn test_article_from_editor_form_sorts_and_dedupes_tags() {
        let article = Article::from(EditorForm {
            id: None,
            slug: None,
            title: Some("t".to_string()),
            tags: Some("wasm, rust, rust, async".to_string()),
            content: None,
        });
        assert_eq!(article.tags, "async, rust, wasm");
    }

    #[test]
    fn test_article_from_editor_form_preserves_id() {
        let article = Article::from(EditorForm {
            id: Some(42),
            slug: None,
            title: Some("t".to_string()),
            tags: None,
            content: None,
        });
        assert_eq!(article.id, Some(42));
    }

    #[test]
    fn test_article_redirect_url_prefers_slug() {
        let article = Article {
            id: Some(5),
            slug: "my-post".to_string(),
            ..Default::default()
        };
        assert_eq!(article.get_redirect_url(), "/article/my-post");
    }

    #[test]
    fn test_article_redirect_url_falls_back_to_id() {
        let article = Article {
            id: Some(5),
            slug: String::new(),
            ..Default::default()
        };
        assert_eq!(article.get_redirect_url(), "/article/5");
    }

    #[test]
    fn test_article_redirect_url_with_no_id_or_slug() {
        let article = Article::default();
        assert_eq!(article.get_redirect_url(), "/");
    }

    #[test]
    fn test_article_summary_has_tag_trims_tag_names() {
        let article = make_summary(1, "post", "rust, wasm, async");

        assert!(article.has_tag("rust"));
        assert!(article.has_tag("wasm"));
        assert!(article.has_tag("async"));
        assert!(!article.has_tag("go"));
    }

    #[test]
    fn test_tags_from_article_summaries_counts_and_orders() {
        let articles = vec![
            make_summary(1, "a", "rust"),
            make_summary(2, "b", "rust, wasm"),
            make_summary(3, "c", "rust, wasm, async"),
        ];

        let tags = Tags::from_article_summaries(&articles);
        let names: Vec<_> = tags.iter().map(|tag| tag.name.as_str()).collect();
        assert_eq!(names, vec!["rust", "wasm", "async"]);
    }

    #[test]
    fn test_article_summary_latest_updated_returns_max_timestamp() {
        let oldest = Utc::now() - chrono::Duration::seconds(20);
        let newest = Utc::now();
        let articles = vec![
            make_summary_updated_at(1, oldest),
            make_summary_updated_at(2, newest),
        ];

        assert_eq!(ArticleSummary::latest_updated(&articles), Some(newest));
        assert_eq!(ArticleSummary::latest_updated(&[]), None);
    }

    #[test]
    fn test_article_display_format() {
        let empty = Article::default();
        assert_eq!(format!("{}", empty), "article");

        let with_id = Article {
            id: Some(7),
            ..Default::default()
        };
        assert_eq!(format!("{}", with_id), "article 7");

        let full = Article {
            id: Some(7),
            title: "My Post".to_string(),
            tags: "rust, wasm".to_string(),
            ..Default::default()
        };
        assert_eq!(format!("{}", full), "article 7 <My Post> [rust, wasm]");
    }

    // ---------- insert path ----------

    #[tokio::test]
    async fn test_insert_persists_all_fields() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let before = Utc::now();
        let inserted = make_article(Some("Hello World"), "First Post", "rust, wasm", "Body")
            .insert(&pool)
            .await
            .unwrap();

        // id is populated by the INSERT (LAST_INSERT_ID on MySQL, RETURNING on Postgres).
        assert!(inserted.id.is_some());
        // Slug was lowercased and whitespace replaced with hyphen by EditorForm -> Article.
        assert_eq!(inserted.slug, "hello-world");
        assert_eq!(inserted.title, "First Post");
        assert_eq!(inserted.content, "Body");
        // Tags are normalized by sort_out_tags before insert.
        assert_eq!(inserted.tags, "rust, wasm");
        // Timestamps should be set by the DB to roughly now (generous tolerance
        // to absorb clock skew between the test process and the DB server).
        assert!(
            (inserted.created_at - before).num_seconds().abs() < 60,
            "created_at should be close to now, got {}",
            inserted.created_at
        );
        assert!(
            (inserted.updated_at - before).num_seconds().abs() < 60,
            "updated_at should be close to now, got {}",
            inserted.updated_at
        );
    }

    #[tokio::test]
    async fn test_insert_preserves_slug() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let article = make_article(Some("my-first-post"), "First Post", "rust", "Hello");
        let inserted = article.insert(&pool).await.unwrap();

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

        assert!(inserted.id.is_some());
        let fetched = Article::get_by_id(&pool, inserted.id.unwrap()).await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().slug, "");
    }

    #[tokio::test]
    async fn test_insert_creates_tag_rows_per_comma() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_article(None, "With Tags", "rust, wasm, async", "body")
            .insert(&pool)
            .await
            .unwrap();
        let id = inserted.id.unwrap();
        assert_eq!(tags_row_count_for(&pool, id).await, 3);
    }

    #[tokio::test]
    async fn test_insert_no_tag_rows_when_tags_empty() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_article(None, "Untagged", "", "body")
            .insert(&pool)
            .await
            .unwrap();
        assert_eq!(tags_row_count_for(&pool, inserted.id.unwrap()).await, 0);
    }

    // ---------- read path ----------

    #[tokio::test]
    async fn test_get_all_empty() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(Article::get_all(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_orders_by_id_desc() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let a = make_article(None, "first", "", "a")
            .insert(&pool)
            .await
            .unwrap();
        let b = make_article(None, "second", "", "b")
            .insert(&pool)
            .await
            .unwrap();
        let c = make_article(None, "third", "", "c")
            .insert(&pool)
            .await
            .unwrap();

        let all = Article::get_all(&pool).await;
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, c.id);
        assert_eq!(all[1].id, b.id);
        assert_eq!(all[2].id, a.id);
    }

    #[tokio::test]
    async fn test_get_all_summaries_orders_by_id_desc() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let a = make_article(None, "first", "rust", "a")
            .insert(&pool)
            .await
            .unwrap();
        let b = make_article(None, "second", "wasm", "b")
            .insert(&pool)
            .await
            .unwrap();
        let c = make_article(None, "third", "async", "c")
            .insert(&pool)
            .await
            .unwrap();

        let all = Article::get_all_summaries(&pool).await;
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, c.id);
        assert_eq!(all[0].title, "third");
        assert_eq!(all[0].tags, "async");
        assert_eq!(all[1].id, b.id);
        assert_eq!(all[2].id, a.id);
    }

    #[tokio::test]
    async fn test_get_recent_respects_limit_and_order() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        for n in 0..5 {
            make_article(None, &format!("t{n}"), "", "c")
                .insert(&pool)
                .await
                .unwrap();
        }

        let recent = Article::get_recent(&pool, 3).await;
        assert_eq!(recent.len(), 3);
        // Descending by id means the most recently inserted three.
        assert_eq!(recent[0].title, "t4");
        assert_eq!(recent[1].title, "t3");
        assert_eq!(recent[2].title, "t2");

        // Limit exceeding row count returns all rows.
        let all = Article::get_recent(&pool, 100).await;
        assert_eq!(all.len(), 5);
    }

    #[tokio::test]
    async fn test_get_on_page_pagination() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        for n in 0..5 {
            make_article(None, &format!("p{n}"), "", "c")
                .insert(&pool)
                .await
                .unwrap();
        }

        // Per page = 2: page 1 -> p4,p3; page 2 -> p2,p1; page 3 -> p0; page 4 -> [].
        let page1 = Article::get_on_page(&pool, 1, 2).await;
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].title, "p4");
        assert_eq!(page1[1].title, "p3");

        let page2 = Article::get_on_page(&pool, 2, 2).await;
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].title, "p2");
        assert_eq!(page2[1].title, "p1");

        let page3 = Article::get_on_page(&pool, 3, 2).await;
        assert_eq!(page3.len(), 1);
        assert_eq!(page3[0].title, "p0");

        let page4 = Article::get_on_page(&pool, 4, 2).await;
        assert!(page4.is_empty());
    }

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

    #[tokio::test]
    async fn test_get_by_id_returns_none_when_missing() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(Article::get_by_id(&pool, 99999).await.is_none());
    }

    #[tokio::test]
    async fn test_get_by_slug_returns_none_when_missing() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(Article::get_by_slug(&pool, "nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_get_by_tag_returns_matching_articles() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let rust = make_article(None, "rust-only", "rust", "c")
            .insert(&pool)
            .await
            .unwrap();
        let multi = make_article(None, "rust-wasm", "rust, wasm", "c")
            .insert(&pool)
            .await
            .unwrap();
        make_article(None, "untagged", "", "c")
            .insert(&pool)
            .await
            .unwrap();

        let rust_hits = Article::get_by_tag(&pool, "rust").await;
        let rust_ids: Vec<_> = rust_hits.iter().map(|a| a.id).collect();
        assert_eq!(rust_hits.len(), 2);
        assert!(rust_ids.contains(&rust.id));
        assert!(rust_ids.contains(&multi.id));

        let wasm_hits = Article::get_by_tag(&pool, "wasm").await;
        assert_eq!(wasm_hits.len(), 1);
        assert_eq!(wasm_hits[0].id, multi.id);
    }

    #[tokio::test]
    async fn test_get_by_tag_empty_for_unknown_tag() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        make_article(None, "t", "rust", "c")
            .insert(&pool)
            .await
            .unwrap();

        assert!(Article::get_by_tag(&pool, "nonexistent").await.is_empty());
    }

    // ---------- update path ----------

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
    async fn test_update_persists_all_fields() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_article(Some("slug-v1"), "Title v1", "rust", "Content v1")
            .insert(&pool)
            .await
            .unwrap();

        let updated = Article::from(EditorForm {
            id: inserted.id,
            slug: Some("slug-v2".to_string()),
            title: Some("Title v2".to_string()),
            tags: Some("rust, wasm".to_string()),
            content: Some("Content v2".to_string()),
        })
        .update(&pool)
        .await
        .unwrap();

        assert_eq!(updated.id, inserted.id);
        assert_eq!(updated.slug, "slug-v2");
        assert_eq!(updated.title, "Title v2");
        assert_eq!(updated.content, "Content v2");
        assert_eq!(updated.tags, "rust, wasm");

        // Round-trip via slug confirms persistence.
        let fetched = Article::get_by_slug(&pool, "slug-v2").await.unwrap();
        assert_eq!(fetched.content, "Content v2");
    }

    #[tokio::test]
    async fn test_update_syncs_tag_rows() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_article(None, "t", "rust, wasm", "c")
            .insert(&pool)
            .await
            .unwrap();
        let id = inserted.id.unwrap();
        assert_eq!(tags_row_count_for(&pool, id).await, 2);

        // Replace tags entirely: old ones must go, new ones must be present.
        Article::from(EditorForm {
            id: Some(id),
            slug: None,
            title: Some("t".to_string()),
            tags: Some("async".to_string()),
            content: Some("c".to_string()),
        })
        .update(&pool)
        .await
        .unwrap();

        assert_eq!(tags_row_count_for(&pool, id).await, 1);
        // Searching by the old tag must not return this article anymore.
        assert!(Article::get_by_tag(&pool, "rust")
            .await
            .iter()
            .all(|a| a.id != Some(id)));
        assert!(Article::get_by_tag(&pool, "wasm")
            .await
            .iter()
            .all(|a| a.id != Some(id)));
        // And the new tag must reach it.
        let by_async = Article::get_by_tag(&pool, "async").await;
        assert_eq!(by_async.len(), 1);
        assert_eq!(by_async[0].id, Some(id));
    }

    #[tokio::test]
    async fn test_update_preserves_created_at_and_bumps_updated_at() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_article(None, "t", "", "c")
            .insert(&pool)
            .await
            .unwrap();
        let original_created_at = inserted.created_at;
        let original_updated_at = inserted.updated_at;

        // MySQL DATETIME has second-level precision, so we need to cross a
        // second boundary for the timestamp comparison to be meaningful.
        tokio::time::sleep(Duration::from_millis(1100)).await;

        let updated = Article::from(EditorForm {
            id: inserted.id,
            slug: None,
            title: Some("t".to_string()),
            tags: None,
            content: Some("c2".to_string()),
        })
        .update(&pool)
        .await
        .unwrap();

        assert_eq!(
            updated.created_at, original_created_at,
            "created_at must not change on update"
        );
        assert!(
            updated.updated_at > original_updated_at,
            "updated_at must advance on update (was {original_updated_at}, now {})",
            updated.updated_at
        );
    }

    // ---------- delete path ----------

    #[tokio::test]
    async fn test_delete_without_id_returns_error() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        let article = make_article(None, "No ID", "", "");
        let result = article.delete(&pool).await;
        assert!(result.is_err(), "delete with no id should return Error");
    }

    #[tokio::test]
    async fn test_delete_removes_article_and_all_its_tag_rows() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_article(None, "doomed", "rust, wasm", "bye")
            .insert(&pool)
            .await
            .unwrap();
        let id = inserted.id.unwrap();
        assert_eq!(tags_row_count_for(&pool, id).await, 2);

        inserted.delete(&pool).await.unwrap();

        assert!(Article::get_by_id(&pool, id).await.is_none());
        assert_eq!(tags_row_count_for(&pool, id).await, 0);
        // Tag join lookup also finds nothing.
        assert!(Article::get_by_tag(&pool, "rust")
            .await
            .iter()
            .all(|a| a.id != Some(id)));
    }

    // ---------- Tags::get_all_with_count ----------

    #[tokio::test]
    async fn test_tags_get_all_with_count_empty() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(Tags::get_all_with_count(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn test_tags_get_all_with_count_and_ordering() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        // 3x rust, 2x wasm, 1x async
        make_article(None, "a", "rust", "c")
            .insert(&pool)
            .await
            .unwrap();
        make_article(None, "b", "rust, wasm", "c")
            .insert(&pool)
            .await
            .unwrap();
        make_article(None, "c", "rust, wasm, async", "c")
            .insert(&pool)
            .await
            .unwrap();

        let counts = Tags::get_all_with_count(&pool).await;
        let names: Vec<_> = counts.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["rust", "wasm", "async"]);
    }
}
