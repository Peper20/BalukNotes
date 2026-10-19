use std::io;
use std::path::PathBuf;

use notes_store::names::{InvalidName, VaultName, quoted_list};
use notes_store::note_id::InvalidId;
use notes_store::vaults;

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

    #[error("no vault \"{name}\"; there are: {}", quoted_list(known))]
    VaultNotFound { name: String, known: Vec<VaultName> },

    #[error("vault \"{0}\" already exists")]
    VaultExists(String),

    #[error("{}", vault_required(.0))]
    VaultRequired(Vec<VaultName>),

    #[error("setting \"{key}\": {reason}")]
    Setting { key: String, reason: String },

    /// The design library itself is broken (for example `css.typ`), not a note.
    #[error("design library: {0}")]
    Library(String),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<InvalidName> for Error {
    fn from(e: InvalidName) -> Self {
        Self::InvalidVault { name: e.name, reason: e.reason }
    }
}

impl From<InvalidId> for Error {
    fn from(e: InvalidId) -> Self {
        Self::InvalidId { id: e.id, reason: e.reason }
    }
}

impl From<vaults::Error> for Error {
    fn from(e: vaults::Error) -> Self {
        match e {
            vaults::Error::NotFound { name, known } => Self::VaultNotFound { name, known },
            vaults::Error::Exists(name) => Self::VaultExists(name),
            vaults::Error::Io { path, source } => Self::Io { path, source },
        }
    }
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

/// No vault named: which ones exist, or how to create the first one.
fn vault_required(list: &[VaultName]) -> String {
    if list.is_empty() {
        "there are no vaults; create one: notes vaults new \"Name\" (or in the app)".into()
    } else {
        format!("name a vault: --vault \"Name\"; there are: {}", quoted_list(list))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_name_error() {
        // The store's error becomes the core one with the same message.
        let err = Error::from(VaultName::new("a/b").unwrap_err());
        assert!(matches!(err, Error::InvalidVault { reason, .. } if reason.starts_with("no characters")));
        assert_eq!(err.to_string(), r#"invalid vault name "a/b": no characters / \ : * ? " < > |"#);
        let found: Result<VaultName> = VaultName::new(" x").map_err(Error::from);
        assert!(found.is_err());
    }
}
