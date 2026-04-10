use std::fmt::{self, Display};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    models::DbPool,
    utils::{Editable, EditorForm},
    Error,
};

#[derive(FromRow, Serialize, Deserialize, Default, Debug)]
pub struct Page {
    id: Option<i32>,
    title: String,
    content: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

const SELECT_ALL_PAGES_SQL: &str =
    "SELECT id, title, content, created_at, updated_at FROM pages ORDER BY id DESC";
const SELECT_ALL_PAGE_TITLES_SQL: &str = "SELECT title FROM pages ORDER BY title ASC";

impl Page {
    pub async fn get_all(db: &DbPool) -> Vec<Self> {
        match db {
            DbPool::MySql(pool) => sqlx::query_as(SELECT_ALL_PAGES_SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_as(SELECT_ALL_PAGES_SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
        }
    }

    pub async fn get_all_titles(db: &DbPool) -> Vec<String> {
        match db {
            DbPool::MySql(pool) => sqlx::query_scalar(SELECT_ALL_PAGE_TITLES_SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
            DbPool::Postgres(pool) => sqlx::query_scalar(SELECT_ALL_PAGE_TITLES_SQL)
                .fetch_all(pool)
                .await
                .unwrap_or_default(),
        }
    }

    pub async fn get_by_id(db: &DbPool, id: i32) -> Option<Self> {
        match db {
            DbPool::MySql(pool) => sqlx::query_as(
                "SELECT id, title, content, created_at, updated_at FROM pages WHERE id = ?",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .ok(),
            DbPool::Postgres(pool) => sqlx::query_as(
                "SELECT id, title, content, created_at, updated_at FROM pages WHERE id = $1",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .ok(),
        }
    }

    pub async fn get_by_title(db: &DbPool, title: &str) -> Option<Self> {
        match db {
            DbPool::MySql(pool) => sqlx::query_as(
                "SELECT id, title, content, created_at, updated_at FROM pages WHERE LOWER(title) = LOWER(?)",
            )
            .bind(title)
            .fetch_one(pool)
            .await
            .ok(),
            DbPool::Postgres(pool) => sqlx::query_as(
                "SELECT id, title, content, created_at, updated_at FROM pages WHERE LOWER(title) = LOWER($1)",
            )
            .bind(title)
            .fetch_one(pool)
            .await
            .ok(),
        }
    }
}

async fn check_title_exists_mysql(
    conn: &mut sqlx::MySqlConnection,
    title: &str,
) -> Result<Option<i32>, Error> {
    sqlx::query_scalar::<_, i32>("SELECT id FROM pages WHERE LOWER(title) = LOWER(?)")
        .bind(title)
        .fetch_optional(conn)
        .await
        .map_err(|e| e.into())
}

async fn check_title_exists_postgres(
    conn: &mut sqlx::PgConnection,
    title: &str,
) -> Result<Option<i32>, Error> {
    sqlx::query_scalar::<_, i32>("SELECT id FROM pages WHERE LOWER(title) = LOWER($1)")
        .bind(title)
        .fetch_optional(conn)
        .await
        .map_err(|e| e.into())
}

impl Display for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "page")?;
        if let Some(id) = self.id {
            write!(f, " {}", id)?;
        }
        if !self.title.is_empty() {
            write!(f, " <{}>", self.title)?;
        }
        Ok(())
    }
}

impl Editable for Page {
    fn get_redirect_url(&self) -> String {
        format!("/{}", self.title.to_lowercase())
    }

    async fn update(&self, db: &DbPool) -> Result<Self, Error> {
        let id = match self.id {
            Some(id) => id,
            None => return Err(sqlx::Error::RowNotFound.into()),
        };

        match db {
            DbPool::MySql(pool) => {
                let mut tx = pool.begin().await?;
                if let Some(id_exists) = check_title_exists_mysql(&mut tx, &self.title).await? {
                    if id_exists != id {
                        return Err(Error::PageTitleExists(self.title.clone()));
                    }
                }
                sqlx::query(
                    "UPDATE pages SET title = ?, content = ?, updated_at = NOW() WHERE id = ?",
                )
                .bind(&self.title)
                .bind(&self.content)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
            }
            DbPool::Postgres(pool) => {
                let mut tx = pool.begin().await?;
                if let Some(id_exists) = check_title_exists_postgres(&mut tx, &self.title).await? {
                    if id_exists != id {
                        return Err(Error::PageTitleExists(self.title.clone()));
                    }
                }
                sqlx::query(
                    "UPDATE pages SET title = $1, content = $2, updated_at = NOW() WHERE id = $3",
                )
                .bind(&self.title)
                .bind(&self.content)
                .bind(id)
                .execute(&mut *tx)
                .await?;
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
                if check_title_exists_mysql(&mut tx, &self.title)
                    .await?
                    .is_some()
                {
                    return Err(Error::PageTitleExists(self.title.clone()));
                }
                sqlx::query(
                    "INSERT INTO pages (title, content, created_at, updated_at) VALUES (?, ?, NOW(), NOW())",
                )
                .bind(&self.title)
                .bind(&self.content)
                .execute(&mut *tx)
                .await?;
                let id = sqlx::query_scalar::<_, u64>("SELECT LAST_INSERT_ID()")
                    .fetch_one(&mut *tx)
                    .await? as i32;
                tx.commit().await?;
                id
            }
            DbPool::Postgres(pool) => {
                let mut tx = pool.begin().await?;
                if check_title_exists_postgres(&mut tx, &self.title)
                    .await?
                    .is_some()
                {
                    return Err(Error::PageTitleExists(self.title.clone()));
                }
                let id: i32 = sqlx::query_scalar(
                    "INSERT INTO pages (title, content, created_at, updated_at) VALUES ($1, $2, NOW(), NOW()) RETURNING id",
                )
                .bind(&self.title)
                .bind(&self.content)
                .fetch_one(&mut *tx)
                .await?;
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
                sqlx::query("DELETE FROM pages WHERE id = ?")
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
            DbPool::Postgres(pool) => {
                sqlx::query("DELETE FROM pages WHERE id = $1")
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(())
    }
}
impl From<EditorForm> for Page {
    fn from(form: EditorForm) -> Self {
        Page {
            id: form.id,
            title: form.title.unwrap_or_default().trim().to_string(),
            content: form.content.unwrap_or_default(),
            ..Default::default()
        }
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

    async fn truncate_pages(db: &DbPool) {
        match db {
            DbPool::MySql(pool) => {
                sqlx::query("DELETE FROM pages")
                    .execute(pool)
                    .await
                    .unwrap();
            }
            DbPool::Postgres(pool) => {
                sqlx::query("DELETE FROM pages")
                    .execute(pool)
                    .await
                    .unwrap();
            }
        }
    }

    async fn setup(db: &DbPool) {
        init_schema(db).await.unwrap();
        truncate_pages(db).await;
    }

    fn make_page(title: &str, content: &str) -> Page {
        Page::from(EditorForm {
            id: None,
            slug: None,
            title: Some(title.to_string()),
            tags: None,
            content: Some(content.to_string()),
        })
    }

    fn edit_page(id: Option<i32>, title: &str, content: &str) -> Page {
        Page::from(EditorForm {
            id,
            slug: None,
            title: Some(title.to_string()),
            tags: None,
            content: Some(content.to_string()),
        })
    }

    // ---------- pure-logic tests (no DB) ----------

    #[test]
    fn test_page_from_editor_form_trims_title() {
        let page = Page::from(EditorForm {
            id: None,
            slug: None,
            title: Some("  About  ".to_string()),
            tags: None,
            content: Some("body".to_string()),
        });
        assert_eq!(page.title, "About");
        assert_eq!(page.content, "body");
    }

    #[test]
    fn test_page_from_editor_form_preserves_id() {
        let page = Page::from(EditorForm {
            id: Some(7),
            slug: None,
            title: Some("t".to_string()),
            tags: None,
            content: None,
        });
        assert_eq!(page.id, Some(7));
    }

    #[test]
    fn test_page_redirect_url_lowercases_title() {
        let page = Page {
            id: Some(1),
            title: "About".to_string(),
            ..Default::default()
        };
        assert_eq!(page.get_redirect_url(), "/about");
    }

    #[test]
    fn test_page_display_format() {
        let empty = Page::default();
        assert_eq!(format!("{}", empty), "page");

        let with_id = Page {
            id: Some(3),
            ..Default::default()
        };
        assert_eq!(format!("{}", with_id), "page 3");

        let full = Page {
            id: Some(3),
            title: "About".to_string(),
            ..Default::default()
        };
        assert_eq!(format!("{}", full), "page 3 <About>");
    }

    // ---------- insert path ----------

    #[tokio::test]
    async fn test_insert_persists_all_fields() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let before = Utc::now();
        let inserted = make_page("About", "Hello world")
            .insert(&pool)
            .await
            .unwrap();

        assert!(inserted.id.is_some());
        assert_eq!(inserted.title, "About");
        assert_eq!(inserted.content, "Hello world");
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
    async fn test_insert_rejects_exact_duplicate_title() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        make_page("About", "first").insert(&pool).await.unwrap();
        let result = make_page("About", "second").insert(&pool).await;
        assert!(matches!(result, Err(Error::PageTitleExists(_))));
    }

    #[tokio::test]
    async fn test_insert_rejects_case_different_duplicate_title() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        make_page("About", "first").insert(&pool).await.unwrap();
        // LOWER(title) = LOWER(?) makes "ABOUT" collide with "About".
        let result = make_page("ABOUT", "second").insert(&pool).await;
        assert!(matches!(result, Err(Error::PageTitleExists(_))));
    }

    // ---------- read path ----------

    #[tokio::test]
    async fn test_get_all_empty() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(Page::get_all(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_orders_by_id_desc() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let first = make_page("First", "c").insert(&pool).await.unwrap();
        let second = make_page("Second", "c").insert(&pool).await.unwrap();
        let third = make_page("Third", "c").insert(&pool).await.unwrap();

        let all = Page::get_all(&pool).await;
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, third.id);
        assert_eq!(all[1].id, second.id);
        assert_eq!(all[2].id, first.id);
    }

    #[tokio::test]
    async fn test_get_all_titles_empty() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(Page::get_all_titles(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_titles_returns_sorted_ascending() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        make_page("Charlie", "c").insert(&pool).await.unwrap();
        make_page("Alpha", "c").insert(&pool).await.unwrap();
        make_page("Bravo", "c").insert(&pool).await.unwrap();

        let titles = Page::get_all_titles(&pool).await;
        assert_eq!(titles, vec!["Alpha", "Bravo", "Charlie"]);
    }

    #[tokio::test]
    async fn test_get_by_id_returns_none_when_missing() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(Page::get_by_id(&pool, 99999).await.is_none());
    }

    #[tokio::test]
    async fn test_get_by_title_case_insensitive() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_page("About", "c").insert(&pool).await.unwrap();

        // LOWER(title) = LOWER(?) must match regardless of input casing.
        assert_eq!(
            Page::get_by_title(&pool, "about").await.map(|p| p.id),
            Some(inserted.id)
        );
        assert_eq!(
            Page::get_by_title(&pool, "ABOUT").await.map(|p| p.id),
            Some(inserted.id)
        );
        assert_eq!(
            Page::get_by_title(&pool, "AbOuT").await.map(|p| p.id),
            Some(inserted.id)
        );
    }

    #[tokio::test]
    async fn test_get_by_title_returns_none_when_missing() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(Page::get_by_title(&pool, "nonexistent").await.is_none());
    }

    // ---------- update path ----------

    #[tokio::test]
    async fn test_update_without_id_returns_error() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        let page = make_page("No ID", "content");
        let result = page.update(&pool).await;
        assert!(result.is_err(), "update with no id should return Error");
    }

    #[tokio::test]
    async fn test_update_persists_all_fields() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_page("About", "Original").insert(&pool).await.unwrap();
        let updated = edit_page(inserted.id, "About", "Updated")
            .update(&pool)
            .await
            .unwrap();

        assert_eq!(updated.id, inserted.id);
        assert_eq!(updated.title, "About");
        assert_eq!(updated.content, "Updated");

        let fetched = Page::get_by_id(&pool, inserted.id.unwrap()).await.unwrap();
        assert_eq!(fetched.content, "Updated");
    }

    #[tokio::test]
    async fn test_update_allows_same_title_on_same_page() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_page("About", "v1").insert(&pool).await.unwrap();
        // Updating to the exact same title on the same row must NOT trip the
        // title-collision check, because the collision is against itself.
        let result = edit_page(inserted.id, "About", "v2").update(&pool).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_rejects_title_collision_with_another_page() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        make_page("About", "c").insert(&pool).await.unwrap();
        let contact = make_page("Contact", "c").insert(&pool).await.unwrap();

        // Try to rename "Contact" to "About" (case-insensitive collision).
        let result = edit_page(contact.id, "about", "c").update(&pool).await;
        assert!(matches!(result, Err(Error::PageTitleExists(_))));
    }

    #[tokio::test]
    async fn test_update_preserves_created_at_and_bumps_updated_at() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_page("t", "v1").insert(&pool).await.unwrap();
        let original_created_at = inserted.created_at;
        let original_updated_at = inserted.updated_at;

        tokio::time::sleep(Duration::from_millis(1100)).await;

        let updated = edit_page(inserted.id, "t", "v2")
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
        let page = make_page("No ID", "content");
        let result = page.delete(&pool).await;
        assert!(result.is_err(), "delete with no id should return Error");
    }

    #[tokio::test]
    async fn test_delete_removes_page() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let inserted = make_page("Doomed", "c").insert(&pool).await.unwrap();
        let id = inserted.id.unwrap();

        inserted.delete(&pool).await.unwrap();

        assert!(Page::get_by_id(&pool, id).await.is_none());
        assert!(Page::get_by_title(&pool, "Doomed").await.is_none());
        // Title freed up: a fresh insert with the same title must succeed.
        make_page("Doomed", "v2").insert(&pool).await.unwrap();
    }
}
