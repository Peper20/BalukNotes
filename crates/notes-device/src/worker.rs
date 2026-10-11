//! The background sync of one vault: a worker thread that keeps the device's
//! copy up to date without being asked.
//!
//! A round runs when the worker starts, and then when
//!
//! - **the vault folder changes** (the core's file watcher,
//!   `Storage::watch`), [`Timing::debounce`] after the last change: an editor
//!   saves in several steps, and a round also writes files that raise events,
//!   which end in one more round that finds nothing to do;
//! - **the server changes**: a helper thread asks the hub for changes after
//!   the worker's last known number and waits ([`Timing::poll_wait`], a long
//!   poll), so there is no polling by the clock. Two rounds follow each other
//!   no faster than the debounce;
//! - **a failed round is retried**: after [`Timing::backoff_min`], doubling up
//!   to [`Timing::backoff_max`]. A worker without a file watcher also runs a
//!   round every [`UNWATCHED_RESCAN`].
//!
//! A refused session ends the worker (state `sign-in`); [`crate::DeviceSync`]
//! starts it again after a new sign-in. A stop (`Worker::stop`) is prompt: it
//! waits for a round that is running, not for the long poll, which ends by
//! itself within [`Timing::poll_wait`] without touching anything.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notes_core::VaultName;
use notes_core::storage::{ChangeSink, DirStorage, Storage as _, WatchGuard};
use notes_hub::client::HttpRemote;
use notes_store::sync::Error as SyncError;
use notes_store::sync::path::is_synced;
use parking_lot::Mutex;
use serde::Serialize;

use crate::account::Account;
use crate::round::sync_linked;
use crate::{Error, Paths, Prefer, Result};

/// Without a file watcher, a round at least this often.
pub const UNWATCHED_RESCAN: Duration = Duration::from_secs(60);

/// The waits of a worker; the defaults are for real use, tests shorten them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Quiet time after a local change before a round.
    pub debounce: Duration,
    /// How long one request for server changes waits (the hub cuts it to 30 s).
    pub poll_wait: Duration,
    /// The first wait after a failed round or request; it doubles.
    pub backoff_min: Duration,
    pub backoff_max: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            debounce: Duration::from_secs(2),
            poll_wait: Duration::from_secs(20),
            backoff_min: Duration::from_secs(5),
            backoff_max: Duration::from_secs(300),
        }
    }
}

impl Timing {
    /// The wait after `failures` failures in a row (1 for the first).
    fn backoff(&self, failures: u32) -> Duration {
        let doublings = failures.saturating_sub(1).min(16);
        self.backoff_min.saturating_mul(1 << doublings).min(self.backoff_max)
    }
}

/// What a worker is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkState {
    Idle,
    Syncing,
    /// The last round failed for another reason than the network.
    Error,
    /// The session ended: the worker stopped, a new sign-in is needed.
    SignIn,
    /// The server cannot be reached; retrying.
    Offline,
}

/// What the worker tells about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live {
    pub state: WorkState,
    pub error: Option<String>,
}

/// What a worker needs from its owner.
pub(crate) struct Context {
    pub paths: Paths,
    pub prefer: Box<dyn Fn() -> Prefer + Send + Sync>,
    pub timing: Timing,
}

#[derive(Debug)]
enum Event {
    Stop,
    /// A file of the vault changed.
    Local,
    /// The server has changes the device does not know.
    Remote,
    /// The long poll failed (the first time of a streak).
    PollFailed(String),
    /// The session is gone.
    SignIn(String),
}

/// To the poller thread.
enum Ctl {
    /// A round ended; the device is up to date with this server number.
    Seq(u64),
    Stop,
}

/// A running worker.
pub(crate) struct Worker {
    tx: Sender<Event>,
    live: Arc<Mutex<Live>>,
    thread: Option<JoinHandle<()>>,
    /// The token of the account it started with: a different one is a new
    /// sign-in.
    token: String,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker").field("live", &self.live()).finish_non_exhaustive()
    }
}

impl Worker {
    pub fn start(ctx: Arc<Context>, vault: VaultName, token: String) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let live = Arc::new(Mutex::new(Live { state: WorkState::Syncing, error: None }));
        let thread = {
            let (tx, live) = (tx.clone(), live.clone());
            std::thread::Builder::new()
                .name(format!("notes-sync-{vault}"))
                .spawn(move || Runner { ctx, vault, tx, rx, live }.run())?
        };
        Ok(Self { tx, live, thread: Some(thread), token })
    }

    pub fn live(&self) -> Live {
        self.live.lock().clone()
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// The thread ended by itself (the session is gone).
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Stops the worker; waits for the round that is running.
    pub fn stop(mut self) {
        let _ = self.tx.send(Event::Stop);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("a sync worker panicked");
        }
    }
}

struct Runner {
    ctx: Arc<Context>,
    vault: VaultName,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    live: Arc<Mutex<Live>>,
}

/// What an event asks the loop to do.
enum Flow {
    Stop,
    Round,
    Local,
    Nothing,
}

impl Runner {
    fn set(&self, state: WorkState, error: Option<String>) {
        *self.live.lock() = Live { state, error };
    }

    fn run(self) {
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let poller = {
            let (ctx, vault, tx, stopped) = (self.ctx.clone(), self.vault.clone(), self.tx.clone(), stopped.clone());
            std::thread::Builder::new()
                .name(format!("notes-sync-poll-{}", self.vault))
                .spawn(move || poll(&ctx, &vault, &tx, &ctl_rx, &stopped))
        };
        if let Err(e) = &poller {
            tracing::warn!("sync of \"{}\": no thread for the server changes: {e}", self.vault);
        }
        // Kept while the loop runs.
        let watch = self.watch_vault();
        self.main_loop(watch.is_some(), &ctl_tx);
        stopped.store(true, Ordering::SeqCst);
        let _ = ctl_tx.send(Ctl::Stop);
        // The poller is not joined: it may sit in a long poll.
        drop(poller);
    }

    /// Reports changes of the vault's files to the loop.
    fn watch_vault(&self) -> Option<WatchGuard> {
        let storage = match DirStorage::open(self.ctx.paths.vault_dir(&self.vault)) {
            Ok(storage) => storage,
            Err(e) => {
                tracing::warn!("sync of \"{}\": the folder is not watched: {e}", self.vault);
                return None;
            }
        };
        let tx = self.tx.clone();
        let sink: ChangeSink = Arc::new(move |paths: Option<Vec<String>>| {
            // Lost events (`None`) may hide anything.
            if paths.is_none_or(|paths| paths.iter().any(|p| is_synced(p))) {
                let _ = tx.send(Event::Local);
            }
        });
        match storage.watch(sink) {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("sync of \"{}\": the folder is not watched: {e}", self.vault);
                None
            }
        }
    }

    fn main_loop(&self, watching: bool, ctl: &Sender<Ctl>) {
        let timing = self.ctx.timing;
        let mut round = true;
        let mut retry_in: Option<Duration> = None;
        let mut failures = 0u32;
        loop {
            if round {
                round = false;
                match self.round() {
                    Ok(seq) => {
                        failures = 0;
                        retry_in = None;
                        self.set(WorkState::Idle, None);
                        let _ = ctl.send(Ctl::Seq(seq));
                    }
                    Err(e) if e.needs_sign_in() => {
                        self.set(WorkState::SignIn, Some(e.to_string()));
                        return;
                    }
                    Err(e) => {
                        failures += 1;
                        retry_in = Some(timing.backoff(failures));
                        tracing::warn!("sync of \"{}\": {e}", self.vault);
                        let state = if e.is_offline() { WorkState::Offline } else { WorkState::Error };
                        self.set(state, Some(e.to_string()));
                    }
                }
            }
            let wait = retry_in.or((!watching).then_some(UNWATCHED_RESCAN));
            let event = match wait {
                Some(wait) => self.rx.recv_timeout(wait),
                None => self.rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match self.flow(event) {
                Flow::Stop => return,
                Flow::Round => round = true,
                Flow::Nothing => {}
                Flow::Local => match self.settle() {
                    Flow::Stop => return,
                    _ => round = true,
                },
            }
        }
    }

    /// Waits until the vault is quiet for the debounce time.
    fn settle(&self) -> Flow {
        loop {
            match self.rx.recv_timeout(self.ctx.timing.debounce) {
                Err(RecvTimeoutError::Timeout) => return Flow::Round,
                event => {
                    if matches!(self.flow(event), Flow::Stop) {
                        return Flow::Stop;
                    }
                }
            }
        }
    }

    fn flow(&self, event: std::result::Result<Event, RecvTimeoutError>) -> Flow {
        match event {
            Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => Flow::Stop,
            Ok(Event::Local) => Flow::Local,
            Ok(Event::Remote) | Err(RecvTimeoutError::Timeout) => Flow::Round,
            Ok(Event::PollFailed(detail)) => {
                if self.live.lock().state != WorkState::Syncing {
                    self.set(WorkState::Offline, Some(detail));
                }
                Flow::Nothing
            }
            Ok(Event::SignIn(detail)) => {
                self.set(WorkState::SignIn, Some(detail));
                Flow::Stop
            }
        }
    }

    /// One round; the server number the device is now up to date with.
    fn round(&self) -> Result<u64> {
        self.set(WorkState::Syncing, None);
        let prefer = (self.ctx.prefer)();
        sync_linked(&self.ctx.paths, &self.vault, prefer).map(|round| round.remote_seq)
    }
}

/// The poller thread: asks the hub for changes after the number the worker
/// has, and tells the worker when there are some.
fn poll(ctx: &Context, vault: &VaultName, tx: &Sender<Event>, ctl: &Receiver<Ctl>, stopped: &AtomicBool) {
    let timing = ctx.timing;
    // Nothing to compare with until the first round ends.
    let Ok(Ctl::Seq(mut after)) = ctl.recv() else { return };
    let mut failures = 0u32;
    let mut last_wake: Option<Instant> = None;
    loop {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        // Not faster than the debounce, whatever the server says.
        if let Some(wait) = last_wake.and_then(|at| timing.debounce.checked_sub(at.elapsed())) {
            match ctl.recv_timeout(wait) {
                Ok(Ctl::Seq(seq)) => after = seq,
                Ok(Ctl::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
        let account = match Account::load(&ctx.paths) {
            Ok(Some(account)) => account,
            Ok(None) => {
                let _ = tx.send(Event::SignIn(Error::NotSignedIn.to_string()));
                return;
            }
            Err(e) => {
                let _ = tx.send(Event::PollFailed(e.to_string()));
                return;
            }
        };
        let remote = HttpRemote::new(&account.server, account.token(), vault.as_str());
        match remote.wait_changes(after, timing.poll_wait) {
            Ok(changes) => {
                // Rounds that ended during the wait moved the number on.
                loop {
                    match ctl.try_recv() {
                        Ok(Ctl::Seq(seq)) => after = seq,
                        Ok(Ctl::Stop) | Err(mpsc::TryRecvError::Disconnected) => return,
                        Err(mpsc::TryRecvError::Empty) => break,
                    }
                }
                // Back online after a failure: a round clears the "offline" state.
                let recovered = std::mem::take(&mut failures) > 0;
                if recovered || changes.seq != after {
                    if tx.send(Event::Remote).is_err() {
                        return;
                    }
                    last_wake = Some(Instant::now());
                    // Until the round ends: its number is the next `after`.
                    match ctl.recv() {
                        Ok(Ctl::Seq(seq)) => after = seq,
                        _ => return,
                    }
                }
            }
            Err(SyncError::Unauthorized) => {
                let error = Error::from_sync(&account, SyncError::Unauthorized, None);
                let _ = tx.send(Event::SignIn(error.to_string()));
                return;
            }
            Err(e) => {
                failures += 1;
                if failures == 1 {
                    let error = Error::from_sync(&account, e, Some(vault.as_str()));
                    let _ = tx.send(Event::PollFailed(error.to_string()));
                }
                match ctl.recv_timeout(timing.backoff(failures)) {
                    Ok(Ctl::Seq(seq)) => after = seq,
                    Ok(Ctl::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_the_limit() {
        let timing = Timing::default();
        let secs = |n| timing.backoff(n).as_secs();
        assert_eq!([secs(1), secs(2), secs(3), secs(4)], [5, 10, 20, 40]);
        assert_eq!(secs(7), 300, "5 * 64 is over the limit");
        assert_eq!(secs(1000), 300, "no overflow");
    }
}
