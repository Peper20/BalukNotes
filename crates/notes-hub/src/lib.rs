//! The storage server online (architecture §9): vault files and their
//! versions, served to the devices that sync, behind a login. It has no Typst
//! and is light enough for a small VPS.
//!
//! It listens on HTTP, on localhost; HTTPS is the job of the reverse proxy in
//! front of it (nginx), which must send `X-Forwarded-Proto` (the `Secure` flag
//! of the cookie) and `Host` or `X-Forwarded-Host` (the check of `Origin`).
//!
//! | Path                                   | What                               |
//! |----------------------------------------|------------------------------------|
//! | `POST /api/login`, `POST /api/logout`, `GET /api/session` | sign-in: [`auth`] |
//! | `GET /api/sync/vaults`, `PUT .../{vault}` | the account's vaults: [`sync`]  |
//! | `GET .../{vault}/changes`              | what changed after `seq`, long polling |
//! | `GET`, `PUT`, `DELETE .../{vault}/files/{*path}` | a file with a version check |
//!
//! Modules: [`auth`] (sign-in and the session check, reusable by any axum
//! server), [`sync`] (the vault API), [`client`] (the device side over HTTP:
//! `HttpRemote`, feature `client`), [`api`] (the wire types), `error`. [`Hub`]
//! puts the server together and [`serve`] runs it.

pub mod api;
pub mod auth;
#[cfg(feature = "client")]
pub mod client;
mod error;
pub mod sync;

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use notes_store::config::hub_dir;
use tokio::net::TcpListener;
use tokio::sync::watch;

pub use auth::Auth;
pub use error::{Error, Result};

/// The default address: localhost, behind the proxy.
pub const DEFAULT_ADDR: &str = "127.0.0.1:8422";

/// The server's state: sign-in and the vaults.
#[derive(Debug, Clone)]
pub struct Hub {
    auth: Arc<Auth>,
    vaults: Arc<sync::Vaults>,
    closing: Arc<watch::Sender<bool>>,
}

impl Hub {
    /// The hub of a data directory: `users.json`, `sessions.json` and the
    /// vaults in `hub/`. A session lives `session_lifetime` after its last use.
    ///
    /// # Errors
    /// A users or sessions file is broken.
    pub fn open(data: &Path, session_lifetime: Duration) -> Result<Self> {
        let closing = Arc::new(watch::Sender::new(false));
        Ok(Self {
            auth: Auth::open(data, session_lifetime)?,
            vaults: sync::Vaults::new(hub_dir(data), closing.clone()),
            closing,
        })
    }

    #[must_use]
    pub fn auth(&self) -> &Arc<Auth> {
        &self.auth
    }

    /// Answers the waiting requests and stops the server of [`serve`].
    pub fn close(&self) {
        self.closing.send_replace(true);
    }

    /// The API: sign-in and the sync routes, all but sign-in behind a session.
    pub fn router(&self) -> Router {
        self.auth.protect(self.vaults.routes(), |_, _| false)
    }
}

/// Serves the hub on `listener` until `shutdown` resolves; then the waiting
/// requests are answered and the open ones finish.
///
/// # Errors
/// The listener fails.
pub async fn serve(
    listener: TcpListener,
    hub: Hub,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let closer = hub.clone();
    tokio::spawn(async move {
        shutdown.await;
        closer.close();
    });
    let mut closing = hub.closing.subscribe();
    axum::serve(listener, hub.router())
        .with_graceful_shutdown(async move {
            let _ = closing.wait_for(|closing| *closing).await;
        })
        .await
}
