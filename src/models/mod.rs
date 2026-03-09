mod articles;
mod pages;
mod users;

pub(crate) use articles::*;
pub(crate) use pages::*;
pub(crate) use users::*;

use tracing::info;

use crate::Error;

const CREATE_TABLE_ARTICLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS articles (
    id INT AUTO_INCREMENT PRIMARY KEY,
    slug VARCHAR(255) NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags VARCHAR(255) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX(slug)
) CHARSET = utf8mb4;
"#;

const CREATE_TABLE_TAGS_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS tags (
    id INT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    article_id INT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX(name),
    INDEX(article_id)
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
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    UNIQUE INDEX idx_username (username)
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

    tx.commit().await?;

    // Run migrations for existing tables that may lack newer schema changes.
    run_migrations(db).await?;

    Ok(())
}

/// Apply incremental schema migrations for already-existing tables.
/// Each migration is idempotent: errors from "already exists" are silently ignored.
pub(crate) async fn run_migrations(db: &sqlx::MySqlPool) -> Result<(), Error> {
    // Migration: add slug column to articles for tables created before this column existed.
    match sqlx::query(
        "ALTER TABLE articles ADD COLUMN slug VARCHAR(255) NOT NULL DEFAULT '' AFTER id",
    )
    .execute(db)
    .await
    {
        Ok(_) => info!("migration: added slug column to articles"),
        Err(sqlx::Error::Database(e))
            if e.downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                .number()
                == 1060 =>
        {
            // MySQL error 1060: Duplicate column name - column already exists.
            info!("migration: slug column already exists in articles, skipping");
        }
        Err(e) => return Err(e.into()),
    }

    // Migration: add index on articles.slug for tables created before this index existed.
    match sqlx::query("ALTER TABLE articles ADD INDEX idx_slug (slug)")
        .execute(db)
        .await
    {
        Ok(_) => info!("migration: added index on articles.slug"),
        Err(sqlx::Error::Database(e))
            if e.downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                .number()
                == 1061 =>
        {
            // MySQL error 1061: Duplicate key name - index already exists.
            info!("migration: index on articles.slug already exists, skipping");
        }
        Err(e) => return Err(e.into()),
    }

    // Migration: add UNIQUE INDEX on users.username for tables created before this constraint.
    match sqlx::query("ALTER TABLE users ADD UNIQUE INDEX idx_username (username)")
        .execute(db)
        .await
    {
        Ok(_) => info!("migration: added unique index on users.username"),
        Err(sqlx::Error::Database(e))
            if e.downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                .number()
                == 1061 =>
        {
            // MySQL error 1061: Duplicate key name - index already exists.
            info!("migration: unique index on users.username already exists, skipping");
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Old schema WITHOUT the UNIQUE INDEX on username, simulating a pre-migration table.
    const OLD_USERS_TABLE_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS users (
        id INT AUTO_INCREMENT PRIMARY KEY,
        username VARCHAR(255) NOT NULL,
        password VARCHAR(255) NOT NULL,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
    ) CHARSET = utf8mb4;
    "#;

    /// Old articles schema WITHOUT the slug column, simulating a pre-migration table.
    const OLD_ARTICLES_TABLE_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS articles (
        id INT AUTO_INCREMENT PRIMARY KEY,
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        tags VARCHAR(255) NOT NULL,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
    ) CHARSET = utf8mb4;
    "#;

    /// Returns a MySQL pool if TEST_DATABASE_URL is set, otherwise None (test skipped).
    async fn get_test_pool() -> Option<sqlx::MySqlPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Some(sqlx::MySqlPool::connect(&url).await.unwrap())
    }

    /// Drops and recreates the users table with the given schema SQL.
    async fn reset_users_table(db: &sqlx::MySqlPool, create_sql: &str) {
        sqlx::query("DROP TABLE IF EXISTS users")
            .execute(db)
            .await
            .unwrap();
        sqlx::query(create_sql).execute(db).await.unwrap();
    }

    /// Drops and recreates the articles (and tags) table with the given schema SQL.
    async fn reset_articles_table(db: &sqlx::MySqlPool, create_sql: &str) {
        sqlx::query("DROP TABLE IF EXISTS tags, articles")
            .execute(db)
            .await
            .unwrap();
        sqlx::query(create_sql).execute(db).await.unwrap();
        sqlx::query(CREATE_TABLE_TAGS_SQL)
            .execute(db)
            .await
            .unwrap();
    }

    /// Checks if a UNIQUE INDEX named `idx_username` exists on the users table.
    async fn has_unique_index(db: &sqlx::MySqlPool) -> bool {
        // SHOW INDEX returns one row per index; Non_unique=0 means unique.
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM information_schema.STATISTICS \
             WHERE TABLE_SCHEMA = DATABASE() \
               AND TABLE_NAME = 'users' \
               AND INDEX_NAME = 'idx_username' \
               AND NON_UNIQUE = 0",
        )
        .fetch_optional(db)
        .await
        .unwrap();
        matches!(row, Some((count,)) if count > 0)
    }

    /// Checks if a column exists on the given table.
    async fn has_column(db: &sqlx::MySqlPool, table: &str, column: &str) -> bool {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() \
               AND TABLE_NAME = ? \
               AND COLUMN_NAME = ?",
        )
        .bind(table)
        .bind(column)
        .fetch_optional(db)
        .await
        .unwrap();
        matches!(row, Some((count,)) if count > 0)
    }

    /// Checks if an index exists on the given table.
    async fn has_index(db: &sqlx::MySqlPool, table: &str, index_name: &str) -> bool {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM information_schema.STATISTICS \
             WHERE TABLE_SCHEMA = DATABASE() \
               AND TABLE_NAME = ? \
               AND INDEX_NAME = ?",
        )
        .bind(table)
        .bind(index_name)
        .fetch_optional(db)
        .await
        .unwrap();
        matches!(row, Some((count,)) if count > 0)
    }

    // NOTE: These tests share tables and must not run in parallel.
    // Run with: TEST_DATABASE_URL="mysql://..." cargo test -- --test-threads=1

    #[tokio::test]
    async fn test_migration_adds_unique_index_to_old_table() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        // Simulate an old environment: table exists but has no unique index.
        reset_users_table(&pool, OLD_USERS_TABLE_SQL).await;
        assert!(!has_unique_index(&pool).await);

        // Migration should add the index.
        run_migrations(&pool).await.unwrap();
        assert!(has_unique_index(&pool).await);

        // Verify the constraint works: duplicate username should be rejected.
        sqlx::query("INSERT INTO users (username, password) VALUES ('admin', 'hash1')")
            .execute(&pool)
            .await
            .unwrap();
        let dup = sqlx::query("INSERT INTO users (username, password) VALUES ('admin', 'hash2')")
            .execute(&pool)
            .await;
        assert!(dup.is_err(), "duplicate username should be rejected");

        reset_users_table(&pool, OLD_USERS_TABLE_SQL).await;
    }

    #[tokio::test]
    async fn test_migration_idempotent() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        // New schema already has the unique index.
        reset_users_table(&pool, CREATE_TABLE_USERS_SQL).await;
        assert!(has_unique_index(&pool).await);

        // Running migration multiple times should always succeed (error 1061 silently skipped).
        run_migrations(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();

        reset_users_table(&pool, OLD_USERS_TABLE_SQL).await;
    }

    #[tokio::test]
    async fn test_migration_fails_with_duplicate_data() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        // Old schema, no unique index.
        reset_users_table(&pool, OLD_USERS_TABLE_SQL).await;

        // Insert duplicate usernames (possible without unique index).
        sqlx::query("INSERT INTO users (username, password) VALUES ('admin', 'hash1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (username, password) VALUES ('admin', 'hash2')")
            .execute(&pool)
            .await
            .unwrap();

        // Migration should fail: ALTER TABLE ADD UNIQUE INDEX fails with error 1062
        // (Duplicate entry) because duplicate data exists. This error is NOT caught
        // by our 1061 handler, so it propagates correctly.
        let result = run_migrations(&pool).await;
        assert!(result.is_err(), "should fail when duplicate usernames exist");

        reset_users_table(&pool, OLD_USERS_TABLE_SQL).await;
    }

    // --- articles.slug migration ---

    #[tokio::test]
    async fn test_migration_adds_slug_column_to_old_articles() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        // Simulate an old environment: articles table exists but has no slug column.
        reset_articles_table(&pool, OLD_ARTICLES_TABLE_SQL).await;
        assert!(!has_column(&pool, "articles", "slug").await);
        assert!(!has_index(&pool, "articles", "idx_slug").await);

        // Migration should add the column and index.
        run_migrations(&pool).await.unwrap();
        assert!(has_column(&pool, "articles", "slug").await);
        assert!(has_index(&pool, "articles", "idx_slug").await);

        // Verify that inserting an article with slug now works.
        sqlx::query(
            "INSERT INTO articles (slug, title, content, tags) VALUES ('test-slug', 'Title', 'Content', 'tag')",
        )
        .execute(&pool)
        .await
        .expect("insert with slug should succeed after migration");

        // Verify that inserting without slug uses the default empty string.
        sqlx::query("INSERT INTO articles (title, content, tags) VALUES ('Title2', 'Content2', 'tag2')")
            .execute(&pool)
            .await
            .expect("insert without slug should succeed (DEFAULT '')");

        reset_articles_table(&pool, OLD_ARTICLES_TABLE_SQL).await;
    }

    #[tokio::test]
    async fn test_migration_slug_idempotent() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        // New schema already has the slug column.
        reset_articles_table(&pool, CREATE_TABLE_ARTICLES_SQL).await;
        assert!(has_column(&pool, "articles", "slug").await);

        // Running migration multiple times should always succeed.
        run_migrations(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();

        reset_articles_table(&pool, OLD_ARTICLES_TABLE_SQL).await;
    }

    // --- full initialization ---

    #[tokio::test]
    async fn test_create_tables_runs_migration() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        // Drop all tables to simulate a fresh deployment.
        sqlx::query("DROP TABLE IF EXISTS tags, articles, pages, users")
            .execute(&pool)
            .await
            .unwrap();

        // Full initialization path.
        create_tables_within_transaction(&pool).await.unwrap();

        // The unique index and slug column should exist after create_tables_within_transaction.
        assert!(has_unique_index(&pool).await);
        assert!(has_column(&pool, "articles", "slug").await);

        // Running again (simulating restart) should also succeed.
        create_tables_within_transaction(&pool).await.unwrap();
        assert!(has_unique_index(&pool).await);
        assert!(has_column(&pool, "articles", "slug").await);

        sqlx::query("DROP TABLE IF EXISTS tags, articles, pages, users")
            .execute(&pool)
            .await
            .unwrap();
    }
}
