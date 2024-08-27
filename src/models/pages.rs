use std::fmt::{self, Display};

use axum::async_trait;
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
        sqlx::query_as("SELECT * FROM pages ORDER BY id DESC")
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
        sqlx::query_as("SELECT * FROM pages WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await
            .ok()
    }

    pub async fn get_by_title(db: &sqlx::MySqlPool, title: &str) -> Option<Self> {
        // check the lowercase version of the title
        sqlx::query_as("SELECT * FROM pages WHERE LOWER(title) = LOWER(?)")
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

#[async_trait]
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

        Ok(Self::get_by_id(db, id).await.unwrap())
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

        Ok(Self::get_by_id(db, id).await.unwrap())
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
