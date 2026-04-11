# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

rsomhaP is a monolithic, server-rendered blog engine written in Rust. It is a rewrite of the Python project [Pomash](https://github.com/JmPotato/Pomash) and ships as a single binary backed by either a MySQL- or PostgreSQL-compatible database. Split DB config under `[database]` selects the backend via `backend = "mysql"` / `"postgres"`, while full `connection_url` / `DATABASE_URL` values select the driver through their URL scheme.

## Commands

```sh
# Build / run (package name and binary name are both `rsomhap`)
cargo build
cargo run --release

# Quality gates (not CI-enforced, but expected before committing)
cargo fmt --check
cargo clippy -- -D warnings

# Tests
cargo test                                        # compile + pure-logic tests; DB tests early-return when TEST_DATABASE_URL is unset
TEST_DATABASE_URL="mysql://user:pass@host:port/db"    cargo test -- --test-threads=1
TEST_DATABASE_URL="postgres://user:pass@host:port/db" cargo test -- --test-threads=1
cargo test <test_name>                            # run a single test by name

# Docker
docker build -t rsomhap .
docker compose up                                 # reads DATABASE_URL / UMAMI_ID from the environment
```

**All DB-backed tests live in `src/models/*` and must run serially.** They share real tables (`articles`, `tags`, `pages`, `users`) and truncate data between cases, so parallel execution corrupts state. Every DB test starts with `let Some(pool) = get_test_pool().await else { return; };` (where `get_test_pool` returns `Option<DbPool>`), so a plain `cargo test` without `TEST_DATABASE_URL` is a compile + pure-logic check only — it is not a substitute for running with a real DB before merging schema or model changes. The DB tests are **backend-agnostic**: they run against whichever backend `TEST_DATABASE_URL` points at. Run them against both MySQL and PostgreSQL before merging any change that touches `src/models/*`. Tests outside `src/models/*` (`is_safe_redirect`, `build_message_url`, `sort_out_tags`, `User` serde) are pure.

**The binary must be launched from the project root.** `TEMPLATES_DIR`, `STATIC_DIR`, and `CONFIG_FILE_PATH` in `src/app.rs` are hard-coded as relative paths (`templates`, `static`, `config.toml`). The Dockerfile copies these into the working directory for the same reason.

## Architecture

### Request pipeline
`App::serve` in `src/app.rs` wires an Axum router in two tiers:

- **Public**: `/` (home, paginated via `/page/{num}`), `/article/{id_or_slug}`, `/articles`, `/tag/{tag}`, `/tags`, `/feed`, `/ping`, `/login`, `/logout`, and a catch-all `/{page}` that resolves a custom `Page` by title (case-insensitive, via `LOWER(title) = LOWER(?)`).
- **Admin**: nested under `/admin`, wrapped with `login_required!(AppState, login_url = "/login")` from `axum-login`. The macro redirects unauthenticated users to `/login?next=...`, and `handlers::is_safe_redirect` confines that `next` to relative paths only. The redirect-safety tests in `handlers.rs` exist specifically to lock this behavior — do not weaken or bypass them when touching the login flow.

Two slug footguns that any change to article routing needs to respect:
- **`handler_article` parses `{id_or_slug}` as `i32` first.** A numeric-looking slug like `"2024"` always resolves as article id 2024 and never reaches the slug lookup. Reject or prefix numeric slugs in the editor if this ever becomes a product concern.
- **`articles.slug` is `INDEX(slug)`, not `UNIQUE`.** `Article::get_by_slug` uses `fetch_one`, so if duplicate slugs ever exist the planner silently picks one. Enforce uniqueness via the schema files (see *Models, schema, and backend dispatch* below) before treating slugs as primary identifiers.

### Sessions and authentication
Sessions use `tower_sessions::MemoryStore` with a per-process `Key::generate()` and `with_secure(false)` (see `src/app.rs:215`). Three load-bearing consequences:

1. **Every restart invalidates all sessions** — both the store and the signing key are fresh on boot. There is no persistent session storage.
2. **`MemoryStore` does not scale horizontally.** `fly.toml`'s single-machine model (`min_machines_running = 0`, no replicas) is load-bearing for auth, not just a cost choice. Adding replicas requires swapping to a shared store (e.g. `tower-sessions-sqlx-store`) before the auth layer will work at all.
3. **The session cookie is not `Secure`-flagged.** Production relies entirely on Fly.io's `force_https = true` at the edge. Never deploy rsomhaP behind a non-HTTPS-terminating proxy without first setting `with_secure(true)` and re-testing login.

`src/auth.rs` implements `axum_login::AuthnBackend` for `AppState`. Password verification (argon2, via `password-auth`) runs inside `task::spawn_blocking`; that offload is load-bearing for request concurrency, so preserve it if you ever refactor the auth path.

### AppState and caches
`AppState` (in `src/app.rs`) is cloned into an `Arc` and serves both as Axum state and as the `axum_login::AuthnBackend`. It holds:

- `config: Config` — parsed once from `config.toml`, with env var overrides (`DATABASE_URL`, `PLAUSIBLE_DOMAIN`, `UMAMI_ID`) layered on top in `Config::load_env_vars`. When split DB fields are used, `[database].backend` chooses whether `Config::database_url()` builds a MySQL or PostgreSQL DSN; `DATABASE_URL` (or an in-file `connection_url`) overrides that and its own URL scheme picks the driver.
- `env: minijinja::Environment` — all templates loaded eagerly at startup from `templates/` via `add_template_owned`. **Template edits require a full restart**; the `minijinja` `loader` feature is enabled in `Cargo.toml` but is not wired up for hot reload.
- `db: models::DbPool` — an enum wrapping either `sqlx::MySqlPool` or `sqlx::PgPool`. Backend is picked at `DbPool::connect` time from the URL scheme.
- `feed_cache` + `page_titles_cache` — both `Arc<RwLock<...>>`.

**Cache invalidation is a correctness requirement.** Any handler that mutates an `Article` or `Page` must call both `state.refresh_feed_cache(...)` and `state.refresh_page_titles_cache()` (see `handler_edit_post` / `handler_delete_post` in `src/handlers.rs`). `page_titles_cache` is injected into every template render via `AppState::render_template` and drives the navbar — an uninvalidated stale cache ships wrong navigation on every page.

**`refresh_feed_cache(force: bool)` semantics are non-obvious but load-bearing.** `force=false` treats `MAX(articles.updated_at)` as a change detector and skips re-rendering when the timestamp hasn't advanced. Inserts and updates are safe with `force=false` because both bump `updated_at`. **Deletions must use `force=true`** — deleting the most-recently-updated row can leave `MAX(updated_at)` unchanged or regressed, which would silently serve a stale feed that still contains the deleted article. `handler_delete_post` passes `true` for exactly this reason; classify any new mutation path accordingly.

### Editable trait and generic admin handlers
`src/utils.rs` defines the `Editable` trait (`update` / `insert` / `delete` / `get_redirect_url`, all taking `&DbPool`) and a custom `Entity<T>` extractor that merges the path `id` with the form body. The same handler functions `handler_edit_post::<Article>` / `handler_edit_post::<Page>` (and their delete counterparts) are instantiated in `App::serve` for both content types. When adding a new CRUD entity, implement `Editable` and `From<EditorForm>` and reuse these handlers — they already perform cache invalidation and map `Error::PageTitleExists` to a user-visible redirect.

The `utils::Path<T>` wrapper around `axum::extract::Path` exists specifically to render the `error.html` template on path rejection instead of Axum's default `400`. Prefer it over the bare extractor in any new handler that takes a path parameter.

### Models, schema, and backend dispatch
`src/models/mod.rs` defines `DbPool`, an enum that wraps `sqlx::MySqlPool` or `sqlx::PgPool` and dispatches based on the final connection URL scheme (`mysql://` vs `postgres://` / `postgresql://`). For split config fields, `Config::database_url()` injects that scheme from `[database].backend`; for full `connection_url` / `DATABASE_URL` inputs, the supplied URL is used as-is. **Every model method takes `&DbPool` and `match`es once at the entry point**, then calls a backend-specific private helper that uses the concrete pool and the dialect-correct SQL. Transactions (`sqlx::Transaction<'_, sqlx::MySql>` vs `sqlx::Transaction<'_, sqlx::Postgres>`) live entirely inside those helpers; cross-backend transaction types never leak into trait signatures.

**Why this codebase does not use an ORM.** This repo intentionally keeps SQL explicit instead of introducing SeaORM/Diesel-style abstractions. The model surface is small, the dialect differences are narrow but load-bearing (`LAST_INSERT_ID()` vs `RETURNING`, `INSERT IGNORE` vs `ON CONFLICT`, placeholder syntax, timestamp semantics), and hand-written dispatch keeps those differences reviewable at the exact call site without extra dependencies or hidden query generation. Do not introduce an ORM as a convenience refactor unless the underlying requirements materially change.

Schema DDL is split across two files, one per backend, with no incremental migrations:

- `src/models/mysql_schema.rs` — MySQL-flavored `CREATE TABLE IF NOT EXISTS` (with `AUTO_INCREMENT`, `DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP`, `CHARSET = utf8mb4`, inline `INDEX` / `UNIQUE INDEX`).
- `src/models/postgres_schema.rs` — Postgres-flavored `CREATE TABLE IF NOT EXISTS` (with `SERIAL`, `TIMESTAMPTZ NOT NULL DEFAULT NOW()`, separate `CREATE INDEX IF NOT EXISTS` / `CREATE UNIQUE INDEX IF NOT EXISTS`). Postgres has **no** `ON UPDATE CURRENT_TIMESTAMP` equivalent — the target schema is identical in *columns*, but time bookkeeping diverges: see below.
- `models::init_schema(&DbPool)` dispatches into the right one and issues the DDL through a transaction handle. Both files are idempotent via `IF NOT EXISTS`, so it is safe to call on every boot. Do not treat that as a cross-backend atomic migration guarantee: MySQL can implicitly commit DDL, so partial progress is still possible if a later statement fails.

**There is no schema-version table, no `run_migrations`, no error-code-catching.** `init_schema` is `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` only — it will **never** `ALTER` an existing table, add a missing column, or drop a stale one.

**Schema evolution workflow**:
1. Edit both schema files (`src/models/mysql_schema.rs` and `src/models/postgres_schema.rs`) so they still describe the same target shape.
2. Update every affected query in `articles.rs`, `pages.rs`, and `users.rs` for both backends.
3. Add or update a test that proves repeated `init_schema` calls are still safe and do not destroy existing rows.
4. Treat schema edits as applying only to fresh databases. `init_schema` will not `ALTER` an existing table into the new shape; any database that already exists has to be migrated out-of-band (or dropped and recreated) before the new code will run against it.

**`updated_at` bookkeeping is application-level**, not relied-upon at the DB level. Every `UPDATE` in the model files explicitly `SET updated_at = NOW()`, including `Page::update` and `User::modify_password`. MySQL's `ON UPDATE CURRENT_TIMESTAMP` column default remains as belt-and-suspenders but is load-bearing on **no** backend — do not rely on it. Postgres has no trigger; **any new `UPDATE` you add must bump `updated_at` explicitly or it will silently go stale on Postgres.**

**`SELECT LAST_INSERT_ID()` vs `RETURNING id`.** For MySQL the `Article::insert` / `Page::insert` paths run an INSERT then a separate `SELECT LAST_INSERT_ID()` inside the same transaction. For Postgres they use `INSERT ... RETURNING id` via `query_scalar::<_, i32>`. Keep the pattern symmetric across backends — callers still receive an `i32` id.

**`INSERT IGNORE` vs `ON CONFLICT DO NOTHING`.** `User::insert` uses `INSERT IGNORE INTO users` on MySQL and `INSERT INTO users ... ON CONFLICT (username) DO NOTHING` on Postgres. Both are load-bearing for the admin bootstrap: `password_auth::generate_hash` is non-deterministic (random salt), so every boot produces a new hash. If this ever overwrote an existing row, the admin's manually changed password would be wiped on restart. **Keep the conflict-target explicit on Postgres** (`ON CONFLICT (username)`) so future additional unique indexes do not silently change behavior.

**Postgres bind-type pitfalls**:
- `sqlx-postgres 0.8` does **not** implement `Encode<Postgres> for u32` (Postgres has no unsigned integer types). The public pagination API keeps `u32` but the internal helpers cast via `i64::from(u)` **before** multiplying to avoid any u32-level overflow, e.g. `let offset = i64::from(page.saturating_sub(1)) * i64::from(per_page);`. Do the same for any new `u32`-bearing query.
- `COUNT(col)` returns `INT8` on Postgres and sqlx-postgres 0.8 is strict about type compatibility. If you materialize a COUNT result into Rust, use `i64` or cast it explicitly in SQL. `Tags::get_all_with_count` deliberately avoids returning the count at all because the count is only used for ordering.

**Article tags are dual-stored**, and both copies must stay in sync:
- `articles.tags` holds a comma-separated canonical string (used for display and for round-tripping through the editor).
- The `tags` table holds normalized rows keyed by `article_id` (used by `GET /tag/{name}` via `INNER JOIN` and by `Tags::get_all_with_count`). It intentionally carries no `created_at` / `updated_at` columns — a pure association table does not need them, and both backends' schemas reflect that.

`Article::insert` writes both; `Article::update` clears the `tags` rows via `clear_tags_{mysql,postgres}` and re-inserts them; `Article::delete` clears the `tags` rows inside the same transaction as the article delete. Any change to the CSV format (delimiter, normalization, case handling) must be applied atomically across both `Tags::insert_tags_{mysql,postgres}` helpers, `utils::sort_out_tags`, and `handler_article`'s display-time split on `,`.

The admin user is bootstrapped via `User::insert`. On first boot the admin password equals the username and must be changed via `/admin/change_password`.

### Markdown & templates
Markdown rendering uses `comrak` with a `syntect` adapter, exposed to templates as the `md_to_html` MiniJinja filter (configured in `AppState::build_env`). The syntax theme comes from `config.toml [style] code_syntax_highlight_theme`. Other custom filters: `truncate_str`, `to_lowercase`, `concat_url`, and `get_slug` (falls back to `id` when `slug` is empty — this is how permalinks keep working for articles that were created without a slug).

### Config exposure to templates
Not every field of `Config` is reachable from templates. `Config`, `Giscus`, `Analytics`, and `TwitterCard` manually implement `minijinja::value::Object` in `src/config.rs` and only whitelist specific fields. When adding a new config field that must be template-visible, update **both** the `get_value` *and* `enumerate` methods on the corresponding `Object` impl — missing `enumerate` breaks template iteration (`for`, `items()`) even when `get_value` works.

## Conventions

- Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`).
- Sign off commits with `git commit -s` — don't hand-write `Signed-off-by:` lines, and never include `Co-Authored-By:`.
- Use `git push --force-with-lease` rather than `--force`.

## Deployment

CI deploys to Fly.io on every push to `main` (`.github/workflows/fly-deploy.yml`). The workflow stages `DATABASE_URL` and `UMAMI_ID` as Fly secrets, then runs `fly deploy --remote-only`. The matching GitHub Actions secret must be configured under the `DATABASE_URL` name before deploys; the workflow has no fallback. The Fly app is configured in `fly.toml` (region `nrt`, internal port `5299`, `/ping` healthcheck, `min_machines_running = 0` with auto-suspend). Because the session store is in-process, suspensions and redeploys both log admin sessions out — acceptable for a single-admin blog, but a blocker for any future multi-tenant mode or multi-machine rollout.

**Database backend selection.** Split config-file fields (`username` / `password` / `host` / `port` / `database` under `[database]`) rely on `[database].backend = "mysql" | "postgres"` to decide which DSN scheme to build. If `connection_url` in the config or `DATABASE_URL` in the environment is present, that full URL wins and its own scheme chooses the backend: `mysql://...` uses sqlx-mysql, `postgres://...` (or `postgresql://...`) uses sqlx-postgres.
