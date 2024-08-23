use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::prelude::FromRow;
use tracing::info;

#[derive(FromRow, Serialize)]
pub struct Article {
    pub id: i32,
    title: String,
    pub content: String,
    pub tags: String,
    pub created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl Article {
    pub async fn get_all(db: &sqlx::MySqlPool) -> Vec<Self> {
        sqlx::query_as("SELECT * FROM articles ORDER BY id DESC")
            .fetch_all(db)
            .await
            .unwrap()
    }

    pub async fn get_on_page(db: &sqlx::MySqlPool, page: u32, article_per_page: u32) -> Vec<Self> {
        sqlx::query_as("SELECT * FROM articles ORDER BY id DESC LIMIT ? OFFSET ?")
            .bind(article_per_page)
            .bind((page - 1) * article_per_page)
            .fetch_all(db)
            .await
            .unwrap()
    }

    pub async fn get_total_count(db: &sqlx::MySqlPool) -> i32 {
        sqlx::query_scalar("SELECT COUNT(*) FROM articles")
            .fetch_one(db)
            .await
            .unwrap()
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
        .unwrap()
    }

    pub async fn get_latest_updated(db: &sqlx::MySqlPool) -> Option<NaiveDateTime> {
        sqlx::query_scalar("SELECT MAX(updated_at) FROM articles")
            .fetch_one(db)
            .await
            .unwrap()
    }

    pub async fn insert(
        db: &sqlx::MySqlPool,
        title: &str,
        content: &str,
        tags: &str,
    ) -> Result<Self, sqlx::Error> {
        info!("inserting article: {}", title);
        let mut tx = db.begin().await?;

        // insert into the articles table
        sqlx::query(
            "INSERT INTO articles (title, content, tags, created_at, updated_at) VALUES (?, ?, ?, NOW(), NOW())",
        )
        .bind(title)
        .bind(content)
        .bind(tags)
        .execute(&mut *tx)
        .await?;
        // get the last inserted id
        let id = sqlx::query_scalar::<_, u64>("SELECT LAST_INSERT_ID()")
            .fetch_one(&mut *tx)
            .await? as i32;
        info!("inserted article {} with id {}", title, id);
        // insert into the tags table
        Article::insert_tags(&mut tx, tags, id).await?;
        info!("inserted tags: {}", tags);

        tx.commit().await?;

        Ok(Self::get_by_id(db, id).await.unwrap())
    }

    pub async fn update(
        db: &sqlx::MySqlPool,
        id: i32,
        title: &str,
        content: &str,
        tags: &str,
    ) -> Result<(), sqlx::Error> {
        info!("updating article: {}", id);
        let mut tx = db.begin().await?;

        // update the articles table
        sqlx::query(
            "UPDATE articles SET title = ?, content = ?, tags = ?, updated_at = NOW() WHERE id = ?",
        )
        .bind(title)
        .bind(content)
        .bind(tags)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        info!("updated article {} with id {}", title, id);
        // update the tags table
        Self::clear_tags(&mut tx, id).await?;
        info!("cleared tags for article {}", id);
        Self::insert_tags(&mut tx, tags, id).await?;
        info!("inserted tags {} for article {}", tags, id);

        tx.commit().await
    }

    async fn clear_tags(
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
        article_id: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM tags WHERE article_id = ?")
            .bind(article_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    async fn insert_tags(
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
        tags: &str,
        article_id: i32,
    ) -> Result<(), sqlx::Error> {
        for tag in tags.split(',').map(|s| s.trim()) {
            sqlx::query("INSERT INTO tags (name, article_id) VALUES (?, ?)")
                .bind(tag)
                .bind(article_id)
                .execute(&mut **tx)
                .await?;
        }
        Ok(())
    }

    pub async fn delete(db: &sqlx::MySqlPool, id: i32) -> Result<(), sqlx::Error> {
        info!("deleting article: {}", id);
        let mut tx = db.begin().await?;

        // delete the article
        sqlx::query("DELETE FROM articles WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        info!("deleted article: {}", id);
        // delete the tags
        Self::clear_tags(&mut tx, id).await?;
        info!("cleared tags for article {}", id);

        tx.commit().await
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
            .unwrap()
    }
}
