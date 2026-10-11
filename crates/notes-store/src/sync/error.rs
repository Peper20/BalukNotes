//! The error of the sync module. The variants fit both the local
//! implementations (a directory) and a future HTTP one.

use std::io;

/// A sync error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The path is not a valid, synced path (see [`super::path`]).
    #[error("invalid path {path:?}: {reason}")]
    InvalidPath { path: String, reason: &'static str },

    /// The text is not a [`super::Base`].
    #[error("invalid base {0:?}: expected \"any\", \"absent\" or a SHA-256 hex hash")]
    InvalidBase(String),

    /// A file is over [`super::MAX_FILE_SIZE`].
    #[error("{path}: {size} bytes is over the limit of {limit} bytes")]
    TooLarge { path: String, size: u64, limit: u64 },

    /// The file does not exist (or is deleted) on the other side.
    #[error("not found: {0}")]
    NotFound(String),

    /// The disk failed.
    #[error(transparent)]
    Io(#[from] io::Error),

    /// A state or manifest file cannot be read.
    #[error("{path}: not a valid file: {reason}")]
    Corrupt { path: String, reason: String },

    /// The other side cannot be reached (HTTP implementations).
    #[error("network error: {0}")]
    Network(String),

    /// The server does not accept the session (HTTP implementations).
    #[error("unauthorized")]
    Unauthorized,

    /// The round would delete a large part of the vault and was stopped
    /// before it deleted anything (see [`super::Deletions`]).
    #[error("{0}")]
    DeletionsHeld(super::Held),

    /// Anything else.
    #[error("{0}")]
    Other(String),
}

/// The result of the sync module.
pub type Result<T> = std::result::Result<T, Error>;
