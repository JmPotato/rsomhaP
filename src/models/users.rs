use serde::Serialize;
use sqlx::prelude::FromRow;

use crate::Error;

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct User {
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
}

impl User {
    pub async fn get_by_username(db: &sqlx::MySqlPool, username: &str) -> Option<Self> {
        sqlx::query_as("SELECT username, password FROM users WHERE username = ?")
            .bind(username)
            .fetch_one(db)
            .await
            .ok()
    }

    pub async fn modify_password(
        db: &sqlx::MySqlPool,
        username: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), Error> {
        sqlx::query("UPDATE users SET password = ? WHERE username = ? AND password = ?")
            .bind(new_password)
            .bind(username)
            .bind(old_password)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn insert(db: &sqlx::MySqlPool, username: &str, password: &str) -> Result<(), Error> {
        // Use INSERT IGNORE to atomically skip if the username already exists (UNIQUE constraint).
        sqlx::query("INSERT IGNORE INTO users (username, password) VALUES (?, ?)")
            .bind(username)
            .bind(password)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn try_check_initialization(db: &sqlx::MySqlPool) -> Result<(), Error> {
        sqlx::query("SELECT username, password FROM users LIMIT 1")
            .fetch_one(db)
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_serialize_excludes_password() {
        let user = User {
            username: "admin".to_string(),
            password: "argon2_hash_secret".to_string(),
        };
        let json = serde_json::to_string(&user).unwrap();
        assert!(json.contains("admin"), "username should be present");
        assert!(
            !json.contains("argon2_hash_secret"),
            "password hash must not appear in serialized output"
        );
        assert!(
            !json.contains("password"),
            "password field key must not appear in serialized output"
        );
    }

    /// Returns a MySQL pool if TEST_DATABASE_URL is set, otherwise None (test skipped).
    async fn get_test_pool() -> Option<sqlx::MySqlPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Some(sqlx::MySqlPool::connect(&url).await.unwrap())
    }

    // NOTE: DB tests share the `users` table.
    // Run with: TEST_DATABASE_URL="mysql://..." cargo test -- --test-threads=1

    #[tokio::test]
    async fn test_insert_ignore_skips_duplicate() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        // Ensure clean state.
        sqlx::query("DELETE FROM users WHERE username = 'testuser'")
            .execute(&pool)
            .await
            .unwrap();

        // First insert should succeed.
        User::insert(&pool, "testuser", "hash_v1").await.unwrap();
        let user = User::get_by_username(&pool, "testuser").await.unwrap();
        assert_eq!(user.password, "hash_v1");

        // Second insert with same username should be silently ignored (not error, not overwrite).
        User::insert(&pool, "testuser", "hash_v2").await.unwrap();
        let user = User::get_by_username(&pool, "testuser").await.unwrap();
        assert_eq!(
            user.password, "hash_v1",
            "INSERT IGNORE should not overwrite existing row"
        );

        // Cleanup.
        sqlx::query("DELETE FROM users WHERE username = 'testuser'")
            .execute(&pool)
            .await
            .unwrap();
    }
}
