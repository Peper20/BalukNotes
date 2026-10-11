//! [`DeviceSync`]: the sync of a running app (`notes serve`). It keeps one
//! [`Worker`](crate::worker) per linked vault while the device is signed in,
//! and offers the operations of the client's API (sign in, link, unlink,
//! "sync now", status), so a link or an unlink also starts or stops the
//! worker.
//!
//! The set of workers follows the files: [`DeviceSync::reconcile`] (called at
//! start, by every operation and by [`DeviceSync::status`]) starts a worker for
//! a linked vault that has none, stops the worker of a vault that is no longer
//! linked (for instance after `notes sync unlink`), and restarts one that
//! ended after the session was refused when the device has signed in again.
//!
//! Renaming or deleting a linked vault: the app calls [`DeviceSync::detach`]
//! before and [`DeviceSync::settle`] after. The worker stops first (it must not
//! see the folder vanish as "everything deleted"); if the folder was moved the
//! vault is unlinked, as it is no longer where the state says. The copy on the
//! server stays untouched; link the vault again under its new name to upload
//! it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use notes_core::VaultName;
use parking_lot::Mutex;

use crate::account::{self, Account};
use crate::round::{self, Round};
use crate::status::{Status, status};
use crate::worker::{Context, Live, Timing, Worker};
use crate::{Error, Paths, Prefer, Result};

/// The sync of one app instance.
pub struct DeviceSync {
    ctx: Arc<Context>,
    workers: Mutex<BTreeMap<VaultName, Worker>>,
}

impl std::fmt::Debug for DeviceSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceSync").field("paths", &self.ctx.paths).field("workers", &self.workers).finish()
    }
}

fn name(vault: &str) -> Result<VaultName> {
    Ok(VaultName::new(vault).map_err(notes_core::Error::from)?)
}

impl DeviceSync {
    /// The sync of the data directory `data`. `prefer` is asked before every
    /// round (the device setting may change while the app runs).
    pub fn new(data: impl Into<PathBuf>, prefer: impl Fn() -> Prefer + Send + Sync + 'static) -> Arc<Self> {
        Self::with_timing(data, prefer, Timing::default())
    }

    pub fn with_timing(
        data: impl Into<PathBuf>,
        prefer: impl Fn() -> Prefer + Send + Sync + 'static,
        timing: Timing,
    ) -> Arc<Self> {
        let ctx = Context { paths: Paths::new(data), prefer: Box::new(prefer), timing };
        Arc::new(Self { ctx: Arc::new(ctx), workers: Mutex::default() })
    }

    pub fn paths(&self) -> &Paths {
        &self.ctx.paths
    }

    /// Clears old replaced files and starts the workers of the linked vaults.
    pub fn start(&self) {
        let pruned = round::prune(&self.ctx.paths);
        if pruned > 0 {
            tracing::info!("sync: removed {pruned} old copies of replaced files");
        }
        self.reconcile();
    }

    /// Stops all workers; waits for the rounds that are running.
    pub fn stop(&self) {
        let workers = std::mem::take(&mut *self.workers.lock());
        workers.into_values().for_each(Worker::stop);
    }

    /// Makes the workers match the account and the linked vaults.
    pub fn reconcile(&self) {
        let account = match Account::load(&self.ctx.paths) {
            Ok(account) => account,
            Err(e) => {
                tracing::warn!("sync: {e}");
                None
            }
        };
        let linked = match &account {
            Some(_) => self.ctx.paths.linked().unwrap_or_else(|e| {
                tracing::warn!("sync: {e}");
                Vec::new()
            }),
            None => Vec::new(),
        };
        let mut stopping = Vec::new();
        {
            let mut workers = self.workers.lock();
            let gone: Vec<VaultName> = workers.keys().filter(|v| !linked.contains(v)).cloned().collect();
            stopping.extend(gone.iter().filter_map(|v| workers.remove(v)));
            for vault in &linked {
                let Some(account) = &account else { continue };
                if let Some(worker) = workers.get(vault) {
                    // Ended on a refused session, and the device signed in again.
                    if !(worker.finished() && worker.token() != account.token()) {
                        continue;
                    }
                    stopping.extend(workers.remove(vault));
                }
                match Worker::start(self.ctx.clone(), vault.clone(), account.token().to_owned()) {
                    Ok(worker) => {
                        workers.insert(vault.clone(), worker);
                    }
                    Err(e) => tracing::error!("sync of \"{vault}\": cannot start: {e}"),
                }
            }
        }
        stopping.into_iter().for_each(Worker::stop);
    }

    /// The status; `remote` asks the server which vaults it has (a few
    /// seconds at most).
    pub fn status(&self, remote: bool) -> Result<Status> {
        self.reconcile();
        let live = |vault: &str| -> Option<Live> {
            let workers = self.workers.lock();
            workers.iter().find(|(v, _)| v.as_str() == vault).map(|(_, w)| w.live())
        };
        status(&self.ctx.paths, live, remote)
    }

    /// Signs in (replacing an account) and starts the workers.
    pub fn login(&self, server: &str, login: &str, password: &str) -> Result<Account> {
        let server = account::parse_server(server)?;
        self.stop();
        let result = account::login(&self.ctx.paths, &server, login, password);
        self.reconcile();
        result
    }

    /// Signs out; the vaults stay linked, their workers stop.
    pub fn logout(&self) -> Result<Option<account::LoggedOut>> {
        self.stop();
        account::logout(&self.ctx.paths)
    }

    /// Links a vault (see [`round::link`]) and starts its worker.
    pub fn link(&self, vault: &str) -> Result<Round> {
        let vault = name(vault)?;
        let round = round::link(&self.ctx.paths, &vault, (self.ctx.prefer)())?;
        self.reconcile();
        Ok(round)
    }

    /// Unlinks a vault and stops its worker; `false`: it was not linked.
    pub fn unlink(&self, vault: &str) -> Result<bool> {
        let vault = name(vault)?;
        self.stop_worker(&vault);
        round::unlink(&self.ctx.paths, &vault)
    }

    /// A round now, in the caller's thread. It waits for a round of the worker
    /// that is running.
    pub fn now(&self, vault: &str) -> Result<Round> {
        let vault = name(vault)?;
        if !self.ctx.paths.is_linked(&vault) {
            return Err(Error::NotLinked(vault.to_string()));
        }
        round::sync_linked(&self.ctx.paths, &vault, (self.ctx.prefer)())
    }

    /// Before the vault's folder is moved or deleted: stops its worker.
    /// Returns whether the vault is linked; pass it to [`Self::settle`].
    pub fn detach(&self, vault: &str) -> bool {
        let Ok(vault) = name(vault) else { return false };
        self.stop_worker(&vault);
        self.ctx.paths.is_linked(&vault)
    }

    /// After the folder was moved or deleted (`moved`) or the attempt failed:
    /// a moved vault is unlinked, otherwise the worker starts again.
    pub fn settle(&self, vault: &str, was_linked: bool, moved: bool) {
        if !was_linked {
            return;
        }
        if moved && let Ok(vault) = name(vault) {
            if let Err(e) = round::unlink(&self.ctx.paths, &vault) {
                tracing::warn!("sync: unlinking \"{vault}\": {e}");
            }
            tracing::info!(
                "sync: \"{vault}\" was moved or deleted, so it is no longer linked; the server keeps its copy"
            );
        }
        self.reconcile();
    }

    fn stop_worker(&self, vault: &VaultName) {
        let worker = self.workers.lock().remove(vault);
        worker.into_iter().for_each(Worker::stop);
    }
}

impl Drop for DeviceSync {
    fn drop(&mut self) {
        self.stop();
    }
}
