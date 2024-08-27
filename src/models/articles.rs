use std::fmt::{self, Display};

use axum::async_trait;
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
    title: String,
    pub content: String,
    pub tags: String,
    pub created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Article {
    pub async fn get_all(db: &sqlx::MySqlPool) -> Vec<Self> {
        sqlx::query_as("SELECT * FROM articles ORDER BY id DESC")
            .fetch_all(db)
            .await
            .unwrap_or_default()
    }

    pub async fn get_on_page(db: &sqlx::MySqlPool, page: u32, article_per_page: u32) -> Vec<Self> {
        sqlx::query_as("SELECT * FROM articles ORDER BY id DESC LIMIT ? OFFSET ?")
            .bind(article_per_page)
            .bind((page - 1) * article_per_page)
            .fetch_all(db)
            .await
            .unwrap_or_default()
    }

    pub async fn get_total_count(db: &sqlx::MySqlPool) -> i32 {
        sqlx::query_scalar("SELECT COUNT(*) FROM articles")
            .fetch_one(db)
            .await
            .unwrap_or_default()
    }

    pub async fn get_by_id(db: &sqlx::MySqlPool, id: i32) -> Option<Self> {
        sqlx::query_as("SELECT * FROM articles WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await
            .ok()
    }

    pub async fn get_by_tag(db: &sqlx::MySqlPool, tag: &str) -> Vec<Self> {
        sqlx::query_as(
            "SELECT a.id, a.title, a.content, a.tags, a.created_at, a.updated_at
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

#[async_trait]
impl Editable for Article {
    fn get_redirect_url(&self) -> String {
        match self.id {
            Some(id) => format!("/article/{}", id),
            None => "/".to_string(),
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
            "UPDATE articles SET title = ?, content = ?, tags = ?, updated_at = NOW() WHERE id = ?",
        )
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

        Ok(Self::get_by_id(db, id).await.unwrap())
    }

    async fn insert(&self, db: &sqlx::MySqlPool) -> Result<Self, Error> {
        let mut tx = db.begin().await?;

        // insert into the articles table
        sqlx::query(
            "INSERT INTO articles (title, content, tags, created_at, updated_at) VALUES (?, ?, ?, NOW(), NOW())",
        )
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

        Ok(Self::get_by_id(db, id).await.unwrap())
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
        Article {
            id: from.id,
            // trim the title and tags to remove leading and trailing whitespace and commas
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
