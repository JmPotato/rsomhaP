use std::{
    collections::HashMap,
    fmt::{self, Display},
    sync::Arc,
};

use axum::{
    async_trait,
    body::Body,
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, Response, StatusCode},
    response::{Html, IntoResponse, Redirect},
    Form,
};
use axum_login::AuthSession;
use chrono::Datelike;
use minijinja::context;
use rand::{thread_rng, Rng};
use regex::Regex;
use serde::{de::DeserializeOwned, Deserialize};
use tracing::error;

use crate::{
    app::{AppState, CHANGE_PW_URL},
    auth::Credentials,
    models::{Article, Page, Tags, User},
};

pub async fn handler_home(state: State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    handler_page(state, Path(1)).await
}

pub async fn handler_page(
    State(state): State<Arc<AppState>>,
    Path(page_num): Path<i32>,
) -> Result<Html<String>, StatusCode> {
    // validate `page_num` before querying the database.
    if page_num <= 0 {
        return handler_404(State(state)).await;
    }
    let total_article_count = Article::get_total_count(&state.db).await as u32;
    let article_per_page = state.config.article_per_page();
    let max_page = (total_article_count as f32 / article_per_page as f32).ceil() as u32;
    if max_page != 0 && page_num as u32 > max_page {
        return handler_404(State(state)).await;
    }
    let articles = Article::get_on_page(&state.db, page_num as u32, article_per_page).await;

    Ok(Html(
        state
            .render_template(
                "home.html",
                context! {
                    articles => articles,
                    total_article_count => total_article_count,
                    page_num => page_num,
                    max_page => max_page,
                },
            )
            .await,
    ))
}

pub async fn handler_article(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    auth_session: AuthSession<AppState>,
) -> Result<Html<String>, StatusCode> {
    if let Some(article) = Article::get_by_id(&state.db, id).await {
        return Ok(Html(
            state
                .render_template(
                    "article.html",
                    context! {
                        article => article,
                        tags => article
                            .tags
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .collect::<Vec<String>>(),
                        image => {
                            // find all image URLs in the article markdown content and choose one randomly.
                            let re = Regex::new(r"\!\[.*?\]\((.*?)\)").unwrap();
                            let mut image_urls: Vec<String> = vec![];
                            for (_, [image_url]) in re.captures_iter(&article.content).map(|c| c.extract()) {
                                image_urls.push(image_url.to_string());
                            }
                            if image_urls.is_empty() {
                                None
                            } else {
                                Some(image_urls[thread_rng().gen_range(0..image_urls.len())].clone())
                            }
                        },
                        logged_in => auth_session.user.is_some(),
                    },
                )
                .await,
        ));
    }
    handler_404(State(state)).await
}

pub async fn handler_tag(
    State(state): State<Arc<AppState>>,
    Path(tag): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let mut years = vec![];
    // get articles by tag and map them by year.
    let articles_by_year = Article::get_by_tag(&state.db, &tag).await.into_iter().fold(
        HashMap::new(),
        |mut acc, article| {
            let year = article.created_at.year();
            acc.entry(year)
                .or_insert_with(|| {
                    years.push(year);
                    Vec::new()
                })
                .push(article);
            acc
        },
    );
    // sort `years` in descending order.
    years.sort_by(|a, b| b.cmp(a));
    Ok(Html(
        state
            .render_template(
                "tag.html",
                context! {
                    tag => tag,
                    years => years,
                    articles_by_year => articles_by_year,
                },
            )
            .await,
    ))
}

pub async fn handler_404(state: State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    Ok(Html(state.render_template("404.html", context! {}).await))
}

pub async fn handler_articles(state: State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let mut years = vec![];
    // get all articles and map them by year.
    let articles_by_year =
        Article::get_all(&state.db)
            .await
            .into_iter()
            .fold(HashMap::new(), |mut acc, article| {
                let year = article.created_at.year();
                acc.entry(year)
                    .or_insert_with(|| {
                        years.push(year);
                        Vec::new()
                    })
                    .push(article);
                acc
            });

    Ok(Html(
        state
            .render_template(
                "articles.html",
                context! {
                years => years,
                articles_by_year => articles_by_year,},
            )
            .await,
    ))
}

pub async fn handler_tags(state: State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    Ok(Html(
        state
            .render_template(
                "tags.html",
                context! {tags => Tags::get_all_with_count(&state.db).await},
            )
            .await,
    ))
}

pub async fn handler_custom_page(
    State(state): State<Arc<AppState>>,
    Path(title): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let page = match Page::get_by_title(&state.db, &title).await {
        Some(page) => page,
        None => return handler_404(State(state)).await,
    };

    Ok(Html(
        state
            .render_template("page.html", context! {page => page})
            .await,
    ))
}

pub async fn handler_feed(state: State<Arc<AppState>>) -> Response<Body> {
    let mut response = Response::new(Body::new(
        state
            .render_template(
                "feed.xml",
                context! {
                    updated_at => Article::get_latest_updated(&state.db).await,
                    articles => Article::get_all(&state.db).await,
                },
            )
            .await,
    ));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, "text/xml; charset=utf-8".parse().unwrap());
    response
}

#[derive(Deserialize)]
pub struct LoginQuery {
    next: Option<String>,
}

pub async fn handler_login_get(
    state: State<Arc<AppState>>,
    Query(query): Query<LoginQuery>,
) -> Result<Html<String>, StatusCode> {
    Ok(Html(
        state
            .render_template("login.html", context! {next => query.next})
            .await,
    ))
}

pub async fn handler_login_post(
    mut auth_session: AuthSession<AppState>,
    form_result: Result<Form<Credentials>, axum::extract::rejection::FormRejection>,
) -> impl IntoResponse {
    let mut login_url = "/login".to_string();
    // ensure the credentials are valid otherwise redirect to the login page.
    let credentials = match form_result {
        Ok(Form(credentials)) => credentials,
        Err(_) => return Redirect::to(&login_url).into_response(),
    };
    // authenticate the user.
    let user = match auth_session.authenticate(credentials.clone()).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            if let Some(next) = credentials.next {
                login_url = format!("{}?next={}", login_url, next);
            };

            return Redirect::to(&login_url).into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    // login the user into the session.
    if auth_session.login(&user).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    // redirect to the next page if it exists.
    if let Some(ref next) = credentials.next {
        Redirect::to(next)
    } else {
        Redirect::to("/admin")
    }
    .into_response()
}

pub async fn handler_logout(mut auth_session: AuthSession<AppState>) -> impl IntoResponse {
    match auth_session.logout().await {
        Ok(_) => Redirect::to("/").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn handler_admin(state: State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    Ok(Html(
        state
            .render_template(
                "admin.html",
                context! {
                    pages => Page::get_all(&state.db).await,
                    articles => Article::get_all(&state.db).await,
                },
            )
            .await,
    ))
}

#[derive(Deserialize)]
pub struct ChangePasswordQuery {
    message: Option<String>,
}

pub async fn handler_change_pw_get(
    state: State<Arc<AppState>>,
    Query(change_pw_query): Query<ChangePasswordQuery>,
) -> Result<Html<String>, StatusCode> {
    Ok(Html(
        state
            .render_template(
                "change_pw.html",
                context! {message => change_pw_query.message},
            )
            .await,
    ))
}

#[derive(Deserialize)]
pub struct ChangePasswordForm {
    old_password: String,
    new_password: String,
}

pub async fn handler_change_pw_post(
    state: State<Arc<AppState>>,
    auth_session: AuthSession<AppState>,
    Form(change_pw_form): Form<ChangePasswordForm>,
) -> impl IntoResponse {
    // get the current user.
    let user = match auth_session.user.clone() {
        Some(user) => user,
        // redirect to the login page if the user is not logged in.
        None => return Redirect::to("/login").into_response(),
    };
    // authenticate to check if the username and password are correct.
    let user = match auth_session
        .authenticate(Credentials {
            username: user.username,
            password: change_pw_form.old_password,
            next: None,
        })
        .await
    {
        Ok(Some(user)) => user,
        _ => {
            return redirect_with_message(
                CHANGE_PW_URL,
                "Failed to validate the old password, please try again.",
            )
            .into_response();
        }
    };
    // update the password hash in the database.
    match User::modify_password(
        &state.db,
        &user.username,
        &user.password,
        &password_auth::generate_hash(&change_pw_form.new_password),
    )
    .await
    {
        Ok(_) => Redirect::to("/admin"),
        Err(_) => redirect_with_message(
            CHANGE_PW_URL,
            "Failed to update the password, please try again.",
        ),
    }
    .into_response()
}

fn redirect_with_message(url: &str, message: &str) -> Redirect {
    Redirect::to(format!("{}?message={}", url, message).as_str())
}

#[derive(Debug, Deserialize)]
pub struct EditorPath {
    id: Option<i32>,
}

pub async fn handler_edit_article_get(
    state: State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
) -> Result<Html<String>, StatusCode> {
    let article = match editor_path.id {
        Some(id) => Article::get_by_id(&state.db, id).await,
        None => None,
    };

    Ok(Html(
        state
            .render_template(
                "editor.html",
                context! {
                    article => article,
                    is_page => false,
                },
            )
            .await,
    ))
}

pub async fn handler_edit_page_get(
    state: State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
) -> Result<Html<String>, StatusCode> {
    let page = match editor_path.id {
        Some(id) => Page::get_by_id(&state.db, id).await,
        None => None,
    };

    Ok(Html(
        state
            .render_template(
                "editor.html",
                context! {
                    article => page,
                    is_page => true,
                },
            )
            .await,
    ))
}

#[async_trait]
pub trait Editable: DeserializeOwned + Display {
    type Output;

    fn get_redirect_url(output: &Self::Output) -> String;
    async fn handle_update(&self, state: &AppState, id: i32) -> Result<Self::Output, sqlx::Error>;
    async fn handle_insert(&self, state: &AppState) -> Result<Self::Output, sqlx::Error>;
}

pub async fn handler_edit_post<T: Editable>(
    State(state): State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
    Form(editor_form): Form<T>,
) -> impl IntoResponse {
    let result = match editor_path.id {
        Some(id) => editor_form.handle_update(&state, id).await,
        None => editor_form.handle_insert(&state).await,
    };

    match result {
        Ok(output) => Redirect::to(T::get_redirect_url(&output).as_str()),
        Err(err) => {
            error!("failed processing {}: {:?}", editor_form, err);
            Redirect::to("/admin")
        }
    }
    .into_response()
}

#[derive(Deserialize)]
pub struct ArticleForm {
    title: String,
    tags: String,
    content: String,
}

impl Display for ArticleForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "article title: {}, tags: {}", self.title, self.tags)
    }
}

#[async_trait]
impl Editable for ArticleForm {
    type Output = Article;

    fn get_redirect_url(output: &Self::Output) -> String {
        format!("/article/{}", output.id)
    }

    async fn handle_update(&self, state: &AppState, id: i32) -> Result<Self::Output, sqlx::Error> {
        Article::update(&state.db, id, &self.title, &self.content, &self.tags).await
    }

    async fn handle_insert(&self, state: &AppState) -> Result<Self::Output, sqlx::Error> {
        Article::insert(&state.db, &self.title, &self.content, &self.tags).await
    }
}

#[derive(Deserialize)]
pub struct PageForm {
    title: String,
    content: String,
}

impl Display for PageForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "page title: {}", self.title)
    }
}

#[async_trait]
impl Editable for PageForm {
    type Output = Page;

    fn get_redirect_url(output: &Self::Output) -> String {
        format!("/{}", output.title.to_lowercase())
    }

    async fn handle_update(&self, state: &AppState, id: i32) -> Result<Self::Output, sqlx::Error> {
        Page::update(&state.db, id, &self.title, &self.content).await
    }

    async fn handle_insert(&self, state: &AppState) -> Result<Self::Output, sqlx::Error> {
        Page::insert(&state.db, &self.title, &self.content).await
    }
}

pub async fn handler_delete_article(
    state: State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
) -> impl IntoResponse {
    let redirect = Redirect::to("/admin");
    let id = match editor_path.id {
        Some(id) => id,
        None => return redirect.into_response(),
    };
    match Article::delete(&state.db, id).await {
        Ok(_) => redirect,
        Err(err) => {
            error!("failed deleting article: {:?}", err);
            redirect
        }
    }
    .into_response()
}

pub async fn handler_delete_page(
    state: State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
) -> impl IntoResponse {
    let redirect = Redirect::to("/admin");
    let id = match editor_path.id {
        Some(id) => id,
        None => return redirect.into_response(),
    };
    match Page::delete(&state.db, id).await {
        Ok(_) => redirect,
        Err(err) => {
            error!("failed deleting page: {:?}", err);
            redirect
        }
    }
    .into_response()
}

pub async fn handler_ping() -> impl IntoResponse {
    "pong"
}
