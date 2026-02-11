use std::fmt::{self, Display};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
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

impl Page {
    pub async fn get_all(db: &sqlx::MySqlPool) -> Vec<Self> {
        sqlx::query_as(
            "SELECT id, title, content, created_at, updated_at FROM pages ORDER BY id DESC",
        )
        .fetch_all(db)
        .await
        .unwrap_or_default()
    }

    pub async fn get_all_titles(db: &sqlx::MySqlPool) -> Vec<String> {
        sqlx::query_scalar("SELECT title FROM pages ORDER BY title ASC")
            .fetch_all(db)
            .await
            .unwrap_or_default()
    }

    pub async fn get_by_id(db: &sqlx::MySqlPool, id: i32) -> Option<Self> {
        sqlx::query_as("SELECT id, title, content, created_at, updated_at FROM pages WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await
            .ok()
    }

    pub async fn get_by_title(db: &sqlx::MySqlPool, title: &str) -> Option<Self> {
        // check the lowercase version of the title
        sqlx::query_as("SELECT id, title, content, created_at, updated_at FROM pages WHERE LOWER(title) = LOWER(?)")
            .bind(title)
            .fetch_one(db)
            .await
            .ok()
    }

    async fn check_title_exists(
        db: &mut sqlx::MySqlConnection,
        title: &str,
    ) -> Result<Option<i32>, Error> {
        sqlx::query_scalar::<_, i32>("SELECT id FROM pages WHERE LOWER(title) = LOWER(?)")
            .bind(title)
            .fetch_optional(db)
            .await
            .map_err(|e| e.into())
    }
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

    async fn update(&self, db: &sqlx::MySqlPool) -> Result<Self, Error> {
        let id = match self.id {
            Some(id) => id,
            None => return Err(sqlx::Error::RowNotFound.into()),
        };

        let mut tx = db.begin().await?;

        // check if the page already exists since we use its title as part of the URL.
        if let Some(id_exists) = Page::check_title_exists(&mut tx, &self.title).await? {
            if id_exists != id {
                return Err(Error::PageTitleExists(self.title.clone()));
            }
        }

        sqlx::query("UPDATE pages SET title = ?, content = ? WHERE id = ?")
            .bind(&self.title)
            .bind(&self.content)
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(Self::get_by_id(db, id)
            .await
            .ok_or(sqlx::Error::RowNotFound)?)
    }

    async fn insert(&self, db: &sqlx::MySqlPool) -> Result<Self, Error> {
        let mut tx = db.begin().await?;

        // check if the page already exists since we use its title as part of the URL.
        if Page::check_title_exists(&mut tx, &self.title)
            .await?
            .is_some()
        {
            return Err(Error::PageTitleExists(self.title.clone()));
        }

        sqlx::query("INSERT INTO pages (title, content) VALUES (?, ?)")
            .bind(&self.title)
            .bind(&self.content)
            .execute(&mut *tx)
            .await?;
        // get the last inserted id.
        let id = sqlx::query_scalar::<_, u64>("SELECT LAST_INSERT_ID()")
            .fetch_one(&mut *tx)
            .await? as i32;

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
        sqlx::query("DELETE FROM pages WHERE id = ?")
            .bind(id)
            .execute(db)
            .await
            .map_err(|e| e.into())
            .map(|_| ())
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
    use crate::models::create_tables_within_transaction;
    use crate::utils::EditorForm;

    async fn get_test_pool() -> Option<sqlx::MySqlPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Some(sqlx::MySqlPool::connect(&url).await.unwrap())
    }

    async fn setup(pool: &sqlx::MySqlPool) {
        create_tables_within_transaction(pool).await.unwrap();
        sqlx::query("DELETE FROM pages")
            .execute(pool)
            .await
            .unwrap();
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

    // --- update/delete error handling ---

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
    async fn test_delete_without_id_returns_error() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        let page = make_page("No ID", "content");
        let result = page.delete(&pool).await;
        assert!(result.is_err(), "delete with no id should return Error");
    }

    // --- insert/update happy path ---

    #[tokio::test]
    async fn test_insert_then_update_returns_ok() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        let page = make_page("About", "Original content");
        let inserted = page.insert(&pool).await.unwrap();
        assert!(inserted.id.is_some());

        let updated_page = Page::from(EditorForm {
            id: inserted.id,
            slug: None,
            title: Some("About".to_string()),
            tags: None,
            content: Some("Updated content".to_string()),
        });
        let updated = updated_page.update(&pool).await.unwrap();
        assert_eq!(updated.id, inserted.id);

        let fetched = Page::get_by_id(&pool, inserted.id.unwrap()).await.unwrap();
        assert_eq!(fetched.content, "Updated content");
    }
}
