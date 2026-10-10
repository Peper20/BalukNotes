//! The account of a device: the server, the login and the session token, in
//! `<data>/sync/account.json` (mode `0600`). The password is never stored: it
//! is sent once by [`login`], the token it returns is kept.

use std::fmt;
use std::fs;
use std::io;
use std::net::IpAddr;

use notes_hub::client;
use notes_store::fsutil::write_private;
use notes_store::sync::Error as SyncError;
use serde::{Deserialize, Serialize};

use crate::{Error, Paths, Result};

/// A signed-in account.
#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    /// The address of the hub: `https://notes.example`.
    pub server: String,
    pub login: String,
    token: String,
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("server", &self.server)
            .field("login", &self.login)
            .field("token", &"<hidden>")
            .finish()
    }
}

impl Account {
    /// The saved account; `None` if the device is not signed in.
    pub fn load(paths: &Paths) -> Result<Option<Self>> {
        let file = paths.account_file();
        match fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| Error::Corrupt { path: file, reason: e.to_string() }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(file, e)),
        }
    }

    /// The saved account or [`Error::NotSignedIn`].
    pub fn require(paths: &Paths) -> Result<Self> {
        Self::load(paths)?.ok_or(Error::NotSignedIn)
    }

    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    fn save(&self, paths: &Paths) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| Error::Corrupt { path: paths.account_file(), reason: e.to_string() })?;
        write_private(&paths.account_file(), &json).map_err(|e| Error::io(paths.account_file(), e))
    }
}

/// A server address as typed by the user, made complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    /// `https://host[:port][/prefix]`, no trailing slash.
    pub url: String,
    /// Plain `http://` to a host that is not this computer: the password and
    /// the files travel unencrypted. Worth a warning.
    pub insecure: bool,
}

/// `notes.example` is `https://notes.example`; `http://` is allowed (a server
/// on this computer, tests).
pub fn parse_server(input: &str) -> Result<Server> {
    let bad = |reason| Error::InvalidServer(input.to_owned(), reason);
    let text = input.trim();
    let (scheme, rest) = match text.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => ("https".to_owned(), text),
    };
    if scheme != "http" && scheme != "https" {
        return Err(bad("only http:// and https:// are supported"));
    }
    let rest = rest.trim_end_matches('/');
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return Err(bad("no host"));
    }
    if authority.contains('@') || rest.contains(['?', '#']) || rest.chars().any(char::is_whitespace) {
        return Err(bad("expected host[:port][/prefix]"));
    }
    let host = match authority.strip_prefix('[') {
        Some(inner) => inner.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    let loopback = host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    Ok(Server { url: format!("{scheme}://{rest}"), insecure: scheme == "http" && !loopback })
}

/// Signs in and saves the account (replacing the previous one).
pub fn login(paths: &Paths, server: &Server, login: &str, password: &str) -> Result<Account> {
    let token = client::login(&server.url, login, password).map_err(|e| match e {
        SyncError::Unauthorized => Error::WrongLogin,
        SyncError::Network(detail) => Error::Unreachable { server: server.url.clone(), detail },
        other => Error::Sync(other),
    })?;
    let account = Account { server: server.url.clone(), login: login.to_owned(), token };
    account.save(paths)?;
    Ok(account)
}

/// What [`logout`] did.
#[derive(Debug)]
pub struct LoggedOut {
    pub account: Account,
    /// Why the session on the server was not ended (the server is
    /// unreachable, the token was already dead); the local sign-in is gone
    /// anyway.
    pub server_error: Option<String>,
}

/// Ends the session on the server (best effort) and removes the account
/// file. Linked vaults stay linked. `None`: the device was not signed in.
pub fn logout(paths: &Paths) -> Result<Option<LoggedOut>> {
    let Some(account) = Account::load(paths)? else {
        return Ok(None);
    };
    let server_error = match client::logout(&account.server, account.token()) {
        Ok(()) | Err(SyncError::Unauthorized) => None,
        Err(e) => Some(e.to_string()),
    };
    let file = paths.account_file();
    match fs::remove_file(&file) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::io(file, e)),
    }
    Ok(Some(LoggedOut { account, server_error }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(input: &str) -> (String, bool) {
        let s = parse_server(input).unwrap();
        (s.url, s.insecure)
    }

    #[test]
    fn server_addresses() {
        assert_eq!(url("notes.example"), ("https://notes.example".into(), false));
        assert_eq!(url(" https://Notes.example/ "), ("https://Notes.example".into(), false));
        assert_eq!(url("notes.example:8443/hub"), ("https://notes.example:8443/hub".into(), false));
        assert_eq!(url("http://127.0.0.1:8422"), ("http://127.0.0.1:8422".into(), false));
        assert_eq!(url("http://localhost:1"), ("http://localhost:1".into(), false));
        assert_eq!(url("http://[::1]:1/"), ("http://[::1]:1".into(), false));
        assert_eq!(url("http://192.168.1.5:8422"), ("http://192.168.1.5:8422".into(), true));
        assert_eq!(url("HTTP://notes.example"), ("http://notes.example".into(), true));
        for bad in ["", "  ", "ftp://x", "https://", "http://user@host", "host?x=1", "ho st"] {
            assert!(matches!(parse_server(bad), Err(Error::InvalidServer(..))), "{bad:?}");
        }
    }

    #[test]
    fn account_file_is_private_and_hides_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        assert!(Account::load(&paths).unwrap().is_none());
        assert!(matches!(Account::require(&paths), Err(Error::NotSignedIn)));
        let account = Account { server: "http://h:1".into(), login: "ivan".into(), token: "secret".into() };
        account.save(&paths).unwrap();
        let loaded = Account::require(&paths).unwrap();
        assert_eq!((loaded.server.as_str(), loaded.login.as_str(), loaded.token()), ("http://h:1", "ivan", "secret"));
        assert!(!format!("{loaded:?}").contains("secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(paths.account_file()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        fs::write(paths.account_file(), "not json").unwrap();
        assert!(matches!(Account::load(&paths), Err(Error::Corrupt { .. })));
    }
}
