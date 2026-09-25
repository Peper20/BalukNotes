use std::io;
use std::path::PathBuf;

/// Ошибки ядра. Ошибки компиляции Typst сюда не входят: это нормальный
/// результат работы (заметку правят), они приходят как [`crate::diag::Diagnostic`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("заметка не найдена: {0}")]
    NotFound(String),

    #[error("недопустимый путь заметки «{id}»: {reason}")]
    InvalidId { id: String, reason: &'static str },

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("настройка «{key}»: {reason}")]
    Setting { key: String, reason: String },

    /// Сломана сама библиотека оформления (например, `css.typ`), а не заметка.
    #[error("библиотека оформления: {0}")]
    Library(String),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
