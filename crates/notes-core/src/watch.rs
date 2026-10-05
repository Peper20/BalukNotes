//! Vault changes: a file watcher instead of timed scans.
//!
//! [`Changes`] starts the vault watcher ([`Storage::watch`]) and hands the
//! changes to those who need them:
//!
//! - the **link index** ([`crate::graph::SourceIndex`]) does not scan the
//!   vault while [`Changes::seq`] stays the same (the counter grows at once
//!   on an event);
//! - **warming** ([`crate::warm`]) wakes up on a change, not once a minute;
//! - the **server** answers the client's waiting events request
//!   (`GET /api/vaults/{vault}/events`): the client checks the note version
//!   instead of polling every N seconds.
//!
//! Listeners get changes in batches, after the pause [`SETTLE`]: an editor
//! saves a file in several steps. Note versions are still the `stat` of
//! their files (a few calls per note, see [`crate::version`]).
//!
//! Besides the vault, another directory can be watched ([`Changes::also`]):
//! the styling library on disk (`/_baluk/` of a debug build); its paths come
//! with a prefix (`_baluk/theme.typ`).
//!
//! The watcher is a speedup, not the source of truth: when it breaks (queue
//! overflow, a network drive) [`Changes::watching`] turns false and
//! everything works as without it, by scanning.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::Mutex;

use crate::storage::{ChangeSink, Storage, WatchGuard};

/// How long to wait for quiet before handing out a batch of changes.
pub const SETTLE: Duration = Duration::from_millis(100);

/// A batch of changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// [`Changes::seq`] after this batch.
    pub seq: u64,
    /// Changed files (vault paths, no repeats). Empty: unknown what (the
    /// watcher broke), check everything.
    pub paths: Vec<String>,
}

type Listener = Box<dyn Fn(&Change) + Send + Sync>;

/// Vault changes: the counter and the listeners.
#[derive(Default)]
pub struct Changes {
    seq: AtomicU64,
    watching: AtomicBool,
    listeners: Mutex<Vec<Listener>>,
    guards: Mutex<Vec<WatchGuard>>,
    /// Where the watchers send events (after [`Changes::start`]).
    sink: Mutex<Option<ChangeSink>>,
}

impl std::fmt::Debug for Changes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Changes").field("seq", &self.seq()).field("watching", &self.watching()).finish_non_exhaustive()
    }
}

impl Changes {
    /// Starts the watcher. `false`: the storage cannot watch (or it failed,
    /// the reason is in the log); everything works by scanning.
    pub fn start(self: &Arc<Self>, storage: &dyn Storage) -> bool {
        let (tx, rx) = mpsc::channel::<Option<Vec<String>>>();
        let this = Arc::downgrade(self);
        let sink: ChangeSink = Arc::new(move |paths: Option<Vec<String>>| {
            if let Some(this) = this.upgrade() {
                // At once: the index must not return the old list after an event.
                this.seq.fetch_add(1, Ordering::SeqCst);
                if paths.is_none() {
                    this.watching.store(false, Ordering::SeqCst);
                }
            }
            let _ = tx.send(paths);
        });
        match storage.watch(sink.clone()) {
            Ok(Some(guard)) => {
                self.guards.lock().push(guard);
                *self.sink.lock() = Some(sink);
                self.watching.store(true, Ordering::SeqCst);
                let this = Arc::downgrade(self);
                let spawned = std::thread::Builder::new().name("notes-watch".into()).spawn(move || settle(&rx, &this));
                if let Err(e) = spawned {
                    tracing::warn!("vault watcher: {e}");
                    self.stop();
                    return false;
                }
                true
            }
            Ok(None) => false,
            Err(e) => {
                tracing::warn!("vault watcher did not start: {e}; scanning files from now on");
                false
            }
        }
    }

    /// Also watches `storage` (after [`Self::start`]): its paths come with
    /// the prefix `prefix` (`_baluk`). On failure only a warning: changes
    /// there are seen by the version check on the button and on returning to
    /// the window.
    pub fn also(&self, storage: &dyn Storage, prefix: &str) -> bool {
        let Some(inner) = self.sink.lock().clone() else { return false };
        let prefix = prefix.to_owned();
        let sink: ChangeSink = Arc::new(move |paths: Option<Vec<String>>| {
            inner(paths.map(|paths| paths.into_iter().map(|p| format!("{prefix}/{p}")).collect()));
        });
        match storage.watch(sink) {
            Ok(Some(guard)) => {
                self.guards.lock().push(guard);
                true
            }
            Ok(None) => false,
            Err(e) => {
                tracing::warn!("watcher of {}: {e}", storage.display("").display());
                false
            }
        }
    }

    /// A change the app made itself (deleting a note): hand it out as a
    /// watcher event without waiting for the OS, so the index does not
    /// return the old list. Without a watcher there is nothing to do: the
    /// index scans the vault anyway.
    pub fn local(&self, paths: Vec<String>) {
        let sink = self.sink.lock().clone();
        if let Some(sink) = sink {
            sink(Some(paths));
        }
    }

    /// Stops the watcher.
    pub fn stop(&self) {
        self.watching.store(false, Ordering::SeqCst);
        self.guards.lock().clear();
        self.sink.lock().take();
    }

    /// The watcher works: while [`Self::seq`] is the same, files did not change.
    pub fn watching(&self) -> bool {
        self.watching.load(Ordering::SeqCst)
    }

    /// Change counter: grows with every event.
    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    /// Calls `f` with every batch of changes (from the watcher thread).
    pub fn subscribe(&self, f: impl Fn(&Change) + Send + Sync + 'static) {
        self.listeners.lock().push(Box::new(f));
    }

    fn publish(&self, change: &Change) {
        for f in self.listeners.lock().iter() {
            f(change);
        }
    }
}

/// The watcher thread: collects events until the pause [`SETTLE`] and hands out the batch.
fn settle(rx: &mpsc::Receiver<Option<Vec<String>>>, changes: &Weak<Changes>) {
    while let Ok(first) = rx.recv() {
        let mut paths: Option<Vec<String>> = first;
        while let Ok(more) = rx.recv_timeout(SETTLE) {
            match (&mut paths, more) {
                (Some(all), Some(more)) => all.extend(more),
                (all, _) => *all = None,
            }
        }
        let Some(changes) = changes.upgrade() else { return };
        let mut paths = paths.unwrap_or_default();
        paths.sort();
        paths.dedup();
        tracing::debug!(files = paths.len(), "vault changes");
        changes.publish(&Change { seq: changes.seq(), paths });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemStorage;

    #[test]
    fn batches_changes_and_bumps_seq_at_once() {
        let storage = MemStorage::new();
        let changes = Arc::new(Changes::default());
        assert!(!changes.watching());
        assert!(changes.start(&storage));
        assert!(changes.watching());
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        changes.subscribe(move |c| tx.lock().send(c.clone()).unwrap());

        storage.write("b.typ", "1");
        storage.write("a.typ", "1");
        storage.write("b.typ", "2");
        assert_eq!(changes.seq(), 3, "the counter grows at once, before the batch");
        let change = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(change, Change { seq: 3, paths: vec!["a.typ".into(), "b.typ".into()] });

        // A second directory: paths with a prefix, in the same batch.
        let library = MemStorage::new();
        assert!(changes.also(&library, "_baluk"));
        library.write("theme.typ", "1");
        storage.write("c.typ", "1");
        let change = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(change, Change { seq: 5, paths: vec!["_baluk/theme.typ".into(), "c.typ".into()] });

        changes.stop();
        assert!(!changes.watching());
        assert!(!changes.also(&library, "_baluk"), "not after a stop");
    }
}
