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
use comrak::{markdown_to_html_with_plugins, plugins::syntect, Options, Plugins};
use minijinja::{context, Environment, Value};
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
        handler_change_pw_post, handler_custom_page, handler_delete_article, handler_delete_page,
        handler_edit_article_get, handler_edit_page_get, handler_edit_post, handler_feed,
        handler_home, handler_login_get, handler_login_post, handler_logout, handler_page,
        handler_ping, handler_tag, handler_tags, ArticleForm, PageForm,
    },
    models::{create_tables, Page, User},
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
    pub db: sqlx::MySqlPool,
}

impl AppState {
    pub async fn new() -> Result<Self, Error> {
        info!("parsing config file");
        let config = Config::new(CONFIG_FILE_PATH)?;

        info!("connecting to the database");
        // connect to the database.
        let db = match sqlx::MySqlPool::connect(&config.mysql_connection_url()).await {
            Ok(db) => db,
            Err(e) => return Err(Error::Sqlx(e)),
        };
        info!("initializing the database");
        // create the tables if they don't exist.
        create_tables(&db).await.unwrap();
        // init the admin user.
        let admin_username = config.admin_username();
        User::insert(
            &db,
            &admin_username,
            &password_auth::generate_hash(&admin_username),
        )
        .await
        .unwrap();

        info!("initializing the environment");
        let mut env = Environment::new();
        // iterate the templates directory and add all the templates.
        for entry in std::fs::read_dir(TEMPLATES_DIR).unwrap() {
            let path = entry.unwrap().path();
            if !path.is_file() {
                continue;
            }
            let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
            let template_content = std::fs::read_to_string(path).unwrap();
            env.add_template_owned(file_name, template_content).unwrap();
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

        Ok(Self { config, env, db })
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
        template
            .render(context! {
                page_titles => Page::get_all_titles(&self.db).await,
                ..context,
            })
            .unwrap()
    }
}

pub const CHANGE_PW_URL: &str = "/admin/change_password";

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
            .route("/admin", get(handler_admin))
            .route(CHANGE_PW_URL, get(handler_change_pw_get))
            .route(CHANGE_PW_URL, post(handler_change_pw_post))
            .route("/admin/edit/article/new", get(handler_edit_article_get))
            .route(
                "/admin/edit/article/new",
                post(handler_edit_post::<ArticleForm>),
            )
            .route("/admin/edit/article/:id", get(handler_edit_article_get))
            .route(
                "/admin/edit/article/:id",
                post(handler_edit_post::<ArticleForm>),
            )
            .route("/admin/delete/article/:id", get(handler_delete_article))
            .route("/admin/edit/page/new", get(handler_edit_page_get))
            .route("/admin/edit/page/new", post(handler_edit_post::<PageForm>))
            .route("/admin/edit/page/:id", get(handler_edit_page_get))
            .route("/admin/edit/page/:id", post(handler_edit_post::<PageForm>))
            .route("/admin/delete/page/:id", get(handler_delete_page))
            .route("/logout", get(handler_logout))
            .route_layer(login_required!(AppState, login_url = "/login"))
            .route("/login", get(handler_login_get))
            .route("/login", post(handler_login_post));

        let app = Router::new()
            .fallback(handler_404)
            // serve the static files
            .nest_service("/static", ServeDir::new(STATIC_DIR))
            // serve the page handlers
            .route("/", get(handler_home))
            .route("/page/:num", get(handler_page))
            .route("/article/:id", get(handler_article))
            .route("/articles", get(handler_articles))
            .route("/tag/:tag", get(handler_tag))
            .route("/tags", get(handler_tags))
            .route("/feed", get(handler_feed))
            .route("/ping", get(handler_ping))
            .route("/:page", get(handler_custom_page))
            .merge(admin_router)
            .layer(auth_layer)
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                    .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
            )
            .with_state(Arc::new(self.state.clone()));

        let listener = tokio::net::TcpListener::bind(self.state.config.server_url())
            .await
            .unwrap();
        info!("listening on {}", listener.local_addr().unwrap());
        axum::serve(listener, app).await.unwrap();

        Ok(())
    }
}
