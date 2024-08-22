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

#[derive(Clone, Debug, Deserialize)]
struct MySQL {
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
}

impl Object for Analytics {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "google" => Some(Value::from(self.google.clone())),
            "plausible" => Some(Value::from(self.plausible.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["google", "plausible"])
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    deploy: Deploy,
    meta: Meta,
    admin: Admin,
    style: Style,
    mysql: MySQL,
    giscus: Giscus,
    analytics: Analytics,
}

impl Config {
    pub fn new(path: &str) -> Result<Self, Error> {
        let config_content = std::fs::read_to_string(path).unwrap();
        let config: Self = toml::from_str(&config_content).map_err(Error::Toml)?;
        config.validate()?;

        Ok(config)
    }

    pub fn validate(&self) -> Result<(), Error> {
        // check the deployment config.
        if self.deploy.host.is_empty() || self.deploy.port == 0 {
            return Err(Error::ConfigValidation(
                "invalid deployment config, please specify the host and port".to_string(),
            ));
        }
        // check the MySQL config.
        if self.mysql.connection_url.is_none()
            && (self.mysql.username.is_none()
                || self.mysql.password.is_none()
                || self.mysql.host.is_none()
                || self.mysql.port.is_none()
                || self.mysql.database.is_none())
        {
            return Err(Error::ConfigValidation(
                "invalid MySQL config, please specify the connection URL or the username, password, host, port and database".to_string(),
            ));
        }

        Ok(())
    }

    // get the server URL according to the config, this will be used to run the server.
    pub fn server_url(&self) -> String {
        format!("{}:{}", self.deploy.host, self.deploy.port)
    }

    // get the MySQL connection URL according to the config, it will use `connection_url` if it is set,
    // otherwise it will use `username`, `password`, `host`, `port` and `database` to build one.
    pub fn mysql_connection_url(&self) -> String {
        if let Some(connection_url) = self.mysql.connection_url.clone() {
            connection_url
        } else {
            format!(
                "mysql://{}:{}@{}:{}/{}",
                self.mysql.username.as_ref().unwrap(),
                self.mysql.password.as_ref().unwrap(),
                self.mysql.host.as_ref().unwrap(),
                self.mysql.port.unwrap(),
                self.mysql.database.as_ref().unwrap()
            )
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
        ])
    }
}
