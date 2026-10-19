//! The rules of the browser server (`notes serve`) that `notes service
//! install` checks before it writes the unit: the defaults and the sign-in
//! requirements of the TCP address.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;
use std::time::Duration;

use crate::config::{sessions_file, users_file};
use crate::{accounts, sessions};

/// Default address of `notes serve`.
pub const DEFAULT_ADDR: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8421));

/// Default lifetime of a sign-in session without requests, in days.
pub const DEFAULT_SESSION_DAYS: u64 = 30;

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
    fn sign_in_needs_an_account() {
        let data = tempfile::tempdir().unwrap();
        let day = Duration::from_hours(24);
        let e = check_accounts(data.path(), day).unwrap_err();
        assert_eq!(e.to_string(), "no users: create one: notes users add <login>");
        accounts::Accounts::open(users_file(data.path())).unwrap().add("ann", "correct-horse-9").unwrap();
        assert!(check_accounts(data.path(), day).is_ok());
    }
}
