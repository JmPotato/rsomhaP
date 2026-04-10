use crate::Error;

const CREATE_ARTICLES: &str = r#"
CREATE TABLE IF NOT EXISTS articles (
    id INT AUTO_INCREMENT PRIMARY KEY,
    slug VARCHAR(255) NOT NULL DEFAULT '',
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags VARCHAR(255) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX idx_slug (slug)
) CHARSET = utf8mb4
"#;

const CREATE_TAGS: &str = r#"
CREATE TABLE IF NOT EXISTS tags (
    id INT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    article_id INT NOT NULL,
    INDEX idx_name (name),
    INDEX idx_article_id (article_id)
) CHARSET = utf8mb4
"#;

const CREATE_PAGES: &str = r#"
CREATE TABLE IF NOT EXISTS pages (
    id INT AUTO_INCREMENT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) CHARSET = utf8mb4
"#;

const CREATE_USERS: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id INT AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(255) NOT NULL,
    password VARCHAR(255) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    UNIQUE INDEX idx_username (username)
) CHARSET = utf8mb4
"#;

pub async fn init_schema(pool: &sqlx::MySqlPool) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    for sql in [CREATE_ARTICLES, CREATE_TAGS, CREATE_PAGES, CREATE_USERS] {
        sqlx::query(sql).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
