//! `notes sync ...`: the vault sync of this device from the command line
//! (`notes_device`). No vault is opened and Typst does not run: it only
//! talks to the storage server and moves files.
//!
//!   notes sync login <server> --login <name> [--password-stdin]
//!   notes sync logout
//!   notes sync status [--json]
//!   notes sync link --vault <name>      first sync; the vault is created on the side that lacks it
//!   notes sync unlink --vault <name>    forget the link; files stay on both sides
//!   notes sync now [--vault <name>]     one round for one or all linked vaults
//!   notes sync confirm --vault <name>   go on with the deletions a round stopped at
//!   notes sync restore --vault <name>   get those files back from the server instead
//!
//! The same files and rules as the sync in `notes serve` (`<data>/sync/`);
//! a round waits for a running round of the same vault. Who wins a file
//! changed on both sides is the setting `device.sync_prefer`.
//!
//! A round that would delete a large part of a vault (on the server because
//! the files are gone from this device, or here because they are gone from
//! the server) stops before it deletes anything, `status` and `now` say so, and
//! it goes on only after `confirm`; `restore` brings the files back instead.

use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum SyncAction {
    /// Sign in to a storage server and save the session on this device (the
    /// password is not kept). `http://` is for a server on this computer.
    Login {
        /// Address: `notes.example` (https is assumed), `http://127.0.0.1:8422`.
        server: String,
        /// Account name on the server.
        #[arg(long)]
        login: String,
        /// Read the password from the first line of stdin instead of asking.
        #[arg(long)]
        password_stdin: bool,
    },
    /// End the session on the server (if it can be reached) and forget the
    /// account on this device. Linked vaults stay linked.
    Logout,
    /// The account, and every vault: linked or not, the last sync and its
    /// result, vaults that exist only on the server.
    Status {
        /// As JSON.
        #[arg(long)]
        json: bool,
    },
    /// Link `--vault <name>` and sync it for the first time. Only on this
    /// device: it is created on the server and uploaded. On both: merged (a
    /// file that differs follows `device.sync_prefer`). Only on the server:
    /// the folder is created here and everything is downloaded.
    Link,
    /// Forget the link of `--vault <name>`. Files stay on both sides.
    Unlink,
    /// One sync round for `--vault <name>`, or for every linked vault. Exit
    /// code 1 if a vault failed.
    Now,
    /// Delete the files that a round of `--vault <name>` stopped at (many
    /// files gone: on the server if they are gone from this device, here if
    /// they are gone from the server). Only that very set: if more files have
    /// gone since, the round stops again.
    Confirm,
    /// Get back from the server the files that are gone from this device and
    /// that a round of `--vault <name>` stopped at, instead of deleting them
    /// there.
    Restore,
}

/// Runs a `notes sync` command in the data directory `data`.
#[cfg(feature = "sync")]
pub fn run(data: &Path, vault: Option<&str>, action: &SyncAction) -> Result<ExitCode> {
    crate::device::run(data, vault, action)
}

/// A build without the device client (the storage server's VPS) has no sync commands.
#[cfg(not(feature = "sync"))]
pub fn run(_data: &Path, _vault: Option<&str>, _action: &SyncAction) -> Result<ExitCode> {
    anyhow::bail!("this notes was built without the sync client (feature `sync`)")
}
