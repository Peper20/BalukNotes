//! The rules of the browser server (`notes serve`) that `notes service
//! install` checks before it writes the unit: the defaults and the sign-in
//! requirements of the TCP address. Also what every server of the client page
//! shares (`notes serve`, `notes hub serve --web`): the CSP and the caching of
//! built files.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;
use std::time::Duration;

use crate::config::{sessions_file, users_file};
use crate::{accounts, sessions};

/// Default address of `notes serve`.
pub const DEFAULT_ADDR: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8421));

/// Default lifetime of a sign-in session without requests, in days.
pub const DEFAULT_SESSION_DAYS: u64 = 30;

/// The CSP of the client page and its files: strict, no inline or eval scripts.
///
/// No unsafe-inline/-eval: scripts come only from files; styles from files and
/// inline (MathML `<style>`); style attributes are allowed by default.
/// `'wasm-unsafe-eval'`: compiling the WebAssembly core of the browser client,
/// not `eval` of JS. The worker of the core is a file of `'self'`
/// (`worker-src` falls back to `script-src`). Images: self and `data:` (SVG
/// inserts from Typst). Fonts: self. No dev-server worker (the servers do not
/// use Vite).
pub const CLIENT_CSP: &str = concat!(
    "default-src 'self';",
    "script-src 'self' 'wasm-unsafe-eval';",
    "object-src 'none';",
    "base-uri 'none';",
    "img-src 'self' data: blob:;",
    "style-src 'self' 'unsafe-inline';",
    "font-src 'self';",
    "connect-src 'self';",
    "frame-ancestors 'self'"
);

/// `name-XXXXXXXX.ext`: Vite puts 8 characters of the content hash in the name,
/// so such a build file never changes and may be cached for good.
#[must_use]
pub fn is_hashed(path: &str) -> bool {
    let stem = path.rsplit_once('.').map_or(path, |(s, _)| s);
    stem.rsplit_once('-')
        .is_some_and(|(_, h)| h.len() == 8 && h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "{0} is not a localhost address: add --auth (sign-in by login and password; accounts: notes users add <login>). The server speaks HTTP only: HTTPS is the job of a reverse proxy in front of it"
    )]
    NetworkWithoutAuth(SocketAddr),
    #[error("--auth protects the TCP address: add --addr")]
    AuthWithoutAddr,
    #[error("no users: create one: notes users add <login>")]
    NoUsers,
    #[error(transparent)]
    Accounts(#[from] accounts::Error),
    #[error(transparent)]
    Sessions(#[from] sessions::Error),
}

/// The rule of the TCP address: other than localhost, it needs sign-in (secure
/// by default); sign-in is for the TCP address only.
///
/// # Errors
/// The flags break the rule; the message says what to add.
pub fn check_sign_in(addr: Option<SocketAddr>, auth: bool) -> Result<(), Error> {
    match addr {
        Some(addr) if !auth && !addr.ip().is_loopback() => Err(Error::NetworkWithoutAuth(addr)),
        None if auth => Err(Error::AuthWithoutAddr),
        _ => Ok(()),
    }
}

/// The sign-in files of the data directory open and there is at least one account.
///
/// # Errors
/// A file is broken, or there are no accounts.
pub fn check_accounts(data: &Path, session_lifetime: Duration) -> Result<(), Error> {
    sessions::Sessions::open(sessions_file(data), session_lifetime)?;
    if accounts::Accounts::open(users_file(data))?.is_empty()? {
        return Err(Error::NoUsers);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcp_address_rules() {
        let addr = |s: &str| Some(s.parse::<SocketAddr>().unwrap());
        // Localhost works as it did, with or without sign-in.
        assert!(check_sign_in(addr("127.0.0.1:8421"), false).is_ok());
        assert!(check_sign_in(addr("[::1]:8421"), false).is_ok());
        assert!(check_sign_in(addr("127.0.0.1:8421"), true).is_ok());
        // Any other address without --auth is refused, and the error says what to add.
        for other in ["0.0.0.0:8421", "192.168.1.5:8421", "[::]:8421"] {
            let e = check_sign_in(addr(other), false).unwrap_err().to_string();
            assert!(e.contains(other) && e.contains("add --auth") && e.contains("HTTPS"), "{e}");
            assert!(check_sign_in(addr(other), true).is_ok());
        }
        // Only the socket: no TCP address to protect.
        assert!(check_sign_in(None, false).is_ok());
        assert!(check_sign_in(None, true).unwrap_err().to_string().contains("--addr"));
    }

    #[test]
    fn hashed_names() {
        assert!(is_hashed("index-CX38oHQo.js"));
        assert!(is_hashed("assets/core-a_b-9xYzAbC1.css"));
        assert!(!is_hashed("theme.js"));
        assert!(!is_hashed("icon-small.svg"));
        assert!(!is_hashed("config.js"));
    }

    #[test]
    fn csp_runs_the_wasm_core_without_js_eval() {
        assert!(CLIENT_CSP.contains("script-src 'self' 'wasm-unsafe-eval';"));
        assert!(!CLIENT_CSP.contains("'unsafe-eval'"));
        assert!(!CLIENT_CSP.contains("worker-src"));
    }

    #[test]
    fn sign_in_needs_an_account() {
        let data = tempfile::tempdir().unwrap();
        let day = Duration::from_hours(24);
        let e = check_accounts(data.path(), day).unwrap_err();
        assert_eq!(e.to_string(), "no users: create one: notes users add <login>");
        accounts::Accounts::open(users_file(data.path())).unwrap().add("ann", "correct-horse-9").unwrap();
        assert!(check_accounts(data.path(), day).is_ok());
    }
}
