use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, Response, StatusCode},
    response::{Html, IntoResponse, Redirect},
    Form,
};
use axum_login::AuthSession;
use chrono::Datelike;
use minijinja::context;
use serde::Deserialize;
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
            .into_response()
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
        Ok(_) => Redirect::to("/admin").into_response(),
        Err(_) => {
            return redirect_with_message(
                CHANGE_PW_URL,
                "Failed to update the password, please try again.",
            )
            .into_response()
        }
    }
}

fn redirect_with_message(url: &str, message: &str) -> Redirect {
    Redirect::to(format!("{}?message={}", url, message).as_str())
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
pub struct EditorForm {
    title: String,
    tags: String,
    content: String,
}

pub async fn handler_edit_article_post(
    state: State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
    Form(editor_form): Form<EditorForm>,
) -> impl IntoResponse {
    async fn handle_update(
        state: &AppState,
        id: i32,
        form: &EditorForm,
    ) -> Result<Redirect, Redirect> {
        Article::update(&state.db, id, &form.title, &form.content, &form.tags)
            .await
            .map(|_| Redirect::to(format!("/article/{}", id).as_str()))
            .map_err(|err| {
                error!("failed updating article: {:?}", err);
                Redirect::to("/")
            })
    }

    async fn handle_insert(state: &AppState, form: &EditorForm) -> Result<Redirect, Redirect> {
        Article::insert(&state.db, &form.title, &form.content, &form.tags)
            .await
            .map(|article| Redirect::to(format!("/article/{}", article.id).as_str()))
            .map_err(|err| {
                error!("failed inserting article: {:?}", err);
                Redirect::to("/")
            })
    }

    match editor_path.id {
        Some(id) => handle_update(&state, id, &editor_form)
            .await
            .into_response(),
        None => handle_insert(&state, &editor_form).await.into_response(),
    }
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

pub async fn handler_edit_page_post(
    state: State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
    Form(editor_form): Form<EditorForm>,
) -> impl IntoResponse {
    async fn handle_update(
        state: &AppState,
        id: i32,
        form: &EditorForm,
    ) -> Result<Redirect, Redirect> {
        Page::update(&state.db, id, &form.title, &form.content)
            .await
            .map(|_| Redirect::to(format!("/{}", form.title.to_lowercase()).as_str()))
            .map_err(|err| {
                error!("failed updating page: {:?}", err);
                Redirect::to("/")
            })
    }

    async fn handle_insert(state: &AppState, form: &EditorForm) -> Result<Redirect, Redirect> {
        Page::insert(&state.db, &form.title, &form.content)
            .await
            .map(|page| Redirect::to(format!("/{}", page.title.to_lowercase()).as_str()))
            .map_err(|err| {
                error!("failed inserting page: {:?}", err);
                Redirect::to("/")
            })
    }

    match editor_path.id {
        Some(id) => handle_update(&state, id, &editor_form)
            .await
            .into_response(),
        None => handle_insert(&state, &editor_form).await.into_response(),
    }
}

pub async fn handler_delete_article(
    state: State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
) -> impl IntoResponse {
    let redirect = Redirect::to("/admin");
    match Article::delete(&state.db, editor_path.id.unwrap()).await {
        Ok(_) => redirect.into_response(),
        Err(err) => {
            error!("failed deleting article: {:?}", err);
            redirect.into_response()
        }
    }
}

pub async fn handler_delete_page(
    state: State<Arc<AppState>>,
    Path(editor_path): Path<EditorPath>,
) -> impl IntoResponse {
    let redirect = Redirect::to("/admin");
    match Page::delete(&state.db, editor_path.id.unwrap()).await {
        Ok(_) => redirect.into_response(),
        Err(err) => {
            error!("failed deleting page: {:?}", err);
            redirect.into_response()
        }
    }
}

pub async fn handler_ping() -> impl IntoResponse {
    "pong"
}
