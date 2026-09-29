//! Ошибки API и запуск блокирующей работы ядра.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::api::ErrorResponse;

/// Ошибка API: код и сообщение в JSON.
#[derive(Debug)]
pub(crate) struct ApiError(pub StatusCode, pub String);

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
            E::InvalidId { .. } | E::InvalidVault { .. } | E::Setting { .. } => StatusCode::BAD_REQUEST,
            E::VaultExists(_) => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if code == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!("{e}");
        }
        Self(code, e.to_string())
    }
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

/// Блокирующая работа ядра — в отдельном потоке.
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> notes_core::Result<T> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("сбой задачи: {e}")))?
        .map_err(Into::into)
}
