mod articles;
mod pages;
mod users;

pub(crate) use articles::*;
pub(crate) use pages::*;
pub(crate) use users::*;

use crate::Error;

const CREATE_TABLE_ARTICLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS articles (
    id INT AUTO_INCREMENT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags VARCHAR(255) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) CHARSET = utf8mb4;
"#;

const CREATE_TABLE_TAGS_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS tags (
    id INT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    article_id INT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX(name)
) CHARSET = utf8mb4;
"#;

const CREATE_TABLE_PAGES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS pages (
    id INT AUTO_INCREMENT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) CHARSET = utf8mb4;
"#;

const CREATE_TABLE_USERS_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id INT AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(255) NOT NULL,
    password VARCHAR(255) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) CHARSET = utf8mb4;
"#;

pub async fn create_tables_within_transaction(db: &sqlx::MySqlPool) -> Result<(), Error> {
    let mut tx = db.begin().await?;

    sqlx::query(CREATE_TABLE_ARTICLES_SQL)
        .execute(&mut *tx)
        .await?;
    sqlx::query(CREATE_TABLE_TAGS_SQL).execute(&mut *tx).await?;
    sqlx::query(CREATE_TABLE_PAGES_SQL)
        .execute(&mut *tx)
        .await?;
    sqlx::query(CREATE_TABLE_USERS_SQL)
        .execute(&mut *tx)
        .await?;

    tx.commit().await.map_err(|e| e.into())
}
