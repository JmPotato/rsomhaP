use serde::Serialize;
use sqlx::prelude::FromRow;

use crate::Error;

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct User {
    pub username: String,
    pub password: String,
}

impl User {
    pub async fn get_by_username(db: &sqlx::MySqlPool, username: &str) -> Option<Self> {
        sqlx::query_as("SELECT * FROM users WHERE username = ?")
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
        // check if the username exists, if it does, do nothing.
        if Self::get_by_username(db, username).await.is_some() {
            return Ok(());
        }
        // insert the user
        sqlx::query("INSERT INTO users (username, password) VALUES (?, ?)")
            .bind(username)
            .bind(password)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn try_check_initialization(db: &sqlx::MySqlPool) -> Result<(), Error> {
        sqlx::query("SELECT * FROM users LIMIT 1")
            .fetch_one(db)
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }
}
