use std::sync::Arc;

use minijinja::{
    value::{Enumerator, Object},
    Value,
};
use serde::Deserialize;

use crate::error::Error;

#[derive(Clone, Debug, Deserialize)]
struct Deploy {
    host: String,
    port: u16,
}

#[derive(Clone, Debug, Deserialize)]
struct Meta {
    blog_name: String,
    blog_url: String,
    blog_author: String,
    about_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct Admin {
    username: String,
    inactive_expiry_days: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
struct Style {
    article_per_page: u32,
    code_syntax_highlight_theme: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum DatabaseBackend {
    #[default]
    MySql,
    Postgres,
}

impl DatabaseBackend {
    fn scheme(self) -> &'static str {
        match self {
            Self::MySql => "mysql",
            Self::Postgres => "postgres",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Database {
    #[serde(default)]
    backend: DatabaseBackend,
    connection_url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct Giscus {
    enable: bool,
    category: String,
    category_id: String,
    emit_metadata: String,
    input_position: String,
    lang: String,
    loading: String,
    mapping: String,
    reactions_enabled: String,
    repo: String,
    repo_id: String,
    theme: String,
}

impl Object for Giscus {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "enable" => Some(Value::from(self.enable)),
            "category" => Some(Value::from(self.category.clone())),
            "category_id" => Some(Value::from(self.category_id.clone())),
            "emit_metadata" => Some(Value::from(self.emit_metadata.clone())),
            "input_position" => Some(Value::from(self.input_position.clone())),
            "lang" => Some(Value::from(self.lang.clone())),
            "loading" => Some(Value::from(self.loading.clone())),
            "mapping" => Some(Value::from(self.mapping.clone())),
            "reactions_enabled" => Some(Value::from(self.reactions_enabled.clone())),
            "repo" => Some(Value::from(self.repo.clone())),
            "repo_id" => Some(Value::from(self.repo_id.clone())),
            "theme" => Some(Value::from(self.theme.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&[
            "enable",
            "category",
            "category_id",
            "emit_metadata",
            "input_position",
            "lang",
            "loading",
            "mapping",
            "reactions_enabled",
            "repo",
            "repo_id",
            "theme",
        ])
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Analytics {
    google: Option<String>,
    plausible: Option<String>,
    umami: Option<String>,
}

impl Object for Analytics {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "google" => Some(Value::from(self.google.clone())),
            "plausible" => Some(Value::from(self.plausible.clone())),
            "umami" => Some(Value::from(self.umami.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["google", "plausible", "umami"])
    }
}

#[derive(Clone, Debug, Deserialize)]
struct TwitterCard {
    enabled: bool,
    user_id: String,
}

impl Object for TwitterCard {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "enabled" => Some(Value::from(self.enabled)),
            "user_id" => Some(Value::from(self.user_id.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["enabled", "user_id"])
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    deploy: Deploy,
    meta: Meta,
    admin: Admin,
    style: Style,
    #[serde(alias = "mysql")]
    database: Database,
    giscus: Giscus,
    analytics: Analytics,
    twitter_card: TwitterCard,
}

impl Config {
    pub fn new(path: &str) -> Result<Self, Error> {
        let config_content = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&config_content).map_err(Error::Toml)?;
        // get some environment variables.
        config.load_env_vars()?;
        // validate the config to ensure the deployment is correct.
        config.validate()?;

        Ok(config)
    }

    fn load_env_vars(&mut self) -> Result<(), Error> {
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            self.database.connection_url = Some(database_url);
        } else if let Ok(mysql_connection_url) = std::env::var("MYSQL_CONNECTION_URL") {
            self.database.connection_url = Some(mysql_connection_url);
        }
        if let Ok(plausible_domain) = std::env::var("PLAUSIBLE_DOMAIN") {
            self.analytics.plausible = Some(plausible_domain);
        }
        if let Ok(umami_id) = std::env::var("UMAMI_ID") {
            self.analytics.umami = Some(umami_id);
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Error> {
        // check the deployment config.
        if self.deploy.host.is_empty() || self.deploy.port == 0 {
            return Err(Error::ConfigValidation(
                "invalid deployment config, please specify the host and port".to_string(),
            ));
        }
        // check the database config.
        if self.database.connection_url.is_none()
            && (self.database.username.is_none()
                || self.database.password.is_none()
                || self.database.host.is_none()
                || self.database.port.is_none()
                || self.database.database.is_none())
        {
            return Err(Error::ConfigValidation(
                "invalid database config, please specify the connection URL or the username, password, host, port and database".to_string(),
            ));
        }

        Ok(())
    }

    // get the server URL according to the config, this will be used to run the server.
    pub fn server_url(&self) -> String {
        format!("{}:{}", self.deploy.host, self.deploy.port)
    }

    // get the database connection URL according to the config. `connection_url`
    // wins if it is set; otherwise build one from the split fields and the
    // configured backend. Full URLs still need their own scheme
    // (`mysql://`, `postgres://`, or `postgresql://`) because sqlx expects it.
    pub fn database_url(&self) -> Result<String, Error> {
        if let Some(connection_url) = &self.database.connection_url {
            Ok(connection_url.clone())
        } else {
            let (username, password, host, port, database) = (
                self.database
                    .username
                    .as_ref()
                    .ok_or(Error::InvalidDatabaseConfig)?,
                self.database
                    .password
                    .as_ref()
                    .ok_or(Error::InvalidDatabaseConfig)?,
                self.database
                    .host
                    .as_ref()
                    .ok_or(Error::InvalidDatabaseConfig)?,
                self.database.port.ok_or(Error::InvalidDatabaseConfig)?,
                self.database
                    .database
                    .as_ref()
                    .ok_or(Error::InvalidDatabaseConfig)?,
            );

            Ok(format!(
                "{}://{}:{}@{}:{}/{}",
                self.database.backend.scheme(),
                username,
                password,
                host,
                port,
                database
            ))
        }
    }

    pub fn admin_username(&self) -> String {
        self.admin.username.clone()
    }

    pub fn admin_inactive_expiry_days(&self) -> i64 {
        self.admin.inactive_expiry_days.unwrap_or(30)
    }

    pub fn article_per_page(&self) -> u32 {
        self.style.article_per_page
    }

    pub fn code_syntax_highlight_theme(&self) -> String {
        self.style.code_syntax_highlight_theme.clone()
    }
}

impl Object for Config {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        // just expose those fields that will be used in the templates.
        match key.as_str()? {
            "blog_name" => Some(Value::from(self.meta.blog_name.clone())),
            "blog_url" => Some(Value::from(self.meta.blog_url.clone())),
            "blog_author" => Some(Value::from(self.meta.blog_author.clone())),
            "about_url" => Some(Value::from(self.meta.about_url.clone())),
            "article_per_page" => Some(Value::from(self.style.article_per_page)),
            "giscus" => Some(Value::from_object(self.giscus.clone())),
            "analytics" => Some(Value::from_object(self.analytics.clone())),
            "twitter_card" => Some(Value::from_object(self.twitter_card.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&[
            "blog_name",
            "blog_url",
            "blog_author",
            "about_url",
            "article_per_page",
            "giscus",
            "analytics",
            "twitter_card",
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn parse_config(section_name: &str, database_section: &str) -> Config {
        toml::from_str(&format!(
            r#"
[deploy]
host = "127.0.0.1"
port = 5299

[meta]
blog_name = "rsomhaP"
blog_url = "https://example.com"
blog_author = "author"

[admin]
username = "admin"

[style]
article_per_page = 15
code_syntax_highlight_theme = "base16-eighties.dark"

[{section_name}]
{database_section}

[giscus]
enable = false
category = ""
category_id = ""
emit_metadata = "0"
input_position = "top"
lang = "en"
loading = ""
mapping = "og:title"
reactions_enabled = "1"
repo = ""
repo_id = ""
theme = "light"

[analytics]

[twitter_card]
enabled = false
user_id = "@user"
"#
        ))
        .unwrap()
    }

    #[test]
    fn test_database_url_defaults_split_fields_to_mysql() {
        let config = parse_config(
            "database",
            r#"
username = "root"
password = "password"
host = "127.0.0.1"
port = 4000
database = "rsomhaP"
"#,
        );

        config.validate().unwrap();
        assert_eq!(
            config.database_url().unwrap(),
            "mysql://root:password@127.0.0.1:4000/rsomhaP"
        );
    }

    #[test]
    fn test_database_url_uses_postgres_backend_for_split_fields() {
        let config = parse_config(
            "database",
            r#"
backend = "postgres"
username = "postgres"
password = "password"
host = "127.0.0.1"
port = 5432
database = "rsomhaP"
"#,
        );

        config.validate().unwrap();
        assert_eq!(
            config.database_url().unwrap(),
            "postgres://postgres:password@127.0.0.1:5432/rsomhaP"
        );
    }

    #[test]
    fn test_database_url_prefers_connection_url_over_backend_field() {
        let config = parse_config(
            "database",
            r#"
backend = "mysql"
connection_url = "postgresql://postgres:secret@127.0.0.1:5432/rsomhaP"
username = "root"
password = "password"
host = "127.0.0.1"
port = 4000
database = "ignored"
"#,
        );

        config.validate().unwrap();
        assert_eq!(
            config.database_url().unwrap(),
            "postgresql://postgres:secret@127.0.0.1:5432/rsomhaP"
        );
    }

    #[test]
    fn test_legacy_mysql_section_alias_still_parses() {
        let config = parse_config(
            "mysql",
            r#"
username = "root"
password = "password"
host = "127.0.0.1"
port = 4000
database = "rsomhaP"
"#,
        );

        config.validate().unwrap();
        assert_eq!(
            config.database_url().unwrap(),
            "mysql://root:password@127.0.0.1:4000/rsomhaP"
        );
    }

    #[test]
    fn test_load_env_vars_falls_back_to_legacy_mysql_connection_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        let database_url_before = std::env::var_os("DATABASE_URL");
        let mysql_connection_url_before = std::env::var_os("MYSQL_CONNECTION_URL");

        unsafe {
            std::env::remove_var("DATABASE_URL");
            std::env::set_var(
                "MYSQL_CONNECTION_URL",
                "mysql://legacy:password@127.0.0.1:4000/rsomhaP",
            );
        }

        let mut config = parse_config(
            "database",
            r#"
username = "root"
password = "password"
host = "127.0.0.1"
port = 4000
database = "rsomhaP"
"#,
        );
        config.load_env_vars().unwrap();
        assert_eq!(
            config.database_url().unwrap(),
            "mysql://legacy:password@127.0.0.1:4000/rsomhaP"
        );

        restore_env_var("DATABASE_URL", database_url_before);
        restore_env_var("MYSQL_CONNECTION_URL", mysql_connection_url_before);
    }

    #[test]
    fn test_load_env_vars_prefers_database_url_over_legacy_mysql_connection_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        let database_url_before = std::env::var_os("DATABASE_URL");
        let mysql_connection_url_before = std::env::var_os("MYSQL_CONNECTION_URL");

        unsafe {
            std::env::set_var(
                "DATABASE_URL",
                "postgres://preferred:secret@127.0.0.1:5432/rsomhaP",
            );
            std::env::set_var(
                "MYSQL_CONNECTION_URL",
                "mysql://legacy:password@127.0.0.1:4000/rsomhaP",
            );
        }

        let mut config = parse_config(
            "database",
            r#"
backend = "mysql"
username = "root"
password = "password"
host = "127.0.0.1"
port = 4000
database = "rsomhaP"
"#,
        );
        config.load_env_vars().unwrap();
        assert_eq!(
            config.database_url().unwrap(),
            "postgres://preferred:secret@127.0.0.1:5432/rsomhaP"
        );

        restore_env_var("DATABASE_URL", database_url_before);
        restore_env_var("MYSQL_CONNECTION_URL", mysql_connection_url_before);
    }

    fn restore_env_var(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
}
