//! The wire types of the hub API, shared by the server and the client.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The name of the session cookie.
pub const COOKIE: &str = "notes_session";

/// The header of a write: the version it replaces (`notes_store::sync::Base`
/// in its text form).
pub const HEADER_BASE: &str = "x-base";
/// The header of a file read: the SHA-256 hex of the content.
pub const HEADER_HASH: &str = "x-hash";
/// The header of a file read: the `seq` of this version.
pub const HEADER_SEQ: &str = "x-seq";

/// The longest long-poll wait in seconds; a bigger `wait` is cut to it.
pub const MAX_WAIT_SECS: u64 = 30;

/// `POST /api/login`.
#[derive(Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

impl fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginRequest").field("login", &self.login).field("password", &"<hidden>").finish()
    }
}

/// The answer of `POST /api/login`: the account and the session token (also
/// set as a cookie).
#[derive(Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub login: String,
    pub token: String,
}

impl fmt::Debug for LoginResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginResponse").field("login", &self.login).field("token", &"<hidden>").finish()
    }
}

/// The answer of `GET /api/session`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResponse {
    pub login: String,
}

/// A vault of an account on the hub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultInfo {
    pub name: String,
    /// The vault's current change number.
    pub seq: u64,
}

/// An error body. The shape is that of `notes-server`: `errors` is always
/// empty here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub errors: Vec<serde_json::Value>,
}
