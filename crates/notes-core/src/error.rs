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

    #[error("не создать «{id}»: {reason}")]
    Create { id: String, reason: String },

    #[error("недопустимое имя хранилища «{name}»: {reason}")]
    InvalidVault { name: String, reason: &'static str },

    #[error("нет хранилища «{name}»; есть: {}", names(known))]
    VaultNotFound { name: String, known: Vec<crate::vaults::VaultName> },

    #[error("хранилище «{0}» уже есть")]
    VaultExists(String),

    #[error("{}", vault_required(.0))]
    VaultRequired(Vec<crate::vaults::VaultName>),

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

/// Имена хранилищ через запятую (для сообщений).
fn names(list: &[crate::vaults::VaultName]) -> String {
    if list.is_empty() {
        return "ни одного".into();
    }
    list.iter().map(|n| format!("«{n}»")).collect::<Vec<_>>().join(", ")
}

/// Хранилище не названо: какие есть или как создать первое.
fn vault_required(list: &[crate::vaults::VaultName]) -> String {
    if list.is_empty() {
        "хранилищ нет — создайте: notes vaults new \"Имя\" (или в приложении)".into()
    } else {
        format!("укажите хранилище: --vault \"Имя\"; есть: {}", names(list))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
