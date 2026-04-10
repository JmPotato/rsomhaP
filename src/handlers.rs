use std::{collections::HashMap, sync::Arc, sync::LazyLock};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header::CONTENT_TYPE, Response, StatusCode},
    response::{Html, IntoResponse, Redirect},
    Form,
};
use axum_login::AuthSession;
use chrono::Datelike;
use minijinja::context;
use rand::{thread_rng, Rng};
use serde::Deserialize;
use tracing::{error, info};

use crate::{
    app::AppState,
    auth::Credentials,
    models::{Article, Page, Tags, User},
    render_template_with_context,
    utils::{Editable, EditorPath, Entity, Path},
    Error,
};

const ADMIN_URL: &str = "/admin";
const CHANGE_PW_URL: &str = "/admin/change_password";

static MARKDOWN_IMAGE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\!\[.*?\]\((.*?)\)").unwrap());

/// Validate that a redirect target is a safe relative path.
/// Rejects absolute URLs (http://, https://, //), data: URIs, etc.
fn is_safe_redirect(url: &str) -> bool {
    url.starts_with('/') && !url.starts_with("//")
}

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

    Ok(render_template_with_context!(
        state,
        "home.html",
        context! {
            articles => articles,
            total_article_count => total_article_count,
            page_num => page_num,
            max_page => max_page,
        },
    ))
}

pub async fn handler_article(
    State(state): State<Arc<AppState>>,
    Path(id_or_slug): Path<String>,
    auth_session: AuthSession<AppState>,
) -> Result<Html<String>, StatusCode> {
    if let Some(article) = if let Ok(id) = id_or_slug.parse::<i32>() {
        info!("try to get article by id: {}", id);
        Article::get_by_id(&state.db, id).await
    } else {
        info!("try to get article by slug: {}", id_or_slug);
        Article::get_by_slug(&state.db, &id_or_slug).await
    } {
        return Ok(render_template_with_context!(
            state,
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
                    let mut image_urls: Vec<String> = vec![];
                    for (_, [image_url]) in MARKDOWN_IMAGE_RE.captures_iter(&article.content).map(|c| c.extract()) {
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
    if articles_by_year.is_empty() {
        return handler_404(State(state)).await;
    }
    // sort `years` in descending order.
    years.sort_by(|a, b| b.cmp(a));
    Ok(render_template_with_context!(
        state,
        "tag.html",
        context! {
            tag => tag,
            years => years,
            articles_by_year => articles_by_year,
        },
    ))
}

pub async fn handler_404(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    handler_error(
        State(state),
        Some("404".to_string()),
        Some("Oops, page not found...".to_string()),
    )
    .await
}

pub async fn handler_error(
    State(state): State<Arc<AppState>>,
    title: Option<String>,
    message: Option<String>,
) -> Result<Html<String>, StatusCode> {
    Ok(render_template_with_context!(
        state,
        "error.html",
        context! {
            title => title.unwrap_or("Error".to_string()),
            message => message.unwrap_or("Oops, it seems like something went wrong...".to_string()),
        },
    ))
}
pub async fn handler_articles(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
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

    Ok(render_template_with_context!(
        state,
        "articles.html",
        context! {
            years => years,
            articles_by_year => articles_by_year,
        },
    ))
}

pub async fn handler_tags(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    Ok(render_template_with_context!(
        state,
        "tags.html",
        context! {tags => Tags::get_all_with_count(&state.db).await},
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

    Ok(render_template_with_context!(
        state,
        "page.html",
        context! {page => page},
    ))
}

pub async fn handler_feed(State(state): State<Arc<AppState>>) -> Response<Body> {
    let mut response = Response::new(Body::new(state.get_feed_cache().await));
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
    State(state): State<Arc<AppState>>,
    Query(query): Query<LoginQuery>,
) -> Result<Html<String>, StatusCode> {
    Ok(render_template_with_context!(
        state,
        "login.html",
        context! {next => query.next},
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
            if let Some(ref next) = credentials.next {
                if is_safe_redirect(next) {
                    login_url = format!("{}?next={}", login_url, next);
                }
            };

            return Redirect::to(&login_url).into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    // login the user into the session.
    if auth_session.login(&user).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    // redirect to the next page if it exists and is a safe relative path.
    if let Some(ref next) = credentials.next {
        if is_safe_redirect(next) {
            Redirect::to(next)
        } else {
            Redirect::to(ADMIN_URL)
        }
    } else {
        Redirect::to(ADMIN_URL)
    }
    .into_response()
}

pub async fn handler_logout(mut auth_session: AuthSession<AppState>) -> impl IntoResponse {
    match auth_session.logout().await {
        Ok(_) => Redirect::to("/").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
pub struct AdminQuery {
    message: Option<String>,
}

pub async fn handler_admin(
    State(state): State<Arc<AppState>>,
    Query(admin_query): Query<AdminQuery>,
) -> Result<Html<String>, StatusCode> {
    Ok(render_template_with_context!(
        state,
        "admin.html",
        context! {
            message => admin_query.message,
            pages => Page::get_all(&state.db).await,
            articles => Article::get_all(&state.db).await,
        },
    ))
}

#[derive(Deserialize)]
pub struct ChangePasswordQuery {
    message: Option<String>,
}

pub async fn handler_change_pw_get(
    State(state): State<Arc<AppState>>,
    Query(change_pw_query): Query<ChangePasswordQuery>,
) -> Result<Html<String>, StatusCode> {
    Ok(render_template_with_context!(
        state,
        "change_pw.html",
        context! {message => change_pw_query.message},
    ))
}

#[derive(Deserialize)]
pub struct ChangePasswordForm {
    old_password: String,
    new_password: String,
}

pub async fn handler_change_pw_post(
    State(state): State<Arc<AppState>>,
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
        Ok(_) => Redirect::to(ADMIN_URL),
        Err(_) => redirect_with_message(
            CHANGE_PW_URL,
            "Failed to update the password, please try again.",
        ),
    }
    .into_response()
}

fn build_message_url(url: &str, message: &str) -> String {
    let encoded: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("message", message)
        .finish();
    format!("{}?{}", url, encoded)
}

fn redirect_with_message(url: &str, message: &str) -> Redirect {
    Redirect::to(build_message_url(url, message).as_str())
}

pub async fn handler_edit_article_get(
    State(state): State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
) -> Result<Html<String>, StatusCode> {
    let article = match editor_path.id {
        Some(id) => Article::get_by_id(&state.db, id).await,
        None => None,
    };

    Ok(render_template_with_context!(
        state,
        "editor.html",
        context! {
            article => article,
            is_page => false,
        },
    ))
}

pub async fn handler_edit_page_get(
    State(state): State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
) -> Result<Html<String>, StatusCode> {
    let page = match editor_path.id {
        Some(id) => Page::get_by_id(&state.db, id).await,
        None => None,
    };

    Ok(render_template_with_context!(
        state,
        "editor.html",
        context! {
            article => page,
            is_page => true,
        },
    ))
}

pub async fn handler_edit_post<T: Editable>(
    State(state): State<Arc<AppState>>,
    Entity { entity, is_new }: Entity<T>,
) -> impl IntoResponse {
    let result = if is_new {
        info!("inserting {}", entity);
        entity.insert(&state.db).await
    } else {
        info!("updating {}", entity);
        entity.update(&state.db).await
    };

    match result {
        Ok(output) => {
            state.refresh_feed_cache(false).await;
            state.refresh_page_titles_cache().await;
            Redirect::to(T::get_redirect_url(&output).as_str())
        }
        Err(err) => {
            error!("failed processing {}: {:?}", entity, err);
            match err {
                Error::PageTitleExists(title) => redirect_with_message(
                    ADMIN_URL,
                    &format!(
                        "Page with title '{}' (whose URL is also '/{}') already exists.",
                        title,
                        title.to_lowercase()
                    ),
                ),
                _ => redirect_with_message(
                    ADMIN_URL,
                    "Failed to save the changes, please try again.",
                ),
            }
        }
    }
    .into_response()
}

pub async fn handler_delete_post<T: Editable>(
    State(state): State<Arc<AppState>>,
    Entity { entity, .. }: Entity<T>,
) -> impl IntoResponse {
    info!("deleting {}", entity);
    match entity.delete(&state.db).await {
        Ok(()) => {
            state.refresh_feed_cache(true).await;
            state.refresh_page_titles_cache().await;
            Redirect::to(ADMIN_URL)
        }
        Err(err) => {
            error!("failed deleting {}: {:?}", entity, err);
            redirect_with_message(ADMIN_URL, "Failed to delete, please try again.")
        }
    }
    .into_response()
}

pub async fn handler_ping(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Check if the database connection is alive.
    if state.db.is_closed() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database connection is closed".to_string(),
        );
    }
    // Check if the database read can be performed.
    if let Err(err) = User::try_check_initialization(&state.db).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read from the database: {:?}", err),
        );
    }
    (StatusCode::OK, "pong".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_safe_redirect ---

    #[test]
    fn test_safe_redirect_relative_paths() {
        assert!(is_safe_redirect("/admin"));
        assert!(is_safe_redirect("/admin/edit/article/1"));
        assert!(is_safe_redirect("/"));
    }

    #[test]
    fn test_safe_redirect_rejects_absolute_urls() {
        assert!(!is_safe_redirect("https://evil.com"));
        assert!(!is_safe_redirect("http://evil.com"));
        assert!(!is_safe_redirect("ftp://evil.com"));
    }

    #[test]
    fn test_safe_redirect_rejects_protocol_relative_urls() {
        // //evil.com is a valid protocol-relative URL that browsers resolve against the current scheme.
        assert!(!is_safe_redirect("//evil.com"));
        assert!(!is_safe_redirect("//evil.com/path"));
    }

    #[test]
    fn test_safe_redirect_rejects_other_schemes() {
        assert!(!is_safe_redirect("javascript:alert(1)"));
        assert!(!is_safe_redirect("data:text/html,<h1>hi</h1>"));
    }

    #[test]
    fn test_safe_redirect_rejects_bare_strings() {
        assert!(!is_safe_redirect("evil.com"));
        assert!(!is_safe_redirect(""));
    }

    // --- build_message_url ---

    #[test]
    fn test_build_message_url_plain() {
        let url = build_message_url("/admin", "success");
        assert_eq!(url, "/admin?message=success");
    }

    #[test]
    fn test_build_message_url_encodes_special_chars() {
        let url = build_message_url("/admin", "hello world&foo=bar");
        // Spaces become +, & and = are percent-encoded in form_urlencoded.
        assert!(url.starts_with("/admin?message="));
        assert!(!url.contains("&foo="), "raw & should be encoded");
        // Decode and verify the message round-trips.
        let query = url.strip_prefix("/admin?").unwrap();
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "message");
        assert_eq!(pairs[0].1, "hello world&foo=bar");
    }

    #[test]
    fn test_build_message_url_encodes_unicode() {
        let url = build_message_url("/admin", "页面已存在");
        let query = url.strip_prefix("/admin?").unwrap();
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(pairs[0].1, "页面已存在");
    }

    #[test]
    fn test_build_message_url_encodes_hash() {
        // '#' would truncate the URL if not encoded.
        let url = build_message_url("/admin", "error #123");
        assert!(!url.contains('#'), "# should be percent-encoded");
        let query = url.strip_prefix("/admin?").unwrap();
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(pairs[0].1, "error #123");
    }
}
