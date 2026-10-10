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
//!
//! The same files and rules as the sync in `notes serve` (`<data>/sync/`);
//! a round waits for a running round of the same vault. Who wins a file
//! changed on both sides is the setting `device.sync_prefer`.

use std::io::BufRead as _;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use notes_core::VaultName;
use notes_core::settings::{Platform, SettingsStore, stored_sync_prefer};
use notes_device::{Account, DeviceSync, Paths, Prefer, Status, VaultStatus, WorkState};

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
}

/// Runs a `notes sync` command in the data directory `data`.
pub fn run(data: &Path, vault: Option<&str>, action: SyncAction) -> Result<ExitCode> {
    let paths = Paths::new(data);
    let prefer = notes_device::prefer(stored_sync_prefer(&data.join("settings.json"), Platform::current()));
    match action {
        SyncAction::Login { server, login, password_stdin } => sign_in(&paths, &server, &login, password_stdin),
        SyncAction::Logout => logout(&paths),
        SyncAction::Status { json } => status(&paths, json),
        SyncAction::Link => link(&paths, &vault_name(vault)?, prefer),
        SyncAction::Unlink => unlink(&paths, &vault_name(vault)?),
        SyncAction::Now => now(&paths, vault, prefer),
    }
}

/// The sync of `notes serve`: its workers keep the linked vaults up to date
/// while the server runs. A conflict follows the device setting as it is at
/// the time of the round.
pub fn service(data: &Path, settings: Arc<SettingsStore>) -> Arc<DeviceSync> {
    DeviceSync::new(data, move || notes_device::prefer(settings.device().sync_prefer))
}

fn vault_name(vault: Option<&str>) -> Result<VaultName> {
    let Some(vault) = vault else { bail!("give the vault: --vault \"Name\" (the list: notes vaults)") };
    if vault.contains(['/', '\\']) {
        bail!("sync works on vaults of the data directory, by name: --vault \"Name\" (the list: notes vaults)");
    }
    Ok(VaultName::new(vault)?)
}

fn password(from_stdin: bool) -> Result<String> {
    let password = if from_stdin {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).context("read the password from stdin")?;
        line.trim_end_matches(['\r', '\n']).to_owned()
    } else {
        rpassword::prompt_password("Password: ")
            .context("cannot ask for the password here: pass it on stdin with --password-stdin")?
    };
    if password.is_empty() {
        bail!("the password is empty");
    }
    Ok(password)
}

fn sign_in(paths: &Paths, server: &str, login: &str, password_stdin: bool) -> Result<ExitCode> {
    let server = notes_device::parse_server(server)?;
    if server.insecure {
        eprintln!(
            "warning: {} is plain http to another computer: the password and your notes travel unencrypted",
            server.url
        );
    }
    let password = password(password_stdin)?;
    let account = notes_device::login(paths, &server, login, &password)?;
    out!("signed in to {} as {}", account.server, account.login);
    Ok(ExitCode::SUCCESS)
}

fn logout(paths: &Paths) -> Result<ExitCode> {
    match notes_device::logout(paths)? {
        None => eprintln!("not signed in"),
        Some(out) => {
            if let Some(why) = out.server_error {
                eprintln!("warning: the session on the server was not ended ({why}); it ends there by itself");
            }
            out!("signed out of {} ({})", out.account.server, out.account.login);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn status(paths: &Paths, json: bool) -> Result<ExitCode> {
    let status = notes_device::status(paths, |_| None, true)?;
    if json {
        out!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        print_status(&status);
    }
    Ok(ExitCode::SUCCESS)
}

fn print_status(status: &Status) {
    match (&status.server, &status.login) {
        (Some(server), Some(login)) => out!("server: {server} (signed in as {login})"),
        _ => out!("not signed in: notes sync login <server> --login <name>"),
    }
    if let Some(why) = &status.server_error {
        out!("server list unavailable: {why}");
    }
    for vault in &status.vaults {
        out!("{}", vault_line(vault));
    }
    if status.vaults.is_empty() {
        out!("no vaults");
    }
}

fn vault_line(v: &VaultStatus) -> String {
    let name = &v.name;
    if !v.linked {
        let place = match (v.local, v.remote) {
            (false, _) => "on the server only",
            (true, Some(true)) => "on both, not linked",
            (true, Some(false)) => "on this device only",
            (true, None) => "on this device",
        };
        return format!("{name}: {place} (notes sync link --vault \"{name}\")");
    }
    let when = v.last_sync.map(format_time);
    let mut line = match (&v.error, &when, &v.report) {
        (Some(error), Some(when), _) => format!("{name}: linked, the last sync failed ({when}): {error}"),
        (Some(error), None, _) => format!("{name}: linked, not synced yet: {error}"),
        (None, Some(when), Some(report)) => format!("{name}: linked, synced {when}: {report}"),
        (None, _, _) => format!("{name}: linked, not synced yet"),
    };
    if !v.local {
        line.push_str(" [the folder is missing]");
    }
    if v.remote == Some(false) {
        line.push_str(" [not on the server]");
    }
    if v.state == WorkState::SignIn {
        line.push_str(" [sign in again]");
    }
    line
}

/// `2026-10-10 12:00:01 UTC`.
fn format_time(unix: u64) -> String {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let secs = unix % 86_400;
    // Civil date from days since the epoch (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", secs / 3600, secs % 3600 / 60, secs % 60)
}

fn link(paths: &Paths, vault: &VaultName, prefer: Prefer) -> Result<ExitCode> {
    notes_device::prune(paths);
    let round = notes_device::link(paths, vault, prefer)?;
    out!("{vault}: linked; {}", round.report);
    Ok(ExitCode::SUCCESS)
}

fn unlink(paths: &Paths, vault: &VaultName) -> Result<ExitCode> {
    if notes_device::unlink(paths, vault)? {
        out!("{vault}: unlinked; the files stay here and on the server");
    } else {
        eprintln!("{vault}: was not linked");
    }
    Ok(ExitCode::SUCCESS)
}

fn now(paths: &Paths, vault: Option<&str>, prefer: Prefer) -> Result<ExitCode> {
    Account::require(paths)?;
    notes_device::prune(paths);
    let vaults = match vault {
        Some(_) => vec![vault_name(vault)?],
        None => paths.linked()?,
    };
    if vaults.is_empty() {
        eprintln!("no linked vaults: notes sync link --vault \"Name\"");
        return Ok(ExitCode::SUCCESS);
    }
    let mut failed = false;
    for vault in &vaults {
        match notes_device::sync_linked(paths, vault, prefer) {
            Ok(round) => out!("{vault}: {}", round.report),
            Err(e) if vaults.len() == 1 => return Err(e.into()),
            Err(e) => {
                failed = true;
                eprintln!("{vault}: failed: {e}");
            }
        }
    }
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(format_time(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_time(951_782_400 + 3661), "2000-02-29 01:01:01 UTC");
        assert_eq!(format_time(1_791_631_201), "2026-10-10 11:20:01 UTC");
    }

    #[test]
    fn vault_arguments() {
        assert!(vault_name(None).is_err());
        assert!(vault_name(Some("tests/vault")).is_err());
        assert_eq!(vault_name(Some("Учёба")).unwrap().as_str(), "Учёба");
    }
}
