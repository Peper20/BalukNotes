//! The device side of vault sync (architecture §9): what makes a device's
//! copy of a vault follow the copy on the storage server. The engine itself
//! is `notes_store::sync`, the network is `notes_hub::client`; this crate
//! keeps the device's data and decides when a round runs. One library for the
//! CLI (`notes sync ...`), the background worker of `notes serve` and the
//! HTTP API of the client, so all of them behave the same.
//!
//! # Where things live
//!
//! Everything is in `<data>/sync/` (the data directory is that of the app):
//!
//! | File                      | What                                             |
//! |---------------------------|--------------------------------------------------|
//! | `account.json`            | `{ server, login, token }`, mode `0600`          |
//! | `vaults/<vault>.json`     | the engine's `State`; its existence is "linked"  |
//! | `status/<vault>.json`     | the last round: time, error, report ([`LastRound`]) |
//! | `locks/<vault>.lock`      | held (`flock`) while a round runs                |
//! | `removed/<vault>/`        | local files that sync replaced or removed        |
//!
//! The vault itself is the ordinary folder `<data>/vaults/<vault>/`. Entries
//! of `removed/` older than [`REMOVED_KEEP`] are deleted when the app starts
//! ([`prune`]).
//!
//! # Modules
//!
//! - [`account`]: sign-in and sign-out, the account file, the server address;
//! - [`round`]: one round for a vault ([`sync_linked`]), [`link`], [`unlink`],
//!   the lock;
//! - [`status`]: what the account, the vaults and the server look like now;
//! - [`worker`] and [`service`]: the background sync of `notes serve`
//!   ([`DeviceSync`]);
//! - [`testing`] (feature `testing`): a hub in this process for tests.
//!
//! # Rules
//!
//! - Who wins a conflict is the device setting `device.sync_prefer`
//!   (`notes_core::settings::SyncPrefer`); the caller gives the [`Prefer`].
//! - Only one round runs for a vault at a time, across processes: the lock
//!   file is an OS lock, so a crash releases it. A second round waits for
//!   [`LOCK_WAIT`] and then says that the sync is already running.
//! - A round that would delete a large part of the vault stops before it
//!   deletes anything ([`Error::Held`], state [`WorkState::Held`]) and goes on
//!   only after [`confirm`] (delete) or [`restore`] (get the files back from
//!   the server). Architecture §9.
//! - Linking never deletes anything on either side. Unlinking forgets the
//!   state only; files stay on both sides.
//! - Errors are one line and say what to do ([`Error`]).
//!
//! # Known limits
//!
//! - The watcher does not report paths with a dot segment, so an edit of
//!   `.baluk/settings.json` is sent with the next round, not at once.
//! - Several processes of the app may run workers for one vault (a service
//!   and a second `notes serve`): the lock makes them take turns, but they all
//!   wake on every change.
//! - Unlinking by another process (the CLI) while `notes serve` runs: the
//!   server's worker stops when its next status or round notices.

pub mod account;
mod error;
mod paths;
pub mod round;
pub mod service;
pub mod status;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod worker;

pub use account::{Account, Server, login, logout, parse_server};
pub use error::{Error, Result};
pub use notes_core::settings::SyncPrefer;
pub use notes_store::sync::{Held, Prefer, Side};
pub use paths::Paths;
pub use round::{
    LOCK_WAIT, LastRound, REMOVED_KEEP, ReportInfo, Round, confirm, link, prune, restore, sync_linked, unlink,
};
pub use service::DeviceSync;
pub use status::{Status, VaultStatus, status};
pub use worker::{Timing, WorkState};

/// The engine's rule for a setting.
#[must_use]
pub fn prefer(setting: SyncPrefer) -> Prefer {
    match setting {
        SyncPrefer::Local => Prefer::Local,
        SyncPrefer::Remote => Prefer::Remote,
    }
}

/// Seconds since the epoch.
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
