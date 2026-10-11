//! What a device's sync looks like now: the account, every vault (on this
//! device, linked, on the server) and how its last round went.

use std::collections::BTreeSet;
use std::time::Duration;

use notes_core::VaultName;
use notes_store::sync::Held;
use serde::Serialize;

use crate::account::Account;
use crate::round::{LastRound, ReportInfo, remote_vaults};
use crate::worker::{Live, WorkState};
use crate::{Paths, Result};

/// How long the status waits for the server's list of vaults.
pub const REMOTE_WAIT: Duration = Duration::from_secs(4);

/// The status of the device's sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// The saved account, if the device is signed in.
    pub server: Option<String>,
    pub login: Option<String>,
    pub signed_in: bool,
    /// Why the server's vault list is unknown (unreachable, the session
    /// ended); then `remote` of the vaults is `None`.
    pub server_error: Option<String>,
    /// The server refuses the saved session: sign in again.
    pub session_ended: bool,
    pub vaults: Vec<VaultStatus>,
}

/// One vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VaultStatus {
    pub name: String,
    /// The folder exists on this device.
    pub local: bool,
    /// The vault exists on the server; `None`: not known.
    pub remote: Option<bool>,
    pub linked: bool,
    pub state: WorkState,
    /// When the last round ended, Unix seconds.
    pub last_sync: Option<u64>,
    /// Why the last round failed, or what the worker is waiting for.
    pub error: Option<String>,
    /// What the last successful round did.
    pub report: Option<ReportInfo>,
    /// The deletions the sync waits to be confirmed ([`WorkState::Held`]).
    pub held: Option<Held>,
}

/// Builds the status. `live` gives the state of a running worker for a vault
/// (`None`: no worker, as in the CLI); `remote` says whether to ask the server
/// for its vaults.
pub fn status(paths: &Paths, live: impl Fn(&str) -> Option<Live>, remote: bool) -> Result<Status> {
    let account = Account::load(paths)?;
    let (listed, server_error, session_ended) = match (&account, remote) {
        (Some(account), true) => match remote_vaults(account, REMOTE_WAIT) {
            Ok(list) => (Some(list.into_iter().map(|v| v.name).collect::<BTreeSet<_>>()), None, false),
            Err(e) => (None, Some(e.to_string()), matches!(e, crate::Error::SessionEnded { .. })),
        },
        _ => (None, None, false),
    };
    let local: BTreeSet<String> = paths.local_vaults()?.iter().map(ToString::to_string).collect();
    let linked: BTreeSet<String> = paths.linked()?.iter().map(ToString::to_string).collect();
    let names: BTreeSet<&String> = local.iter().chain(&linked).chain(listed.iter().flatten()).collect();

    let vaults = names
        .into_iter()
        .map(|name| {
            let last = VaultName::new(name).ok().and_then(|v| LastRound::read(paths, &v));
            let live = live(name);
            let failed = last.as_ref().is_some_and(|l| l.error.is_some());
            let held = last.as_ref().and_then(|l| l.held.clone());
            let state = match &live {
                // A round that was held, whoever ran it, has not been settled yet.
                Some(live) if held.is_some() && matches!(live.state, WorkState::Idle | WorkState::Held) => {
                    WorkState::Held
                }
                Some(live) => live.state,
                None if held.is_some() => WorkState::Held,
                None if failed => WorkState::Error,
                None => WorkState::Idle,
            };
            let live_error = live.and_then(|l| l.error);
            VaultStatus {
                name: name.clone(),
                local: local.contains(name),
                remote: listed.as_ref().map(|list| list.contains(name)),
                linked: linked.contains(name),
                state,
                last_sync: last.as_ref().map(|l| l.at),
                error: live_error.or_else(|| last.as_ref().and_then(|l| l.error.clone())),
                report: last.and_then(|l| l.report),
                held,
            }
        })
        .collect();

    Ok(Status {
        server: account.as_ref().map(|a| a.server.clone()),
        login: account.as_ref().map(|a| a.login.clone()),
        signed_in: account.is_some(),
        server_error,
        session_ended,
        vaults,
    })
}
