//! API errors and running blocking core work.

use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::api::ErrorResponse;

/// An API error: a status code and a message in JSON.
#[derive(Debug)]
pub(crate) struct ApiError(pub StatusCode, pub String);

/// What a client is told about an error of ours: the details (file paths) stay
/// in the log.
const INTERNAL_ERROR: &str = "internal error";

/// The longest framework rejection text read to put it into JSON.
const REJECTION_LIMIT: usize = 64 * 1024;

impl ApiError {
    /// A 500: the error goes to the log, the client gets a generic text.
    pub(crate) fn internal(e: &dyn std::fmt::Display) -> Self {
        tracing::error!("{e}");
        Self(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorResponse { error: self.1, errors: Vec::new() })).into_response()
    }
}

impl From<notes_core::Error> for ApiError {
    fn from(e: notes_core::Error) -> Self {
        use notes_core::Error as E;
        let code = match e {
            E::NotFound(_) | E::VaultNotFound { .. } => StatusCode::NOT_FOUND,
            E::InvalidId { .. } | E::InvalidVault { .. } | E::Setting { .. } | E::Rename { .. } => {
                StatusCode::BAD_REQUEST
            }
            E::VaultExists(_) => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if code == StatusCode::INTERNAL_SERVER_ERROR {
            return Self::internal(&e);
        }
        Self(code, e.to_string())
    }
}

impl From<notes_store::note_id::InvalidId> for ApiError {
    fn from(e: notes_store::note_id::InvalidId) -> Self {
        notes_core::Error::from(e).into()
    }
}

/// Turns the plain-text 4xx answers of the framework (a body or a query it
/// cannot read, a method the route lacks) into the JSON of [`ApiError`]
/// (API-1, API-2). A JSON answer passes as it is.
pub(crate) async fn json_rejections(response: Response) -> Response {
    let status = response.status();
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if !status.is_client_error() || is_json {
        return response;
    }
    let (parts, body) = response.into_parts();
    let text = match axum::body::to_bytes(body, REJECTION_LIMIT).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).trim().to_owned(),
        Err(_) => String::new(),
    };
    let text = if text.is_empty() { status.canonical_reason().unwrap_or("request refused").to_owned() } else { text };
    let mut json = ApiError(status, text).into_response();
    // `Allow`, `Retry-After` and the like stay; the body headers are the old ones.
    for (name, value) in &parts.headers {
        if name != header::CONTENT_TYPE && name != header::CONTENT_LENGTH {
            json.headers_mut().append(name.clone(), value.clone());
        }
    }
    json
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

/// Blocking core work, on a blocking thread.
#[cfg(not(target_family = "wasm"))]
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> notes_core::Result<T> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(&format_args!("task failed: {e}")))?
        .map_err(Into::into)
}

/// Blocking core work, inline: the browser build has one thread and no
/// blocking pool (a build blocks the worker either way).
#[cfg(target_family = "wasm")]
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> notes_core::Result<T> + Send + 'static,
) -> ApiResult<T> {
    f().map_err(Into::into)
}
