use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use axum_login::{
    login_required,
    tower_sessions::{cookie::time::Duration, Expiry, MemoryStore, SessionManagerLayer},
    AuthManagerLayerBuilder,
};
use chrono::{DateTime, Utc};
use comrak::{markdown_to_html_with_plugins, plugins::syntect, Options, Plugins};
use minijinja::{context, Environment, Value};
use tokio::sync::RwLock;
use tower_http::{
    services::ServeDir,
    trace::{self, TraceLayer},
};
use tower_sessions::cookie::Key;
use tracing::{info, Level};

use crate::{
    config::Config,
    error::Error,
    handlers::{
        handler_404, handler_admin, handler_article, handler_articles, handler_change_pw_get,
        handler_change_pw_post, handler_custom_page, handler_delete_post, handler_edit_article_get,
        handler_edit_page_get, handler_edit_post, handler_feed, handler_home, handler_login_get,
        handler_login_post, handler_logout, handler_page, handler_ping, handler_tag, handler_tags,
    },
    models::{init_schema, Article, ArticleSummary, DbPool, Page, User},
};

const TEMPLATES_DIR: &str = "templates";
const STATIC_DIR: &str = "static";
// TODO: support specifying the config file path via command line argument.
const CONFIG_FILE_PATH: &str = "config.toml";

// AppState is used to pass the global states to the handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub env: Environment<'static>,
    pub db: DbPool,
    // Cache the feed content to reduce the database query.
    pub feed_cache: Arc<RwLock<(DateTime<Utc>, Option<String>)>>,
    // Cache article list metadata so public index/tag pages do not hit the DB.
    pub article_summaries_cache: Arc<RwLock<Vec<ArticleSummary>>>,
    // Cache the page titles to avoid querying the database on every template render.
    pub page_titles_cache: Arc<RwLock<Vec<String>>>,
}

impl AppState {
    pub async fn new() -> Result<Self, Error> {
        info!("parsing config file");
        let config = Config::new(CONFIG_FILE_PATH)?;

        info!("connecting to the database");
        // connect to the database. The URL scheme selects the backend
        // (`mysql://` or `postgres://`).
        let db = DbPool::connect(&config.database_url()?).await?;
        info!("initializing the database");
        // create the tables if they don't exist.
        init_schema(&db).await?;
        // init the admin user.
        let admin_username = config.admin_username();
        User::insert(
            &db,
            &admin_username,
            &password_auth::generate_hash(&admin_username),
        )
        .await?;

        info!("building the environment");
        let env = Self::build_env(&config)?;

        let article_summaries = Article::get_all_summaries(&db).await;
        let page_titles = Page::get_all_titles(&db).await;
        let state = Self {
            config,
            env,
            db,
            feed_cache: Arc::new(RwLock::new((Default::default(), None))),
            article_summaries_cache: Arc::new(RwLock::new(article_summaries)),
            page_titles_cache: Arc::new(RwLock::new(page_titles)),
        };
        state.refresh_feed_cache(true).await;

        Ok(state)
    }

    fn build_env(config: &Config) -> Result<Environment<'static>, Error> {
        let mut env = Environment::new();
        // iterate the templates directory and add all the templates.
        for entry in std::fs::read_dir(TEMPLATES_DIR)? {
            let path = entry.unwrap().path();
            if !path.is_file() {
                continue;
            }
            let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
            let template_content = std::fs::read_to_string(path)?;
            env.add_template_owned(file_name, template_content)?;
        }
        // load the global variables into the environment.
        env.add_global("config", Value::from_object(config.clone()));
        // load the embedded functions into the environment.
        let config_clone = config.clone();
        env.add_filter("md_to_html", move |md_content: &str| {
            Self::md_to_html(&config_clone, md_content)
        });
        env.add_filter("truncate_str", |value: &str, max_length: usize| {
            if value.chars().count() > max_length {
                value.chars().take(max_length).collect()
            } else {
                value.to_string()
            }
        });
        env.add_filter("to_lowercase", |value: &str| value.to_lowercase());
        env.add_filter("concat_url", |value: &str, uri: &str| {
            if value.ends_with('/') {
                format!("{}{}", value, uri)
            } else {
                format!("{}/{}", value, uri)
            }
        });
        env.add_filter("get_slug", |value: &Value| {
            let slug = value.get_attr("slug").unwrap_or_default().to_string();
            if slug.is_empty() {
                value.get_attr("id").unwrap_or_default().to_string()
            } else {
                slug
            }
        });

        Ok(env)
    }

    pub async fn refresh_feed_cache(&self, force: bool) {
        let mut feed_cache = self.feed_cache.write().await;
        // Get the latest updated time of the articles.
        let article_latest_updated = Article::get_latest_updated(&self.db)
            .await
            .unwrap_or_default();
        // No need to update
        if article_latest_updated <= feed_cache.0 && !force && feed_cache.1.is_some() {
            return;
        }
        feed_cache.0 = article_latest_updated;
        const FEED_ARTICLE_LIMIT: u32 = 20;
        feed_cache.1 = Some(
            self.render_template(
                "feed.xml",
                context! {
                    updated_at => article_latest_updated,
                    articles => Article::get_recent(&self.db, FEED_ARTICLE_LIMIT).await,
                },
            )
            .await,
        );
        info!(
            "feed cache updated at {}, latest article updated at {}",
            Utc::now(),
            article_latest_updated
        );
    }

    pub async fn refresh_page_titles_cache(&self) {
        let titles = Page::get_all_titles(&self.db).await;
        let mut cache = self.page_titles_cache.write().await;
        *cache = titles;
    }

    pub async fn refresh_article_summaries_cache(&self) {
        let articles = Article::get_all_summaries(&self.db).await;
        let mut cache = self.article_summaries_cache.write().await;
        *cache = articles;
    }

    pub async fn get_article_summaries(&self) -> Vec<ArticleSummary> {
        self.article_summaries_cache.read().await.clone()
    }

    pub async fn get_feed_cache(&self) -> String {
        let feed_cache = self.feed_cache.read().await;
        feed_cache.1.clone().unwrap_or_default()
    }

    fn md_to_html(config: &Config, md_content: &str) -> String {
        // enable some extension options.
        let mut options = Options::default();
        options.extension.strikethrough = true;
        options.extension.autolink = true;
        options.render.figure_with_caption = true;
        // enable the syntax highlight adapter.
        let mut plugins = Plugins::default();
        let adapter = syntect::SyntectAdapterBuilder::new()
            .theme(config.code_syntax_highlight_theme().as_str())
            .build();
        plugins.render.codefence_syntax_highlighter = Some(&adapter);

        markdown_to_html_with_plugins(md_content, &options, &plugins)
    }

    pub async fn render_template(&self, template_name: &str, context: Value) -> String {
        let template = self.env.get_template(template_name).unwrap();
        let page_titles = self.page_titles_cache.read().await.clone();
        template
            .render(context! {
                page_titles => page_titles,
                ..context,
            })
            .unwrap()
    }
}

pub struct App {
    state: AppState,
}

impl App {
    pub async fn new() -> Result<Self, Error> {
        Ok(Self {
            state: AppState::new().await?,
        })
    }

    pub async fn serve(&self) -> Result<(), Error> {
        // session layer resident in memory.
        let session_layer = SessionManagerLayer::new(MemoryStore::default())
            .with_secure(false)
            .with_expiry(Expiry::OnInactivity(Duration::days(
                self.state.config.admin_inactive_expiry_days(),
            )))
            .with_signed(Key::generate());
        // authentication layer
        let auth_layer = AuthManagerLayerBuilder::new(self.state.clone(), session_layer).build();

        let admin_router = Router::new()
            .route("/", get(handler_admin))
            .route("/change_password", get(handler_change_pw_get))
            .route("/change_password", post(handler_change_pw_post))
            .route("/edit/article/new", get(handler_edit_article_get))
            .route("/edit/article/new", post(handler_edit_post::<Article>))
            .route("/edit/article/{id}", get(handler_edit_article_get))
            .route("/edit/article/{id}", post(handler_edit_post::<Article>))
            .route("/delete/article/{id}", get(handler_delete_post::<Article>))
            .route("/edit/page/new", get(handler_edit_page_get))
            .route("/edit/page/new", post(handler_edit_post::<Page>))
            .route("/edit/page/{id}", get(handler_edit_page_get))
            .route("/edit/page/{id}", post(handler_edit_post::<Page>))
            .route("/delete/page/{id}", get(handler_delete_post::<Page>))
            .route_layer(login_required!(AppState, login_url = "/login"));

        let app = Router::new()
            .fallback(handler_404)
            // serve the static files
            .nest_service("/static", ServeDir::new(STATIC_DIR))
            // serve the page handlers
            .route("/", get(handler_home))
            .route("/page/{num}", get(handler_page))
            .route("/article/{id_or_slug}", get(handler_article))
            .route("/articles", get(handler_articles))
            .route("/tag/{tag}", get(handler_tag))
            .route("/tags", get(handler_tags))
            .route("/feed", get(handler_feed))
            .route("/ping", get(handler_ping))
            .route("/{page}", get(handler_custom_page))
            .route("/login", get(handler_login_get))
            .route("/login", post(handler_login_post))
            .route("/logout", get(handler_logout))
            // nest the admin router under the `/admin` path.
            .nest("/admin", admin_router)
            .layer(auth_layer)
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                    .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
            )
            .with_state(Arc::new(self.state.clone()));

        let listener = tokio::net::TcpListener::bind(self.state.config.server_url()).await?;
        info!("listening on {}", listener.local_addr()?);
        axum::serve(listener, app).await?;

        Ok(())
    }
}
