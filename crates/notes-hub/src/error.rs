//! Errors: the library's [`Error`] and the API error answered to a client.

use std::io;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use notes_store::names::InvalidName;
use notes_store::sync;
use notes_store::{accounts, sessions};

use crate::api::ErrorResponse;

/// An error of starting the hub or of a hub operation outside a request.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Accounts(#[from] accounts::Error),

    #[error(transparent)]
    Sessions(#[from] sessions::Error),

    #[error(transparent)]
    Sync(#[from] sync::Error),

    #[error(transparent)]
    Name(#[from] InvalidName),

    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The result of the hub library.
pub type Result<T> = std::result::Result<T, Error>;

/// An API error: a status code and a message in JSON.
#[derive(Debug)]
pub(crate) struct ApiError(pub StatusCode, pub String);

impl ApiError {
    pub(crate) fn internal(e: &dyn std::fmt::Display) -> Self {
        tracing::error!("{e}");
        // The details (paths) stay in the log.
        Self(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorResponse { error: self.1, errors: Vec::new() })).into_response()
    }
}

impl From<sync::Error> for ApiError {
    fn from(e: sync::Error) -> Self {
        use sync::Error as E;
        match e {
            E::InvalidPath { .. } | E::InvalidBase(_) => Self(StatusCode::BAD_REQUEST, e.to_string()),
            E::NotFound(_) => Self(StatusCode::NOT_FOUND, e.to_string()),
            E::TooLarge { .. } => Self(StatusCode::PAYLOAD_TOO_LARGE, e.to_string()),
            _ => Self::internal(&e),
        }
    }
}

impl From<InvalidName> for ApiError {
    fn from(e: InvalidName) -> Self {
        Self(StatusCode::BAD_REQUEST, e.to_string())
    }
}

impl From<io::Error> for ApiError {
    fn from(e: io::Error) -> Self {
        Self::internal(&e)
    }
}

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        Self::internal(&e)
    }
}

impl From<sessions::Error> for ApiError {
    fn from(e: sessions::Error) -> Self {
        Self::internal(&e)
    }
}

pub(crate) type ApiResult<T> = std::result::Result<T, ApiError>;

/// Blocking work (disk, Argon2) on a blocking thread.
pub(crate) async fn blocking<T, E>(f: impl FnOnce() -> std::result::Result<T, E> + Send + 'static) -> ApiResult<T>
where
    T: Send + 'static,
    E: Into<ApiError> + Send + 'static,
{
    tokio::task::spawn_blocking(f).await.map_err(|e| ApiError::internal(&e))?.map_err(Into::into)
}
