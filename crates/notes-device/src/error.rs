//! The errors of device sync: one line each, with what to do.

use std::io;
use std::path::PathBuf;

use notes_store::sync::Error as SyncError;

use crate::account::Account;

/// A device sync error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not signed in: notes sync login <server> --login <name>")]
    NotSignedIn,

    /// The server refuses the saved token (the session ended or was revoked).
    #[error("the session ended: sign in again: notes sync login {server} --login {login}")]
    SessionEnded { server: String, login: String },

    #[error("wrong login or password")]
    WrongLogin,

    #[error("cannot reach the server {server}: {detail}")]
    Unreachable { server: String, detail: String },

    #[error("invalid server address {0:?}: {1}")]
    InvalidServer(String, &'static str),

    #[error("sync of \"{0}\" is already running")]
    Busy(String),

    #[error("vault \"{0}\" exists neither on this device nor on the server")]
    NoSuchVault(String),

    #[error("vault \"{0}\" is not linked: notes sync link --vault \"{0}\"")]
    NotLinked(String),

    #[error("vault \"{0}\" is not on the server: unlink it and link it again")]
    RemoteMissing(String),

    #[error("the folder of vault \"{0}\" is missing")]
    FolderMissing(String),

    /// Any other failure of the sync engine or the hub.
    #[error(transparent)]
    Sync(#[from] SyncError),

    #[error(transparent)]
    Core(#[from] notes_core::Error),

    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{}: not a valid file: {reason}", path.display())]
    Corrupt { path: PathBuf, reason: String },
}

/// The result of device sync.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }

    /// Only a new sign-in helps.
    #[must_use]
    pub fn needs_sign_in(&self) -> bool {
        matches!(self, Self::NotSignedIn | Self::SessionEnded { .. })
    }

    /// The server could not be reached: try again later.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Unreachable { .. })
    }

    /// An engine or hub error as the account sees it.
    pub(crate) fn from_sync(account: &Account, error: SyncError, vault: Option<&str>) -> Self {
        match error {
            SyncError::Unauthorized => {
                Self::SessionEnded { server: account.server.clone(), login: account.login.clone() }
            }
            SyncError::Network(detail) => Self::Unreachable { server: account.server.clone(), detail },
            SyncError::NotFound(_) if vault.is_some() => Self::RemoteMissing(vault.unwrap_or_default().to_owned()),
            other => Self::Sync(other),
        }
    }
}
