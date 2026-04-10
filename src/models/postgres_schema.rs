use crate::Error;

const CREATE_ARTICLES: &str = r#"
CREATE TABLE IF NOT EXISTS articles (
    id SERIAL PRIMARY KEY,
    slug VARCHAR(255) NOT NULL DEFAULT '',
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

const CREATE_IDX_SLUG: &str = "CREATE INDEX IF NOT EXISTS idx_slug ON articles (slug)";

const CREATE_TAGS: &str = r#"
CREATE TABLE IF NOT EXISTS tags (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    article_id INTEGER NOT NULL
)
"#;

const CREATE_IDX_TAG_NAME: &str = "CREATE INDEX IF NOT EXISTS idx_name ON tags (name)";
const CREATE_IDX_TAG_ARTICLE_ID: &str =
    "CREATE INDEX IF NOT EXISTS idx_article_id ON tags (article_id)";

const CREATE_PAGES: &str = r#"
CREATE TABLE IF NOT EXISTS pages (
    id SERIAL PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

const CREATE_USERS: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    username VARCHAR(255) NOT NULL,
    password VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

const CREATE_IDX_USERNAME: &str =
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_username ON users (username)";

pub async fn init_schema(pool: &sqlx::PgPool) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    for sql in [
        CREATE_ARTICLES,
        CREATE_IDX_SLUG,
        CREATE_TAGS,
        CREATE_IDX_TAG_NAME,
        CREATE_IDX_TAG_ARTICLE_ID,
        CREATE_PAGES,
        CREATE_USERS,
        CREATE_IDX_USERNAME,
    ] {
        sqlx::query(sql).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
