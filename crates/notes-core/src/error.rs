use std::io;
use std::path::PathBuf;

/// Core errors. Typst compile errors are not here: they are a normal result
/// (the note gets fixed) and come as [`crate::diag::Diagnostic`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("note not found: {0}")]
    NotFound(String),

    #[error("invalid note path \"{id}\": {reason}")]
    InvalidId { id: String, reason: &'static str },

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot create \"{id}\": {reason}")]
    Create { id: String, reason: String },

    #[error("cannot rename \"{id}\": {reason}")]
    Rename { id: String, reason: String },

    #[error("invalid vault name \"{name}\": {reason}")]
    InvalidVault { name: String, reason: &'static str },

    #[error("no vault \"{name}\"; there are: {}", names(known))]
    VaultNotFound { name: String, known: Vec<crate::vaults::VaultName> },

    #[error("vault \"{0}\" already exists")]
    VaultExists(String),

    #[error("{}", vault_required(.0))]
    VaultRequired(Vec<crate::vaults::VaultName>),

    #[error("setting \"{key}\": {reason}")]
    Setting { key: String, reason: String },

    /// The design library itself is broken (for example `css.typ`), not a note.
    #[error("design library: {0}")]
    Library(String),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

/// Vault names separated by commas (for messages).
fn names(list: &[crate::vaults::VaultName]) -> String {
    if list.is_empty() {
        return "none".into();
    }
    list.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ")
}

/// No vault named: which ones exist, or how to create the first one.
fn vault_required(list: &[crate::vaults::VaultName]) -> String {
    if list.is_empty() {
        "there are no vaults; create one: notes vaults new \"Name\" (or in the app)".into()
    } else {
        format!("name a vault: --vault \"Name\"; there are: {}", names(list))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
