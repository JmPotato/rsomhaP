mod articles;
mod mysql_schema;
mod pages;
mod postgres_schema;
mod users;

pub(crate) use articles::*;
pub(crate) use pages::*;
pub(crate) use users::*;

use crate::Error;

/// A database pool that dispatches to either MySQL or PostgreSQL based on the
/// connection URL scheme. All model methods take `&DbPool` and internally
/// `match` on the variant to pick the right backend implementation.
#[derive(Clone)]
pub enum DbPool {
    MySql(sqlx::MySqlPool),
    Postgres(sqlx::PgPool),
}

impl DbPool {
    /// Connect using a URL whose scheme selects the backend.
    ///
    /// - `mysql://...` → MySQL
    /// - `postgres://...` or `postgresql://...` → PostgreSQL
    pub async fn connect(url: &str) -> Result<Self, Error> {
        if url.starts_with("mysql://") {
            Ok(Self::MySql(sqlx::MySqlPool::connect(url).await?))
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Ok(Self::Postgres(sqlx::PgPool::connect(url).await?))
        } else {
            Err(Error::InvalidDatabaseConfig)
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            Self::MySql(p) => p.is_closed(),
            Self::Postgres(p) => p.is_closed(),
        }
    }
}

/// Initialize the target schema for the current backend. Idempotent: all DDL
/// uses `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`, so it is
/// safe to call on every boot against both fresh and existing databases.
pub async fn init_schema(db: &DbPool) -> Result<(), Error> {
    match db {
        DbPool::MySql(pool) => mysql_schema::init_schema(pool).await,
        DbPool::Postgres(pool) => postgres_schema::init_schema(pool).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::User;

    async fn get_test_pool() -> Option<DbPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Some(DbPool::connect(&url).await.unwrap())
    }

    async fn clear_all_tables(db: &DbPool) {
        match db {
            DbPool::MySql(pool) => {
                sqlx::query("DELETE FROM tags").execute(pool).await.unwrap();
                sqlx::query("DELETE FROM articles")
                    .execute(pool)
                    .await
                    .unwrap();
                sqlx::query("DELETE FROM pages")
                    .execute(pool)
                    .await
                    .unwrap();
                sqlx::query("DELETE FROM users")
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
                sqlx::query("DELETE FROM pages")
                    .execute(pool)
                    .await
                    .unwrap();
                sqlx::query("DELETE FROM users")
                    .execute(pool)
                    .await
                    .unwrap();
            }
        }
    }

    /// Calling `init_schema` multiple times must be a no-op after the first
    /// successful run, and must not destroy already-written data. Runs against
    /// whichever backend `TEST_DATABASE_URL` points at.
    #[tokio::test]
    async fn test_init_schema_idempotent() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        init_schema(&pool).await.unwrap();
        clear_all_tables(&pool).await;

        User::insert(&pool, "persistent-user", "persistent-hash")
            .await
            .unwrap();
        let before = User::get_by_username(&pool, "persistent-user")
            .await
            .expect("fixture user should exist before repeated init_schema");

        init_schema(&pool).await.unwrap();
        init_schema(&pool).await.unwrap();

        let after = User::get_by_username(&pool, "persistent-user")
            .await
            .expect("fixture user should survive repeated init_schema");
        assert_eq!(before.username, after.username);
        assert_eq!(before.password, after.password);
    }
}
