use std::{collections::HashSet, fmt::Display, sync::Arc};

use axum::{
    async_trait,
    extract::{rejection::PathRejection, FromRef, FromRequest, FromRequestParts, Request},
    http::request::Parts,
    response::Html,
    RequestExt,
};
use minijinja::context;
use serde::{de::DeserializeOwned, Deserialize};
use tracing::error;

use crate::app::AppState;
use crate::Error;

#[macro_export]
macro_rules! render_template_with_context {
    ($state:expr, $template_name:expr $(,)?) => {
        Html($state.render_template($template_name, context! {}).await)
    };
    ($state:expr, $template_name:expr, $context:expr $(,)?) => {
        Html($state.render_template($template_name, $context).await)
    };
}

// A wrapper for `axum::extract::Path` that can render a 404 page if the path is rejected.
pub struct Path<T>(pub T);

#[async_trait]
impl<S, T> FromRequestParts<S> for Path<T>
where
    Arc<AppState>: FromRef<S>,
    // derive the `FromRequestParts` implementation for `axum::extract::Path` for the type `T`.
    axum::extract::Path<T>: FromRequestParts<S, Rejection = PathRejection>,
    T: Send,
    S: Send + Sync,
{
    type Rejection = Html<String>;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => {
                error!("parse path rejection: {:?}", rejection);
                // get the app state.
                let state = Arc::<AppState>::from_ref(state);
                // render the template.
                Err(render_template_with_context!(
                    state,
                    "error.html",
                    context! {
                        title => "404",
                        message => "Oops, it seems like you've stumbled upon a URL that doesn't exist...",
                    },
                ))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EditorPath {
    pub id: Option<i32>,
}

#[derive(Deserialize)]
pub struct EditorForm {
    pub id: Option<i32>,
    pub title: Option<String>,
    pub tags: Option<String>,
    pub content: Option<String>,
}

#[async_trait]
pub trait Editable: DeserializeOwned + Display {
    fn get_redirect_url(&self) -> String;
    async fn update(&self, db: &sqlx::MySqlPool) -> Result<Self, Error>;
    async fn insert(&self, db: &sqlx::MySqlPool) -> Result<Self, Error>;
    async fn delete(&self, db: &sqlx::MySqlPool) -> Result<(), Error>;
}

pub struct Entity<T> {
    pub entity: T,
    pub is_new: bool,
}

#[async_trait]
impl<S, T> FromRequest<S> for Entity<T>
where
    Arc<AppState>: FromRef<S>,
    T: Editable + From<EditorForm>,
    S: Send + Sync,
{
    type Rejection = Html<String>;

    async fn from_request(mut req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // first get the path from the request to ensure we can determine if the entity is new by checking if the ID is present.
        let path = req
            .extract_parts_with_state::<Path<EditorPath>, S>(state)
            .await?;
        // extract the form from the request, this will consume the request.
        let form = match axum::extract::Form::<EditorForm>::from_request(req, state).await {
            Ok(mut form) => {
                // set the ID from the parsed path
                form.id = path.0.id;
                form
            }
            Err(rejection) => {
                error!("parse form rejection: {:?}", rejection);
                let app_state = Arc::<AppState>::from_ref(state);
                return Err(render_template_with_context!(
                    app_state,
                    "error.html",
                    context! {
                        title => "Error",
                        message => "Oops, it seems like something went wrong during the posting...",
                    }
                ));
            }
        };
        let is_new = form.id.is_none();
        let entity = T::from(form.0);
        Ok(Entity { entity, is_new })
    }
}

// Sort out tags and remove duplicates.
pub fn sort_out_tags(tags: &str) -> String {
    let mut tags = tags
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    tags.sort();
    tags.join(", ")
}

#[cfg(test)]
mod tests {
    use super::sort_out_tags;

    #[test]
    fn test_organize_tags() {
        assert_eq!(
            sort_out_tags("hello,world,foo,bar"),
            "bar, foo, hello, world"
        );
        assert_eq!(
            sort_out_tags("hello,world,foo,bar,"),
            "bar, foo, hello, world"
        );
        assert_eq!(
            sort_out_tags(",hello,world,foo,bar,"),
            "bar, foo, hello, world"
        );
        assert_eq!(
            sort_out_tags(",,,hello,world,foo,bar, ,  ,"),
            "bar, foo, hello, world"
        );
        assert_eq!(
            sort_out_tags("hello,world,world,foo,bar,,foo"),
            "bar, foo, hello, world"
        );
        assert_eq!(sort_out_tags(""), "");
        assert_eq!(sort_out_tags(",,,,"), "");
    }
}
