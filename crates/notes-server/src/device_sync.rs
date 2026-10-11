//! Vault sync of this device, for the client: sign in to a storage server,
//! link vaults to it, see how they are doing (`notes_device`, architecture
//! §9). Device-level, not per vault, and apart from the hub's own
//! `/api/sync/...` routes, which this process does not serve.
//!
//! | Path                                          | What                          |
//! |-----------------------------------------------|-------------------------------|
//! | `GET /api/device/sync`                        | [`SyncStatus`]: the account, every vault |
//! | `POST /api/device/sync/login`                 | `{ server, login, password }` -> [`SyncAccount`] |
//! | `POST /api/device/sync/logout`                | 204; vaults stay linked       |
//! | `POST /api/device/sync/vaults/{vault}/link`   | link and run the first round -> [`SyncReport`] |
//! | `POST /api/device/sync/vaults/{vault}/unlink` | 204; files stay on both sides |
//! | `POST /api/device/sync/vaults/{vault}/now`    | a round now -> [`SyncReport`] |
//!
//! The status asks the server for its vault list with a short timeout; when it
//! does not answer, `remote` of the vaults is `null` and `server_error` says
//! why. The background workers (`DeviceSync`, started by [`crate::serve`])
//! sync linked vaults by themselves: on a local change, on a server change, and
//! with a retry after an error; `state` is the worker's.
//!
//! Errors are `{ error }` with a code that is never 401 (the client takes that
//! for "sign in to this app"): 409 not signed in, session ended, a sync that is
//! already running or a vault that is not linked; 422 a wrong login or
//! password; 502 the storage server is unreachable or refuses; 404 an unknown
//! vault; 404 for all of this when the server was started without sync.
//!
//! Renaming or deleting a vault that is linked stops its worker and unlinks it
//! (the folder is no longer where the sync state says); the storage server
//! keeps its copy. Link the vault again, under its new name, to upload it.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use notes_device::{DeviceSync, Error as SyncError, ReportInfo, Round, Status, VaultStatus, WorkState};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::{ApiError, ApiResult};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/device/sync", get(status))
        .route("/api/device/sync/login", post(login))
        .route("/api/device/sync/logout", post(logout))
        .route("/api/device/sync/vaults/{vault}/link", post(link))
        .route("/api/device/sync/vaults/{vault}/unlink", post(unlink))
        .route("/api/device/sync/vaults/{vault}/now", post(now))
}

/// `GET /api/device/sync`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncStatus {
    /// The storage server of the saved account.
    pub server: Option<String>,
    pub login: Option<String>,
    pub signed_in: bool,
    /// Why the server's vault list is unknown (unreachable, session ended).
    pub server_error: Option<String>,
    /// The server refuses the saved session: sign in again.
    pub session_ended: bool,
    /// Every vault of this device, of the account on the server and every
    /// linked one, in alphabetical order.
    pub vaults: Vec<SyncVault>,
}

/// A vault in [`SyncStatus`].
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncVault {
    pub name: String,
    /// The folder exists on this device.
    pub local: bool,
    /// The vault is on the server; `null`: not known (see `server_error`).
    pub remote: Option<bool>,
    pub linked: bool,
    pub state: SyncState,
    /// When the last round ended, Unix seconds.
    pub last_sync: Option<u64>,
    /// Why the last round failed, or what the worker is waiting for.
    pub error: Option<String>,
    /// What the last successful round did.
    pub report: Option<SyncReport>,
}

/// What a linked vault's worker is doing (`idle` for one that is not linked).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SyncState {
    Idle,
    Syncing,
    /// The last round failed.
    Error,
    /// The session ended: sign in again.
    SignIn,
    /// The server cannot be reached; retrying.
    Offline,
}

/// What a round did: the answer of `link` and `now`, and `report` of a vault.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncReport {
    pub uploaded: usize,
    pub downloaded: usize,
    pub removed_local: usize,
    pub removed_remote: usize,
    /// Files changed on both sides; settled by the setting `device.sync_prefer`.
    pub conflicts: Vec<String>,
    /// Files not synced in this round (too large, ...).
    pub skipped: Vec<String>,
}

/// `POST /api/device/sync/login`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncLogin {
    /// The address of the storage server; `https://` is assumed without a scheme.
    pub server: String,
    pub login: String,
    pub password: String,
}

impl std::fmt::Debug for SyncLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncLogin").field("server", &self.server).field("login", &self.login).finish_non_exhaustive()
    }
}

/// The answer of `POST /api/device/sync/login`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncAccount {
    /// The complete address that was saved.
    pub server: String,
    pub login: String,
    /// Plain `http://` to a host that is not this computer: the password and
    /// the notes travel unencrypted.
    pub insecure: bool,
}

impl From<ReportInfo> for SyncReport {
    fn from(r: ReportInfo) -> Self {
        Self {
            uploaded: r.uploaded,
            downloaded: r.downloaded,
            removed_local: r.removed_local,
            removed_remote: r.removed_remote,
            conflicts: r.conflicts,
            skipped: r.skipped,
        }
    }
}

impl From<WorkState> for SyncState {
    fn from(state: WorkState) -> Self {
        match state {
            WorkState::Idle => Self::Idle,
            WorkState::Syncing => Self::Syncing,
            WorkState::Error => Self::Error,
            WorkState::SignIn => Self::SignIn,
            WorkState::Offline => Self::Offline,
        }
    }
}

impl From<VaultStatus> for SyncVault {
    fn from(v: VaultStatus) -> Self {
        Self {
            name: v.name,
            local: v.local,
            remote: v.remote,
            linked: v.linked,
            state: v.state.into(),
            last_sync: v.last_sync,
            error: v.error,
            report: v.report.map(Into::into),
        }
    }
}

impl From<Status> for SyncStatus {
    fn from(s: Status) -> Self {
        Self {
            server: s.server,
            login: s.login,
            signed_in: s.signed_in,
            server_error: s.server_error,
            session_ended: s.session_ended,
            vaults: s.vaults.into_iter().map(Into::into).collect(),
        }
    }
}

fn api_error(e: SyncError) -> ApiError {
    let e = match e {
        SyncError::Core(core) => return core.into(),
        other => other,
    };
    let code = match &e {
        SyncError::NotSignedIn
        | SyncError::SessionEnded { .. }
        | SyncError::Busy(_)
        | SyncError::NotLinked(_)
        | SyncError::RemoteMissing(_)
        | SyncError::FolderMissing(_) => StatusCode::CONFLICT,
        SyncError::WrongLogin => StatusCode::UNPROCESSABLE_ENTITY,
        SyncError::Unreachable { .. } | SyncError::Sync(_) => StatusCode::BAD_GATEWAY,
        SyncError::InvalidServer(..) => StatusCode::BAD_REQUEST,
        SyncError::NoSuchVault(_) => StatusCode::NOT_FOUND,
        SyncError::Core(_) | SyncError::Io { .. } | SyncError::Corrupt { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if code == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!("sync: {e}");
    }
    ApiError(code, e.to_string())
}

/// Runs blocking device sync work with the app's sync.
async fn run<T: Send + 'static>(
    s: &AppState,
    f: impl FnOnce(&DeviceSync) -> notes_device::Result<T> + Send + 'static,
) -> ApiResult<T> {
    let sync: Arc<DeviceSync> = s
        .device_sync
        .clone()
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "this server runs without vault sync".into()))?;
    tokio::task::spawn_blocking(move || f(&sync))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("task failed: {e}")))?
        .map_err(api_error)
}

async fn status(State(s): State<AppState>) -> ApiResult<Json<SyncStatus>> {
    Ok(Json(run(&s, |sync| sync.status(true)).await?.into()))
}

async fn login(State(s): State<AppState>, Json(req): Json<SyncLogin>) -> ApiResult<Json<SyncAccount>> {
    let account = run(&s, move |sync| {
        let server = notes_device::parse_server(&req.server)?;
        let account = sync.login(&req.server, &req.login, &req.password)?;
        Ok(SyncAccount { server: account.server, login: account.login, insecure: server.insecure })
    })
    .await?;
    Ok(Json(account))
}

async fn logout(State(s): State<AppState>) -> ApiResult<StatusCode> {
    run(&s, |sync| {
        if let Some(out) = sync.logout()?
            && let Some(why) = out.server_error
        {
            tracing::warn!("sync: the session on the server was not ended: {why}");
        }
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn report(round: Round) -> Json<SyncReport> {
    Json(round.report.into())
}

async fn link(State(s): State<AppState>, Path(vault): Path<String>) -> ApiResult<Json<SyncReport>> {
    Ok(report(run(&s, move |sync| sync.link(&vault)).await?))
}

async fn unlink(State(s): State<AppState>, Path(vault): Path<String>) -> ApiResult<StatusCode> {
    run(&s, move |sync| sync.unlink(&vault)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn now(State(s): State<AppState>, Path(vault): Path<String>) -> ApiResult<Json<SyncReport>> {
    Ok(report(run(&s, move |sync| sync.now(&vault)).await?))
}

/// Before a vault's folder is renamed or deleted: its worker stops. Returns
/// whether the vault is linked (pass it to [`settle`]).
pub(crate) async fn detach(s: &AppState, vault: &str) -> bool {
    let Some(sync) = s.device_sync.clone() else { return false };
    let vault = vault.to_owned();
    tokio::task::spawn_blocking(move || sync.detach(&vault)).await.unwrap_or(false)
}

/// After the rename or deletion (`moved`) or its failure: unlinks a moved
/// vault, otherwise starts the worker again.
pub(crate) async fn settle(s: &AppState, vault: &str, was_linked: bool, moved: bool) {
    let Some(sync) = s.device_sync.clone() else { return };
    let vault = vault.to_owned();
    let _ = tokio::task::spawn_blocking(move || sync.settle(&vault, was_linked, moved)).await;
}
