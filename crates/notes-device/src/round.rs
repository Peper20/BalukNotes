//! One sync round for a vault, linking and unlinking.
//!
//! A round loads the account and the vault's state, runs the engine
//! (`notes_store::sync::sync`) over the folder `<data>/vaults/<vault>` and the
//! hub, and records the result in `status/<vault>.json`. It holds the vault's
//! lock for its whole run.

use std::fmt;
use std::fs;
use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use notes_core::VaultName;
use notes_hub::api::VaultInfo;
use notes_hub::client::{self, HttpRemote};
use notes_store::fsutil::write_atomic;
use notes_store::sync::{DirTree, Prefer, Report, State, sync};
use serde::{Deserialize, Serialize};

use crate::account::Account;
use crate::{Error, Paths, Result, now_secs};

/// How long a second round waits for the one that runs.
pub const LOCK_WAIT: Duration = Duration::from_secs(60);

/// How long the files that sync replaced or removed are kept.
pub const REMOVED_KEEP: Duration = Duration::from_hours(24 * 30);

/// What a round did, as stored and shown.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportInfo {
    pub uploaded: usize,
    pub downloaded: usize,
    pub removed_local: usize,
    pub removed_remote: usize,
    pub conflicts: Vec<String>,
    pub skipped: Vec<String>,
}

impl From<&Report> for ReportInfo {
    fn from(r: &Report) -> Self {
        Self {
            uploaded: r.uploaded,
            downloaded: r.downloaded,
            removed_local: r.removed_local,
            removed_remote: r.removed_remote,
            conflicts: r.conflicts.clone(),
            skipped: r.skipped.clone(),
        }
    }
}

impl fmt::Display for ReportInfo {
    /// `uploaded 1, downloaded 0, removed 0 local / 0 on the server[,
    /// conflicts: a, b][, skipped: c]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "uploaded {}, downloaded {}, removed {} local / {} on the server",
            self.uploaded, self.downloaded, self.removed_local, self.removed_remote
        )?;
        if !self.conflicts.is_empty() {
            write!(f, ", conflicts: {}", self.conflicts.join(", "))?;
        }
        if !self.skipped.is_empty() {
            write!(f, ", skipped: {}", self.skipped.join(", "))?;
        }
        Ok(())
    }
}

/// The last round of a vault (`status/<vault>.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastRound {
    /// When it ended, Unix seconds.
    pub at: u64,
    /// Why it failed; `None`: it succeeded.
    pub error: Option<String>,
    pub report: Option<ReportInfo>,
}

impl LastRound {
    /// The last round of the vault; `None` if none ran yet or the file is
    /// unreadable (it is only a note for the user).
    #[must_use]
    pub fn read(paths: &Paths, vault: &VaultName) -> Option<Self> {
        serde_json::from_slice(&fs::read(paths.status_file(vault)).ok()?).ok()
    }

    fn write(&self, paths: &Paths, vault: &VaultName) {
        let written = serde_json::to_vec(self)
            .map_err(io::Error::other)
            .and_then(|json| write_atomic(&paths.status_file(vault), &json));
        if let Err(e) = written {
            tracing::warn!("sync status of \"{vault}\": {e}");
        }
    }
}

/// The result of a successful round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    pub report: ReportInfo,
    /// The server's change number the device is up to date with.
    pub remote_seq: u64,
}

/// The lock of a vault's round: an OS lock on a file, released when this is
/// dropped or the process dies.
#[derive(Debug)]
pub struct Lock {
    _file: fs::File,
}

impl Lock {
    /// Waits up to `wait` for the lock; then [`Error::Busy`].
    pub fn acquire(paths: &Paths, vault: &VaultName, wait: Duration) -> Result<Self> {
        let path = paths.lock_file(vault);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| Error::io(&path, e))?;
        let deadline = Instant::now() + wait;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(fs::TryLockError::WouldBlock) => return Err(Error::Busy(vault.to_string())),
                Err(fs::TryLockError::Error(e)) => return Err(Error::io(&path, e)),
            }
        }
    }
}

/// One round for a vault that is linked. The vault folder must exist.
///
/// # Errors
/// [`Error::NotSignedIn`], [`Error::SessionEnded`], [`Error::Unreachable`],
/// [`Error::NotLinked`], [`Error::Busy`], ... (see [`Error`]).
pub fn sync_linked(paths: &Paths, vault: &VaultName, prefer: Prefer) -> Result<Round> {
    let account = Account::require(paths)?;
    run(paths, &account, vault, prefer, true)
}

fn run(paths: &Paths, account: &Account, vault: &VaultName, prefer: Prefer, linked: bool) -> Result<Round> {
    let dir = paths.vault_dir(vault);
    if !dir.is_dir() {
        return Err(Error::FolderMissing(vault.to_string()));
    }
    let _lock = Lock::acquire(paths, vault, LOCK_WAIT)?;
    // Under the lock: an unlink that ran meanwhile must not be undone by the
    // state this round saves.
    if linked && !paths.is_linked(vault) {
        return Err(Error::NotLinked(vault.to_string()));
    }
    let mut state = State::open(paths.state_file(vault))?;
    let tree = DirTree::new(&dir, paths.removed_dir(vault));
    let remote = HttpRemote::new(&account.server, account.token(), vault.as_str());
    let result = sync(&tree, &mut state, &remote, prefer);
    // The user-facing words, not the engine's ("unauthorized").
    let result = result.map_err(|e| Error::from_sync(account, e, Some(vault.as_str())));
    let last = match &result {
        Ok(report) => LastRound { at: now_secs(), error: None, report: Some(report.into()) },
        Err(e) => LastRound { at: now_secs(), error: Some(e.to_string()), report: None },
    };
    last.write(paths, vault);
    result.map(|report| Round { report: (&report).into(), remote_seq: state.remote_seq })
}

/// Links a vault and runs its first round.
///
/// - only on this device: it is created on the server and uploaded;
/// - on both: merged by the engine's rules (`prefer` for a file that differs);
/// - only on the server: the folder `<data>/vaults/<vault>` is created and
///   everything is downloaded.
///
/// A vault that is already linked just runs a round. If the first round fails
/// the vault stays unlinked.
pub fn link(paths: &Paths, vault: &VaultName, prefer: Prefer) -> Result<Round> {
    let account = Account::require(paths)?;
    let local = paths.vault_dir(vault).is_dir();
    let remote = remote_vaults(&account, LINK_LIST_WAIT)?.iter().any(|v| v.name == vault.as_str());
    match (local, remote) {
        (false, false) => return Err(Error::NoSuchVault(vault.to_string())),
        (false, true) => {
            notes_core::Vaults::new(paths.data()).create(vault)?;
        }
        (true, false) => {
            client::create_vault(&account.server, account.token(), vault.as_str())
                .map_err(|e| Error::from_sync(&account, e, None))?;
        }
        (true, true) => {}
    }
    let was_linked = paths.is_linked(vault);
    let result = run(paths, &account, vault, prefer, false);
    if result.is_err() && !was_linked {
        forget(paths, vault);
    }
    result
}

/// The wait for the list of server vaults when linking.
const LINK_LIST_WAIT: Duration = Duration::from_secs(30);

/// Forgets the link: the state and the status. Files stay on both sides.
/// `false`: it was not linked.
pub fn unlink(paths: &Paths, vault: &VaultName) -> Result<bool> {
    if !paths.is_linked(vault) {
        return Ok(false);
    }
    // After a running round, not under it.
    let _lock = Lock::acquire(paths, vault, LOCK_WAIT)?;
    forget(paths, vault);
    Ok(true)
}

fn forget(paths: &Paths, vault: &VaultName) {
    for file in [paths.state_file(vault), paths.status_file(vault)] {
        match fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!("{}: {e}", file.display()),
        }
    }
}

/// Deletes what sync replaced or removed more than [`REMOVED_KEEP`] ago, in
/// every vault. Returns how many entries went.
pub fn prune(paths: &Paths) -> usize {
    let root = paths.dir().join("removed");
    let Ok(items) = fs::read_dir(&root) else { return 0 };
    let mut pruned = 0;
    for item in items.flatten() {
        let tree = DirTree::new(item.path().join("none"), item.path());
        match tree.prune_removed(REMOVED_KEEP, SystemTime::now()) {
            Ok(n) => pruned += n,
            Err(e) => tracing::warn!("{}: {e}", item.path().display()),
        }
    }
    pruned
}

/// The vaults of the account on the server, with a limit on the wait: the
/// request runs on its own thread, which is left to finish if the limit
/// passes.
pub(crate) fn remote_vaults(account: &Account, wait: Duration) -> Result<Vec<VaultInfo>> {
    let (tx, rx) = mpsc::channel();
    let (server, token) = (account.server.clone(), account.token().to_owned());
    std::thread::Builder::new()
        .name("notes-sync-list".into())
        .spawn(move || {
            let _ = tx.send(client::vaults(&server, &token));
        })
        .map_err(|e| Error::Unreachable { server: account.server.clone(), detail: e.to_string() })?;
    match rx.recv_timeout(wait) {
        Ok(result) => result.map_err(|e| Error::from_sync(account, e, None)),
        Err(_) => Err(Error::Unreachable {
            server: account.server.clone(),
            detail: format!("no answer in {} s", wait.as_secs()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_text() {
        let mut report = ReportInfo { uploaded: 2, removed_remote: 1, ..ReportInfo::default() };
        assert_eq!(report.to_string(), "uploaded 2, downloaded 0, removed 0 local / 1 on the server");
        report.conflicts = vec!["a.typ".into(), "b.typ".into()];
        report.skipped = vec!["big.bin".into()];
        assert_eq!(
            report.to_string(),
            "uploaded 2, downloaded 0, removed 0 local / 1 on the server, conflicts: a.typ, b.typ, skipped: big.bin"
        );
    }

    #[test]
    fn lock_is_exclusive_and_released() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let vault = VaultName::new("v").unwrap();
        let first = Lock::acquire(&paths, &vault, Duration::ZERO).unwrap();
        let busy = Lock::acquire(&paths, &vault, Duration::from_millis(60)).unwrap_err();
        assert_eq!(busy.to_string(), "sync of \"v\" is already running");
        // Another vault is free.
        Lock::acquire(&paths, &VaultName::new("w").unwrap(), Duration::ZERO).unwrap();
        // A waiter gets it as soon as the holder lets go.
        let waiter = std::thread::scope(|scope| {
            let waiter = scope.spawn(|| Lock::acquire(&paths, &vault, Duration::from_secs(10)).map(|_| ()));
            std::thread::sleep(Duration::from_millis(100));
            drop(first);
            waiter.join().unwrap()
        });
        waiter.unwrap();
        Lock::acquire(&paths, &vault, Duration::ZERO).unwrap();
    }

    #[test]
    fn prunes_old_replaced_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let removed = paths.removed_dir(&VaultName::new("v").unwrap());
        let now = now_secs();
        let old = now - 31 * 24 * 3600;
        for stamp in [old, now] {
            fs::create_dir_all(removed.join(stamp.to_string())).unwrap();
            fs::write(removed.join(stamp.to_string()).join("a.typ"), "x").unwrap();
        }
        assert_eq!(prune(&paths), 1);
        assert!(!removed.join(old.to_string()).exists());
        assert!(removed.join(now.to_string()).join("a.typ").is_file());
        assert_eq!(prune(&Paths::new(dir.path().join("none"))), 0);
    }
}
