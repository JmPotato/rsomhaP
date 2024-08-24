use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::prelude::FromRow;

#[derive(FromRow, Serialize)]
pub struct Page {
    pub id: i32,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Page {
    pub async fn get_all(db: &sqlx::MySqlPool) -> Vec<Self> {
        sqlx::query_as("SELECT * FROM pages ORDER BY id DESC")
            .fetch_all(db)
            .await
            .unwrap()
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

    pub async fn update(
        db: &sqlx::MySqlPool,
        id: i32,
        title: &str,
        content: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query("UPDATE pages SET title = ?, content = ? WHERE id = ?")
            .bind(title)
            .bind(content)
            .bind(id)
            .execute(db)
            .await?;

        Ok(Self::get_by_id(&db, id).await.unwrap())
    }

    pub async fn insert(
        db: &sqlx::MySqlPool,
        title: &str,
        content: &str,
    ) -> Result<Self, sqlx::Error> {
        let mut tx = db.begin().await?;

        sqlx::query("INSERT INTO pages (title, content) VALUES (?, ?)")
            .bind(title)
            .bind(content)
            .execute(&mut *tx)
            .await?;
        // get the last inserted id
        let id = sqlx::query_scalar::<_, u64>("SELECT LAST_INSERT_ID()")
            .fetch_one(&mut *tx)
            .await? as i32;

        tx.commit().await?;

        Ok(Self::get_by_id(&db, id).await.unwrap())
    }

    pub async fn delete(db: &sqlx::MySqlPool, id: i32) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM pages WHERE id = ?")
            .bind(id)
            .execute(db)
            .await?;
        Ok(())
    }
}
