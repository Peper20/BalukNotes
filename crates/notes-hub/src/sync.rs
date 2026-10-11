//! The sync API over [`HubVault`]: the vaults of an account and their files.
//! Everything needs a session ([`crate::auth`]); an account sees only its own
//! vaults, in `<hub>/<login>/<vault>/`.
//!
//! | Path                                       | What                                  |
//! |--------------------------------------------|---------------------------------------|
//! | `GET /api/sync/vaults`                     | `[{ name, seq }]`                     |
//! | `PUT /api/sync/vaults/{vault}`             | creates an empty vault: 201, or 200 if it exists |
//! | `GET .../{vault}/changes?after=&wait=`     | `Changes` after `seq`, long polling   |
//! | `GET .../{vault}/files/{*path}`            | the bytes; headers `X-Hash`, `X-Seq`  |
//! | `PUT .../{vault}/files/{*path}`            | raw body, header `X-Base` -> `Entry`, or 409 `Conflict` |
//! | `DELETE .../{vault}/files/{*path}`         | header `X-Base` -> `Entry`, or 409 `Conflict` |
//!
//! Errors are `{ error, errors: [] }`: an invalid vault name, path or base 400,
//! an unknown vault or a missing file 404, a file over
//! [`MAX_FILE_SIZE`] 413, the rest 500 (the details go to the log).
//!
//! # Long polling
//!
//! `changes` with `wait` (seconds, at most [`MAX_WAIT_SECS`]) and nothing after
//! `after` waits for a write to that vault and then answers, with the new
//! entries, or empty after the wait. A `tokio::sync::watch` per open vault
//! carries the `seq`; a server stop answers the waiting requests.
//!
//! # Open vaults
//!
//! A [`HubVault`] is opened (and its folder scanned) on the first request and
//! kept for the life of the process, one per (account, vault): it must be the
//! only owner of its folder. The files are read, hashed and written whole in
//! memory, on blocking threads.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{DefaultBodyLimit, Extension, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use notes_store::names::VaultName;
use notes_store::sync::{self, Base, Changes, HubVault, MAX_FILE_SIZE, Outcome};
use serde::Deserialize;
use tokio::sync::watch;

use crate::api::{HEADER_BASE, HEADER_HASH, HEADER_SEQ, MAX_WAIT_SECS, VaultInfo};
use crate::auth::Login;
use crate::error::{ApiError, ApiResult, blocking};

/// A body may be a little over the file limit, so that [`HubVault`] answers
/// "too large" with the sizes.
#[expect(clippy::cast_possible_truncation, reason = "64 MiB fits any usize the server runs on")]
const BODY_LIMIT: usize = MAX_FILE_SIZE as usize + 64 * 1024;

/// An open vault and the notice of its writes.
#[derive(Debug)]
struct OpenVault {
    vault: Arc<HubVault>,
    /// The vault's `seq` after the last write.
    written: watch::Sender<u64>,
}

impl OpenVault {
    /// Tells the waiting requests about a write that reached `seq`.
    fn notify(&self, seq: u64) {
        self.written.send_if_modified(|current| {
            let changed = *current != seq;
            *current = seq;
            changed
        });
    }
}

/// The vaults of all accounts on the disk and the open ones.
#[derive(Debug)]
pub struct Vaults {
    root: PathBuf,
    open: tokio::sync::Mutex<HashMap<(String, String), Arc<OpenVault>>>,
    closing: Arc<watch::Sender<bool>>,
}

impl Vaults {
    /// The vaults under `root` (`<data>/hub`); `closing` becoming `true`
    /// answers the waiting requests.
    #[must_use]
    pub fn new(root: PathBuf, closing: Arc<watch::Sender<bool>>) -> Arc<Self> {
        Arc::new(Self { root, open: tokio::sync::Mutex::new(HashMap::new()), closing })
    }

    /// The routes, for [`crate::auth::Auth::protect`].
    pub fn routes(self: &Arc<Self>) -> Router {
        let limit = DefaultBodyLimit::max(BODY_LIMIT);
        Router::new()
            .route("/api/sync/vaults", get(list))
            .route("/api/sync/vaults/{vault}", put(create))
            .route("/api/sync/vaults/{vault}/changes", get(changes))
            .route("/api/sync/vaults/{vault}/files/{*path}", get(read).put(write).delete(remove).layer(limit))
            .with_state(self.clone())
    }

    /// The open vault of an account; opens it (on a blocking thread). A vault
    /// that is not on the disk is [`sync::Error::NotFound`] unless `create`.
    /// The flag says the folder did not exist.
    async fn vault(&self, login: &str, name: &str, create: bool) -> ApiResult<(Arc<OpenVault>, bool)> {
        let name = VaultName::new(name)?;
        let key = (login.to_owned(), name.as_str().to_owned());
        // One lock for the whole opening: two requests must not open one folder.
        let mut open = self.open.lock().await;
        if let Some(vault) = open.get(&key) {
            return Ok((vault.clone(), false));
        }
        let dir = self.root.join(login).join(name.as_str());
        let (vault, created) = blocking(move || -> sync::Result<_> {
            let exists = dir.is_dir();
            if !exists && !create {
                return Err(sync::Error::NotFound(format!("vault \"{name}\"")));
            }
            Ok((HubVault::open(dir)?, !exists))
        })
        .await?;
        let opened = Arc::new(OpenVault { written: watch::Sender::new(vault.seq()), vault: Arc::new(vault) });
        open.insert(key, opened.clone());
        Ok((opened, created))
    }

    /// The names of an account's vaults on the disk.
    async fn names(&self, login: &str) -> ApiResult<Vec<String>> {
        let dir = self.root.join(login);
        blocking(move || -> std::io::Result<_> {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            let mut names = Vec::new();
            for entry in entries {
                let entry = entry?;
                if let Some(name) = entry.file_name().to_str()
                    && entry.file_type()?.is_dir()
                    && VaultName::new(name).is_ok()
                {
                    names.push(name.to_owned());
                }
            }
            names.sort();
            Ok(names)
        })
        .await
    }
}

async fn list(
    State(s): State<Arc<Vaults>>,
    Extension(Login(login)): Extension<Login>,
) -> ApiResult<Json<Vec<VaultInfo>>> {
    let mut vaults = Vec::new();
    for name in s.names(&login).await? {
        let (open, _) = s.vault(&login, &name, false).await?;
        vaults.push(VaultInfo { name, seq: open.vault.seq() });
    }
    Ok(Json(vaults))
}

async fn create(
    State(s): State<Arc<Vaults>>,
    Extension(Login(login)): Extension<Login>,
    Path(name): Path<String>,
) -> ApiResult<(StatusCode, Json<VaultInfo>)> {
    let (open, created) = s.vault(&login, &name, true).await?;
    let code = if created { StatusCode::CREATED } else { StatusCode::OK };
    Ok((code, Json(VaultInfo { name, seq: open.vault.seq() })))
}

#[derive(Debug, Deserialize)]
struct ChangesQuery {
    #[serde(default)]
    after: u64,
    #[serde(default)]
    wait: u64,
}

async fn changes(
    State(s): State<Arc<Vaults>>,
    Extension(Login(login)): Extension<Login>,
    Path(name): Path<String>,
    Query(q): Query<ChangesQuery>,
) -> ApiResult<Json<Changes>> {
    let (open, _) = s.vault(&login, &name, false).await?;
    // Subscribe before looking: a write between the look and the wait is seen.
    let mut written = open.written.subscribe();
    let mut closing = s.closing.subscribe();
    let after = q.after;
    let look = |open: &Arc<OpenVault>| {
        let vault = open.vault.clone();
        blocking(move || -> sync::Result<_> { Ok(vault.changes(after)) })
    };
    let mut found = look(&open).await?;
    let wait = Duration::from_secs(q.wait.min(MAX_WAIT_SECS));
    if found.entries.is_empty() && !wait.is_zero() && !*closing.borrow_and_update() {
        tokio::select! {
            _ = written.changed() => {}
            _ = closing.changed() => {}
            () = tokio::time::sleep(wait) => {}
        }
        found = look(&open).await?;
    }
    Ok(Json(found))
}

/// The `X-Base` header: required.
fn base(headers: &HeaderMap) -> ApiResult<Base> {
    let Some(value) = headers.get(HEADER_BASE) else {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!("header {HEADER_BASE} is required: any, absent or a hash"),
        ));
    };
    Ok(value.to_str().unwrap_or_default().parse()?)
}

/// An entry as the answer of a write, or the conflict as 409.
fn outcome(outcome: Outcome) -> Response {
    match outcome {
        Ok(entry) => Json(entry).into_response(),
        Err(conflict) => (StatusCode::CONFLICT, Json(conflict)).into_response(),
    }
}

async fn read(
    State(s): State<Arc<Vaults>>,
    Extension(Login(login)): Extension<Login>,
    Path((name, path)): Path<(String, String)>,
) -> ApiResult<Response> {
    let (open, _) = s.vault(&login, &name, false).await?;
    let (entry, data) = blocking(move || open.vault.read(&path)).await?;
    let mut response = data.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    if let Some(hash) = entry.hash.as_deref().and_then(|h| HeaderValue::from_str(h).ok()) {
        headers.insert(HEADER_HASH, hash);
    }
    headers.insert(HEADER_SEQ, HeaderValue::from(entry.seq));
    Ok(response)
}

async fn write(
    State(s): State<Arc<Vaults>>,
    Extension(Login(login)): Extension<Login>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<Response> {
    let base = base(&headers)?;
    let body = body.map_err(|e| ApiError(e.status(), e.body_text()))?;
    let (open, _) = s.vault(&login, &name, false).await?;
    let written = open.clone();
    let (result, seq) = blocking(move || -> sync::Result<_> {
        let result = written.vault.write(&path, &base, &body)?;
        Ok((result, written.vault.seq()))
    })
    .await?;
    if result.is_ok() {
        open.notify(seq);
    }
    Ok(outcome(result))
}

async fn remove(
    State(s): State<Arc<Vaults>>,
    Extension(Login(login)): Extension<Login>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let base = base(&headers)?;
    let (open, _) = s.vault(&login, &name, false).await?;
    let deleted = open.clone();
    let (result, seq) = blocking(move || -> sync::Result<_> {
        let result = deleted.vault.delete(&path, &base)?;
        Ok((result, deleted.vault.seq()))
    })
    .await?;
    if result.is_ok() {
        open.notify(seq);
    }
    Ok(outcome(result))
}
