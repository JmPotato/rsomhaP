use serde::Serialize;
use sqlx::prelude::FromRow;

use crate::models::DbPool;
use crate::Error;

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct User {
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
}

const SELECT_ALL_USERS_SQL: &str = "SELECT username, password FROM users LIMIT 1";

impl User {
    pub async fn get_by_username(db: &DbPool, username: &str) -> Option<Self> {
        match db {
            DbPool::MySql(pool) => {
                sqlx::query_as("SELECT username, password FROM users WHERE username = ?")
                    .bind(username)
                    .fetch_one(pool)
                    .await
                    .ok()
            }
            DbPool::Postgres(pool) => {
                sqlx::query_as("SELECT username, password FROM users WHERE username = $1")
                    .bind(username)
                    .fetch_one(pool)
                    .await
                    .ok()
            }
        }
    }

    pub async fn modify_password(
        db: &DbPool,
        username: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), Error> {
        match db {
            DbPool::MySql(pool) => {
                sqlx::query(
                    "UPDATE users SET password = ?, updated_at = NOW() WHERE username = ? AND password = ?",
                )
                .bind(new_password)
                .bind(username)
                .bind(old_password)
                .execute(pool)
                .await?;
            }
            DbPool::Postgres(pool) => {
                sqlx::query(
                    "UPDATE users SET password = $1, updated_at = NOW() WHERE username = $2 AND password = $3",
                )
                .bind(new_password)
                .bind(username)
                .bind(old_password)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Atomically insert an admin user, or no-op if the username already exists.
    ///
    /// This is load-bearing for the admin bootstrap in `AppState::new`:
    /// `password_auth::generate_hash` uses a random salt, so every boot produces
    /// a different hash. If this ever overwrote an existing row, the admin's
    /// manually changed password would be wiped on restart. Both backends use
    /// native "conflict → do nothing" semantics that preserve the existing row.
    pub async fn insert(db: &DbPool, username: &str, password: &str) -> Result<(), Error> {
        match db {
            DbPool::MySql(pool) => {
                sqlx::query("INSERT IGNORE INTO users (username, password) VALUES (?, ?)")
                    .bind(username)
                    .bind(password)
                    .execute(pool)
                    .await?;
            }
            DbPool::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO users (username, password) VALUES ($1, $2) ON CONFLICT (username) DO NOTHING",
                )
                .bind(username)
                .bind(password)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    pub async fn try_check_initialization(db: &DbPool) -> Result<(), Error> {
        match db {
            DbPool::MySql(pool) => sqlx::query(SELECT_ALL_USERS_SQL)
                .fetch_one(pool)
                .await
                .map_err(|e| e.into())
                .map(|_| ()),
            DbPool::Postgres(pool) => sqlx::query(SELECT_ALL_USERS_SQL)
                .fetch_one(pool)
                .await
                .map_err(|e| e.into())
                .map(|_| ()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::init_schema;
    use chrono::{DateTime, Utc};
    use std::time::Duration;

    // ---------- pure-logic tests (no DB) ----------

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

    // ---------- test helpers (DB-backed) ----------

    async fn get_test_pool() -> Option<DbPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        Some(DbPool::connect(&url).await.unwrap())
    }

    /// Clear every row in the users table. Tests share this table so each
    /// DB-backed test truncates before setting up its own fixtures.
    async fn truncate_users(db: &DbPool) {
        match db {
            DbPool::MySql(pool) => {
                sqlx::query("DELETE FROM users")
                    .execute(pool)
                    .await
                    .unwrap();
            }
            DbPool::Postgres(pool) => {
                sqlx::query("DELETE FROM users")
                    .execute(pool)
                    .await
                    .unwrap();
            }
        }
    }

    async fn setup(db: &DbPool) {
        init_schema(db).await.unwrap();
        truncate_users(db).await;
    }

    /// Read `users.updated_at` directly. `User` struct does not expose the
    /// timestamp fields, so we query via raw SQL for the bump-on-update test.
    async fn fetch_user_updated_at(db: &DbPool, username: &str) -> DateTime<Utc> {
        match db {
            DbPool::MySql(pool) => sqlx::query_scalar::<_, DateTime<Utc>>(
                "SELECT updated_at FROM users WHERE username = ?",
            )
            .bind(username)
            .fetch_one(pool)
            .await
            .unwrap(),
            DbPool::Postgres(pool) => sqlx::query_scalar::<_, DateTime<Utc>>(
                "SELECT updated_at FROM users WHERE username = $1",
            )
            .bind(username)
            .fetch_one(pool)
            .await
            .unwrap(),
        }
    }

    // NOTE: DB tests share the `users` table and must run serially.
    // Run with: TEST_DATABASE_URL="mysql://..."    cargo test -- --test-threads=1
    // or        TEST_DATABASE_URL="postgres://..." cargo test -- --test-threads=1

    // ---------- insert path ----------

    #[tokio::test]
    async fn test_insert_creates_row_retrievable_by_username() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        User::insert(&pool, "alice", "hash_alice").await.unwrap();
        let user = User::get_by_username(&pool, "alice").await.unwrap();
        assert_eq!(user.username, "alice");
        assert_eq!(user.password, "hash_alice");
    }

    #[tokio::test]
    async fn test_insert_skips_duplicate() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        // First insert should succeed.
        User::insert(&pool, "testuser", "hash_v1").await.unwrap();
        let user = User::get_by_username(&pool, "testuser").await.unwrap();
        assert_eq!(user.password, "hash_v1");

        // Second insert with the same username should silently skip (not error,
        // not overwrite). This covers both MySQL's INSERT IGNORE and Postgres's
        // ON CONFLICT (username) DO NOTHING. The invariant is load-bearing
        // for admin bootstrap: `password_auth::generate_hash` is non-deterministic,
        // so every boot would wipe a manually changed admin password if we
        // ever overwrote on conflict.
        User::insert(&pool, "testuser", "hash_v2").await.unwrap();
        let user = User::get_by_username(&pool, "testuser").await.unwrap();
        assert_eq!(
            user.password, "hash_v1",
            "conflict should not overwrite existing row"
        );
    }

    // ---------- read path ----------

    #[tokio::test]
    async fn test_get_by_username_returns_none_when_missing() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        assert!(User::get_by_username(&pool, "ghost").await.is_none());
    }

    #[tokio::test]
    async fn test_try_check_initialization_returns_err_when_empty() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;
        // After truncate there is no user row, so the health check must fail.
        // This guards the `/ping` contract: it couples liveness to the
        // presence of at least the bootstrapped admin row.
        assert!(User::try_check_initialization(&pool).await.is_err());
    }

    #[tokio::test]
    async fn test_try_check_initialization_returns_ok_when_users_exist() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        User::insert(&pool, "admin", "hash").await.unwrap();
        assert!(User::try_check_initialization(&pool).await.is_ok());
    }

    // ---------- modify_password ----------

    #[tokio::test]
    async fn test_modify_password_success_changes_hash() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        User::insert(&pool, "alice", "old_hash").await.unwrap();
        User::modify_password(&pool, "alice", "old_hash", "new_hash")
            .await
            .unwrap();

        let user = User::get_by_username(&pool, "alice").await.unwrap();
        assert_eq!(user.password, "new_hash");
    }

    #[tokio::test]
    async fn test_modify_password_with_wrong_old_password_is_noop() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        User::insert(&pool, "alice", "real_hash").await.unwrap();
        // Wrong old-password: the UPDATE WHERE clause matches zero rows.
        // sqlx treats "0 rows affected" as success, so the call returns Ok
        // but the row is unchanged.
        User::modify_password(&pool, "alice", "wrong_old", "new_hash")
            .await
            .unwrap();

        let user = User::get_by_username(&pool, "alice").await.unwrap();
        assert_eq!(
            user.password, "real_hash",
            "password must not change when old_password is wrong"
        );
    }

    #[tokio::test]
    async fn test_modify_password_bumps_updated_at() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        setup(&pool).await;

        User::insert(&pool, "alice", "old_hash").await.unwrap();
        let before = fetch_user_updated_at(&pool, "alice").await;

        // MySQL DATETIME has second precision; cross a full second boundary.
        tokio::time::sleep(Duration::from_millis(1100)).await;

        User::modify_password(&pool, "alice", "old_hash", "new_hash")
            .await
            .unwrap();

        let after = fetch_user_updated_at(&pool, "alice").await;
        assert!(
            after > before,
            "modify_password must bump updated_at explicitly (SET updated_at = NOW()); \
             was {before}, now {after}"
        );
    }
}
