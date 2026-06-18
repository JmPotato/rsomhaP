mod articles;
mod mysql_schema;
mod pages;
mod postgres_schema;
mod users;

pub(crate) use articles::*;
pub(crate) use pages::*;
pub(crate) use users::*;

use std::time::Duration;

use tracing::info;

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
    ///
    /// The pool is tuned for Neon serverless billing (CU-hrs):
    /// - `min_connections(0)` so no connection is held idle while idle, letting
    ///   the Neon compute scale to zero between bursts of traffic.
    /// - A generous `acquire_timeout` to absorb the few seconds Neon takes to
    ///   wake a suspended compute.
    /// - A short `idle_timeout` so idle connections are returned (and the
    ///   compute can go idle) rather than pinned open.
    /// - A small `max_connections`: a single-admin blog has low concurrency,
    ///   and against the Neon pooled endpoint PgBouncer already multiplexes,
    ///   so the app-side pool does not need many connections.
    pub async fn connect(url: &str) -> Result<Self, Error> {
        if url.starts_with("mysql://") {
            info!("connecting to MySQL (non-pooler defaults)");
            let pool = sqlx::mysql::MySqlPoolOptions::new()
                .min_connections(0)
                .max_connections(max_connections_for(url))
                .acquire_timeout(Duration::from_secs(15))
                .idle_timeout(Some(Duration::from_secs(60)))
                .connect(url)
                .await?;
            Ok(Self::MySql(pool))
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            if is_neon_pooler_url(url) {
                info!("connecting to Neon via the pooled endpoint (-pooler.neon.tech)");
            } else {
                info!("connecting to PostgreSQL (direct endpoint)");
            }
            let pool = sqlx::postgres::PgPoolOptions::new()
                .min_connections(0)
                .max_connections(max_connections_for(url))
                .acquire_timeout(Duration::from_secs(15))
                .idle_timeout(Some(Duration::from_secs(60)))
                .connect(url)
                .await?;
            Ok(Self::Postgres(pool))
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

/// Detect a Neon pooled endpoint. Neon's `-pooler.neon.tech` host runs
/// PgBouncer in transaction mode, which already multiplexes connections, so
/// the app-side pool can stay small. Direct (`ep-*.neon.tech`) and non-Neon
/// hosts get a slightly larger app-side pool.
fn is_neon_pooler_url(url: &str) -> bool {
    url.contains("-pooler.") && url.contains(".neon.tech")
}

/// Pick an app-side `max_connections` for the configured backend. The Neon
/// pooled endpoint already multiplexes, so the app does not need many
/// connections; everything else gets a modest pool sized for a single-admin
/// blog's low concurrency.
fn max_connections_for(url: &str) -> u32 {
    if is_neon_pooler_url(url) {
        3
    } else {
        5
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

/// Initialize the schema only if it looks like a fresh database. On every boot
/// against an already-provisioned database this runs a single existence probe
/// (`SELECT COUNT(*) ...`) instead of the full set of `CREATE TABLE IF NOT
/// EXISTS` / `CREATE INDEX IF NOT EXISTS` statements, cutting cold-start DB
/// round-trips (and Neon CU-hrs) from eight down to one.
///
/// The probe checks *every* table in `SCHEMA_TABLES`, not just one: if any
/// table is missing (e.g. someone dropped `tags` but left `articles`), the
/// count comes up short and the full idempotent `init_schema` runs to
/// self-heal it — preserving the original boot behavior rather than silently
/// leaving a broken schema. Indexes are not probed: a missing index degrades
/// performance but does not break correctness, and `init_schema`'s
/// `CREATE INDEX IF NOT EXISTS` only runs on the full-DDL path.
pub async fn init_schema_if_needed(db: &DbPool) -> Result<(), Error> {
    if all_schema_tables_exist(db).await? {
        return Ok(());
    }
    init_schema(db).await
}

/// The tables `init_schema` provisions. `init_schema_if_needed` uses this set
/// to detect an already-provisioned schema. If a table is ever added to the
/// schema files, add it here *and* to the `IN (...)` lists in
/// `all_schema_tables_exist` so the probe stays in sync.
const SCHEMA_TABLES: [&str; 4] = ["articles", "tags", "pages", "users"];

/// Whether every table in `SCHEMA_TABLES` exists in the current
/// database/schema, in a single `COUNT(*)` round-trip. The `IN (...)` lists
/// below enumerate the same tables as `SCHEMA_TABLES`; keep them in sync.
async fn all_schema_tables_exist(db: &DbPool) -> Result<bool, Error> {
    let expected: i64 = SCHEMA_TABLES.len() as i64;
    match db {
        DbPool::MySql(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_schema = DATABASE() \
                 AND table_name IN ('articles','tags','pages','users')",
            )
            .fetch_one(pool)
            .await?;
            Ok(count == expected)
        }
        DbPool::Postgres(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pg_tables \
                 WHERE schemaname = current_schema() \
                 AND tablename IN ('articles','tags','pages','users')",
            )
            .fetch_one(pool)
            .await?;
            Ok(count == expected)
        }
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

    #[tokio::test]
    async fn test_all_schema_tables_exist_returns_true_after_init() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        init_schema(&pool).await.unwrap();
        assert!(
            all_schema_tables_exist(&pool).await.unwrap(),
            "all schema tables should exist after init_schema"
        );
    }

    /// `init_schema_if_needed` must self-heal a partially-dropped schema: if a
    /// single table is missing (here `tags`) the probe must report not-all-present
    /// so the full idempotent DDL runs and recreates it. This is the regression
    /// the all-tables probe exists to prevent — a single-table sentinel would
    /// have skipped the DDL and left `tags` missing.
    #[tokio::test]
    async fn test_init_schema_if_needed_self_heals_partial_drop() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        init_schema(&pool).await.unwrap();
        clear_all_tables(&pool).await;

        // Simulate an out-of-band partial drop of just the `tags` table. `tags`
        // has no foreign keys in either schema, so this won't cascade. Each arm
        // is a block so the `match` evaluates to `()` (the backend-specific
        // query result types do not unify across arms).
        match &pool {
            DbPool::MySql(p) => {
                sqlx::query("DROP TABLE IF EXISTS tags")
                    .execute(p)
                    .await
                    .unwrap();
            }
            DbPool::Postgres(p) => {
                sqlx::query("DROP TABLE IF EXISTS tags")
                    .execute(p)
                    .await
                    .unwrap();
            }
        }

        // The probe must now come up short (4 expected, 3 found).
        assert!(
            !all_schema_tables_exist(&pool).await.unwrap(),
            "probe must detect the missing tags table"
        );

        // init_schema_if_needed should run the full DDL and recreate it.
        init_schema_if_needed(&pool).await.unwrap();
        assert!(
            all_schema_tables_exist(&pool).await.unwrap(),
            "init_schema_if_needed should have recreated the dropped table"
        );
    }

    /// `init_schema_if_needed` must skip the DDL when the schema already
    /// exists and must not destroy existing rows. Guards the cold-start
    /// optimization (one existence probe instead of eight DDL round-trips).
    #[tokio::test]
    async fn test_init_schema_if_needed_skips_without_destroying_data() {
        let Some(pool) = get_test_pool().await else {
            return;
        };
        init_schema(&pool).await.unwrap();
        clear_all_tables(&pool).await;

        User::insert(&pool, "persistent-user", "persistent-hash")
            .await
            .unwrap();

        // Tables already exist → this should be a probe-only no-op.
        init_schema_if_needed(&pool).await.unwrap();

        let after = User::get_by_username(&pool, "persistent-user")
            .await
            .expect("fixture user should survive init_schema_if_needed");
        assert_eq!(after.username, "persistent-user");
        assert_eq!(after.password, "persistent-hash");
    }

    #[test]
    fn test_is_neon_pooler_url_detects_pooler_hosts() {
        assert!(is_neon_pooler_url(
            "postgresql://user:pass@ep-example-pooler.ap-southeast-1.aws.neon.tech/db"
        ));
        // Direct Neon endpoint is not the pooler.
        assert!(!is_neon_pooler_url(
            "postgresql://user:pass@ep-example.ap-southeast-1.aws.neon.tech/db"
        ));
        // Non-Neon hosts are never the pooler.
        assert!(!is_neon_pooler_url("postgresql://user:pass@example.com/db"));
        assert!(!is_neon_pooler_url("mysql://user:pass@127.0.0.1:3306/db"));
    }

    #[test]
    fn test_max_connections_for_pooler_is_smaller() {
        assert_eq!(
            max_connections_for("postgresql://u:p@ep-x-pooler.aws.neon.tech/db"),
            3
        );
        assert_eq!(
            max_connections_for("postgresql://u:p@ep-x.aws.neon.tech/db"),
            5
        );
        assert_eq!(max_connections_for("mysql://u:p@127.0.0.1:3306/db"), 5);
    }
}
