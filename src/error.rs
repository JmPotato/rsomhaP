#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    MiniJinja(#[from] minijinja::Error),

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error(transparent)]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("config validation failed: {0}")]
    ConfigValidation(String),

    #[error("invalid MySQL config, please specify the connection URL or the username, password, host, port and database")]
    InvalidMySQLConfig,

    #[error("page with same title {0} already exists")]
    PageTitleExists(String),
}
