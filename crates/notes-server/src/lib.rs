//! HTTP API and the web client.
//!
//! The same interface for the browser (VPS) and the app (Tauri over
//! localhost). Compiling is blocking work: it goes to `spawn_blocking` so it
//! does not hold an async runtime thread.
//!
//! Responses are compressed (brotli or gzip, whichever the client accepts):
//! note HTML with formulas and figures shrinks 7-13 times (a big book: 3.7 MB ->
//! 0.28 MB brotli, 0.54 MB gzip; the default levels are faster than the "best"
//! ones at almost the same size).
//!
//! The routes and their behaviour: `docs/specs/http-api.md`.
//!
//! Modules by area: `vaults` (vaults opened by the server), `notes` (notes,
//! PDF, warming, deletion), `graph`, `search`, `settings` (and themes),
//! `assets` (the client), `fonts`, `events` (vault changes), `device_sync`
//! (vault sync of this device), `auth` (the sign-in gate of the TCP address,
//! the routes are `notes_hub::auth`); shared - [`AppState`] and `error`.

pub mod api;
#[cfg(not(target_family = "wasm"))]
mod assets;
mod auth;
#[cfg(not(target_family = "wasm"))]
mod device_sync;
mod error;
mod events;
mod fonts;
mod graph;
mod notes;
mod search;
mod settings;
mod vaults;

#[cfg(not(target_family = "wasm"))]
use std::future::IntoFuture;
use std::sync::Arc;

use axum::Router;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use notes_core::settings::SettingsStore;
use tokio::sync::watch;
#[cfg(not(target_family = "wasm"))]
use tower_http::compression::CompressionLayer;
#[cfg(not(target_family = "wasm"))]
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};

pub use vaults::{OpenVault, VaultSet};

/// State shared by the handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub vaults: Arc<VaultSet>,
    pub settings: Arc<SettingsStore>,
    /// Sign-in: when set, the TCP router needs a session (see `auth`).
    #[cfg(not(target_family = "wasm"))]
    pub auth: Option<Arc<notes_hub::Auth>>,
    /// The server is stopping: waiting event requests get their answer.
    pub closing: Arc<watch::Sender<bool>>,
    /// Vault sync of this device (`notes serve` sets it): its workers run
    /// while the server does, and `/api/device/sync` works on it.
    #[cfg(not(target_family = "wasm"))]
    pub device_sync: Option<Arc<notes_device::DeviceSync>>,
}

impl AppState {
    /// State without sign-in. Vault changes reach the vault events once the
    /// watcher is on (`Notes::watch`, done by [`serve`]). Device settings are
    /// applied to the core right away.
    pub fn new(vaults: VaultSet) -> Self {
        vaults.apply_device();
        let settings = vaults.settings().clone();
        Self {
            vaults: Arc::new(vaults),
            settings,
            #[cfg(not(target_family = "wasm"))]
            auth: None,
            closing: Arc::new(watch::Sender::new(false)),
            #[cfg(not(target_family = "wasm"))]
            device_sync: None,
        }
    }

    /// The vault from the path; opened (on a blocking thread) if it is not yet.
    pub(crate) async fn vault(&self, name: String) -> error::ApiResult<Arc<OpenVault>> {
        if let Some(open) = self.vaults.opened(&name) {
            open.touch();
            return Ok(open);
        }
        let vaults = self.vaults.clone();
        let open = error::blocking(move || vaults.get(&name)).await?;
        open.touch();
        Ok(open)
    }

    /// With the sync of this device's vaults (the API `/api/device/sync`; the
    /// background workers run while [`serve`] does).
    #[cfg(not(target_family = "wasm"))]
    #[must_use]
    pub fn with_device_sync(mut self, sync: Option<Arc<notes_device::DeviceSync>>) -> Self {
        self.device_sync = sync;
        self
    }

    /// With sign-in by login and password.
    #[cfg(not(target_family = "wasm"))]
    #[must_use]
    pub fn with_auth(mut self, auth: Option<Arc<notes_hub::Auth>>) -> Self {
        self.auth = auth;
        self
    }
}

/// A CSS answer that may change (themes, fonts): no long caching.
pub(crate) fn css(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

/// The API router. Natively it also serves the client files, device sync,
/// sign-in (with [`AppState::auth`]) and compresses the answers; in the
/// browser build (`wasm32`) it is the API alone, and `GET /api/session`
/// answers 204.
pub fn router(state: AppState) -> Router {
    let app = Router::new()
        .merge(vaults::routes())
        .merge(notes::routes())
        .merge(graph::routes())
        .merge(search::routes())
        .merge(settings::routes())
        .merge(fonts::routes())
        .merge(events::routes());
    #[cfg(not(target_family = "wasm"))]
    let app = app.merge(device_sync::routes());
    // The API answers errors in JSON, also those of the framework; the client
    // files below keep their own answers.
    let app = app.layer(axum::middleware::map_response(error::json_rejections));
    #[cfg(not(target_family = "wasm"))]
    let app = {
        let app = app.merge(assets::routes());
        match state.auth.clone() {
            Some(auth) => auth.protect(app, auth::is_public),
            None => app.merge(auth::no_auth_routes()),
        }
    };
    #[cfg(target_family = "wasm")]
    let app = app.merge(auth::no_auth_routes());
    // WOFF2 is already brotli-compressed: do not compress it twice.
    #[cfg(not(target_family = "wasm"))]
    let app = app.layer(
        CompressionLayer::new().compress_when(DefaultPredicate::new().and(NotForContentType::const_new("font/woff2"))),
    );
    app.with_state(state)
}

/// Stops the sync workers when the server ends, however it ends.
#[cfg(not(target_family = "wasm"))]
struct StopSync(Arc<notes_device::DeviceSync>);

#[cfg(not(target_family = "wasm"))]
impl Drop for StopSync {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Where the server listens.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
pub enum Listen {
    /// TCP (`notes serve --addr`): the browser; with sign-in, if set.
    Tcp(tokio::net::TcpListener),
    /// A Unix socket (`notes serve --socket`): the `notes-app` window. Only the
    /// user has access to the socket, so no sign-in.
    #[cfg(unix)]
    Unix(tokio::net::UnixListener),
}

/// Runs the server on the given sockets until `shutdown` resolves.
#[cfg(not(target_family = "wasm"))]
pub async fn serve(
    listeners: Vec<Listen>,
    state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    // Browser fonts are compressed ahead in the background: otherwise the first
    // page would wait for it (the math font takes ~2 s). Fonts and themes are
    // shared by all vaults (one library).
    let library = state.vaults.library().clone();
    std::thread::spawn(move || library.warm_fonts());
    // Warming and the file watcher run for every open vault.
    state.vaults.start_background();
    // Vault sync: the workers of the linked vaults run until the server stops.
    let _sync = state.device_sync.clone().map(StopSync);
    if let Some(sync) = state.device_sync.clone() {
        let _ = tokio::task::spawn_blocking(move || sync.start()).await;
    }
    // Inactive vaults without requests or waiting events close after a timeout.
    let vaults = state.vaults.clone();
    let closing = state.closing.clone();
    std::thread::spawn(move || vaults.close_idle_forever(&closing));
    let closing = state.closing.clone();
    tokio::spawn(async move {
        shutdown.await;
        closing.send_replace(true);
    });
    let stopped = |closing: &Arc<watch::Sender<bool>>| {
        let mut rx = closing.subscribe();
        async move {
            let _ = rx.wait_for(|c| *c).await;
        }
    };
    let mut servers = tokio::task::JoinSet::new();
    for listen in listeners {
        let stop = stopped(&state.closing);
        match listen {
            Listen::Tcp(l) => {
                servers.spawn(axum::serve(l, router(state.clone())).with_graceful_shutdown(stop).into_future());
            }
            #[cfg(unix)]
            Listen::Unix(l) => {
                let state = AppState { auth: None, ..state.clone() };
                servers.spawn(axum::serve(l, router(state)).with_graceful_shutdown(stop).into_future());
            }
        }
    }
    while let Some(done) = servers.join_next().await {
        done.map_err(std::io::Error::other)??;
    }
    Ok(())
}
