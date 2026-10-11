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
//! | Path                         | What                                         |
//! |------------------------------|----------------------------------------------|
//! | `GET /`, `/v/{vault}/...` (`n/{*id}`, `f/{*path}`, `tags...`, `graph`) | the client (one page, routing in JS) |
//! | `GET /assets/{*path}`        | client files (the `app/dist/assets` build)   |
//! | `GET /api/vaults`            | the vaults and which one to open by default  |
//! | `POST /api/vaults`           | create a vault `{ name }`                    |
//! | `PATCH /api/vaults/{vault}`  | rename `{ name }`                            |
//! | `DELETE /api/vaults/{vault}` | the whole vault to the system trash          |
//! | `GET /api/settings`          | settings schema and values                   |
//! | `PUT /api/settings`          | change settings (partially)                  |
//! | `GET /api/themes`            | themes: name, title, whether dark            |
//! | `GET /api/themes.css`        | CSS variables of the themes                  |
//! | `GET /api/fonts.css`         | `@font-face` for the design fonts (in parts) |
//! | `GET /fonts/{family}/{style}/{part}.woff2` | a font part (WOFF2, a character set) |
//!
//! A vault lives under `/api/vaults/{vault}` (`...` below):
//!
//! | Path                         | What                                         |
//! |------------------------------|----------------------------------------------|
//! | `GET .../notes`              | notes and books                              |
//! | `GET .../folders`            | folders with titles (`_folder.toml`)         |
//! | `GET .../notes/{*id}`        | a note: HTML, headings, links, errors        |
//! | `...?chapter=N`, `...?anchor=` | a book as one chapter (the N-th or the one with the anchor) |
//! | `DELETE .../notes/{*id}`     | a note (a book as a folder) to the trash     |
//! | `GET .../version/{*id}`      | the note version: cheap, no compiling        |
//! | `GET .../links/{*id}`        | links of the note and backlinks to it        |
//! | `GET .../graph`              | the note graph: nodes and edges              |
//! | `POST .../graph/layout`      | the graph by a filter, laid out (`notes_core::vault_graph`) |
//! | `GET .../search?q=&limit=`   | search in the text of all notes              |
//! | `GET .../preview/{*id}?anchor=` | preview of a note or section (no compiling) |
//! | `GET .../pdf/{*id}?theme=`   | the note as PDF (the first theme by default) |
//! | `POST .../warm`              | what to build ahead first (see `notes_core::warm`) |
//! | `GET .../events?after=`      | file changes (long polling, see `events`)    |
//! | `GET .../settings`           | vault settings: result, shared, own          |
//! | `PUT .../settings`           | set own ones (`null` - back to shared)       |
//!
//! Vault sync of this device (`device_sync`, not per vault):
//! `GET /api/device/sync` (status), `POST /api/device/sync/login` and `/logout`,
//! `POST /api/device/sync/vaults/{vault}/link`, `/unlink`, `/now`.
//!
//! Modules by area: `vaults` (vaults opened by the server), `notes` (notes,
//! PDF, warming, deletion), `graph`, `search`, `settings` (and themes),
//! `assets` (the client), `fonts`, `events` (vault changes), `device_sync` (vault sync of this device); shared -
//! [`AppState`] and `error`. With [`AppState::auth`] (`notes serve --auth`) the
//! TCP address needs a sign-in (`auth`): `notes_hub::auth` adds `POST
//! /api/login`, `POST /api/logout` and `GET /api/session`, and only the client
//! page, its files, the themes and the fonts are open. The Unix socket never
//! needs it. Without it `GET /api/session` answers 204.

pub mod api;
mod assets;
mod auth;
mod device_sync;
mod error;
mod events;
mod fonts;
mod graph;
mod notes;
mod search;
mod settings;
mod vaults;

use std::future::IntoFuture;
use std::sync::Arc;

use axum::Router;
use notes_core::settings::SettingsStore;
use tokio::sync::watch;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};

pub use vaults::{OpenVault, VaultSet};

/// State shared by the handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub vaults: Arc<VaultSet>,
    pub settings: Arc<SettingsStore>,
    /// Sign-in: when set, the TCP router needs a session (see `auth`).
    pub auth: Option<Arc<notes_hub::Auth>>,
    /// The server is stopping: waiting event requests get their answer.
    pub closing: Arc<watch::Sender<bool>>,
    /// Vault sync of this device (`notes serve` sets it): its workers run
    /// while the server does, and `/api/device/sync` works on it.
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
            auth: None,
            closing: Arc::new(watch::Sender::new(false)),
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
    #[must_use]
    pub fn with_device_sync(mut self, sync: Option<Arc<notes_device::DeviceSync>>) -> Self {
        self.device_sync = sync;
        self
    }

    /// With sign-in by login and password.
    #[must_use]
    pub fn with_auth(mut self, auth: Option<Arc<notes_hub::Auth>>) -> Self {
        self.auth = auth;
        self
    }
}

pub fn router(state: AppState) -> Router {
    let app = Router::new()
        .merge(assets::routes())
        .merge(vaults::routes())
        .merge(notes::routes())
        .merge(graph::routes())
        .merge(search::routes())
        .merge(settings::routes())
        .merge(fonts::routes())
        .merge(events::routes())
        .merge(device_sync::routes());
    let app = match state.auth.clone() {
        Some(auth) => auth.protect(app, auth::is_public),
        None => app.merge(auth::no_auth_routes()),
    };
    // WOFF2 is already brotli-compressed: do not compress it twice.
    app.layer(
        CompressionLayer::new().compress_when(DefaultPredicate::new().and(NotForContentType::const_new("font/woff2"))),
    )
    .with_state(state)
}

/// Stops the sync workers when the server ends, however it ends.
struct StopSync(Arc<notes_device::DeviceSync>);

impl Drop for StopSync {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Where the server listens.
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
