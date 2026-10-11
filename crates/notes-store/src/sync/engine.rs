//! The sync round between a device [`Tree`] and a [`Remote`].
//!
//! # How a round works
//!
//! The device keeps a [`State`]: for every file the hash both sides had after
//! the last round (the base). A round compares three versions of every path:
//! the base, the local file and the server's file.
//!
//! 1. `remote.changes(state.remote_seq)` lists what the server changed; an
//!    entry counts only if its hash differs from the base hash.
//! 2. The tree is scanned. Only files whose size or modification time differ
//!    from the base are read and hashed; a file with the same hash is not a
//!    change. A file in the base that is gone from the tree is a local delete.
//! 3. For each path that changed on either side:
//!    - only the server changed: download it, or remove the local file (the
//!      server's tombstone), but only if the local file is still what the scan
//!      saw;
//!    - only the device changed: `put` / `delete` with the base as the
//!      expected version ([`Base::Hash`], or [`Base::Absent`] for a new file).
//!      A [`Conflict`] answer means the server changed meanwhile: it is
//!      resolved in the same round as a "both changed" case with the server's
//!      current version;
//!    - both changed to the same content (or both deleted): just recorded;
//!    - both changed differently: the [`Prefer`] rule. `Local` pushes the
//!      device's version with [`Base::Any`] (the server keeps the replaced
//!      one in its history). `Remote` takes the server's version; the local
//!      one goes to the tree's removed folder. Such a path is in
//!      [`Report::conflicts`].
//!
//!    Removals of local files go first, so a folder can become a file.
//! 4. The base is updated for each file as soon as it is done. At the end of
//!    the round, and when it fails midway, the state is saved. A crash or a
//!    network error therefore never makes the next round upload everything
//!    again or delete a file that was only not yet recorded.
//!
//! `state.remote_seq` moves forward only if every path was settled: a path
//! skipped because it changed locally during the round, or vanished on the
//! server, makes the next round look at the server's changes again.
//!
//! # The guard against mass deletion
//!
//! The plan is complete before anything is executed. If it deletes a large
//! part of the vault on the server (the files are gone from the device) or on
//! the device (they are gone from the server) - [`super::guard::is_mass`] -
//! the round returns [`Error::DeletionsHeld`] having changed nothing but the
//! state's file timestamps: no upload, download or deletion, `remote_seq` does
//! not move. [`Deletions::Confirmed`] with the fingerprint of that very set
//! lets the next round through; [`Deletions::Restore`] turns the device's
//! deletions into downloads, so the files come back from the server.
//!
//! # First sync and edge cases
//!
//! The first round of a device is the same algorithm with an empty base: a
//! file that is equal on both sides is recorded, a file on one side is copied
//! to the other, different content goes by `prefer`. So an empty local folder
//! linked to a filled server downloads everything, and a file deleted locally
//! BEFORE the first sync is downloaded again (without a base it is not a
//! deletion).
//!
//! Safety rules: a server tombstone removes a local file only if the local
//! file is unchanged against the base; a local deletion deletes on the
//! server only if the server still has the base version (or the device wins
//! by `Prefer::Local`). A local file is never overwritten or removed without
//! a fresh check that it is still what the scan saw. If the server's `seq`
//! went back (it was restored from a backup), the device compares with the
//! whole server listing; what the server no longer has is uploaded again.
//!
//! A file over [`MAX_FILE_SIZE`], and a server path that is not valid
//! ([`super::path`]), are listed in [`Report::skipped`] and not synced. A
//! failing tree operation (for example a folder and a file with the same name)
//! fails the round.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io;

use super::error::{Error, Result};
use super::guard::{self, Deletions, Side};
use super::hub::MAX_FILE_SIZE;
use super::path::is_synced;
use super::protocol::{Base, Changes, Conflict, Outcome, hash_hex};
use super::state::{State, Synced};
use super::tree::{FileInfo, Tree};

/// The server side of a sync, as the engine sees it. [`super::HubVault`]
/// implements it directly; an HTTP client implements it over the network
/// (errors: [`Error::Network`], [`Error::Unauthorized`]).
pub trait Remote {
    /// The files changed after `after` (see [`super::HubVault::changes`]).
    fn changes(&self, after: u64) -> Result<Changes>;
    /// The live file; [`Error::NotFound`] if it is gone.
    fn get(&self, path: &str) -> Result<Vec<u8>>;
    /// Writes if the server's version matches `base`.
    fn put(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome>;
    /// Deletes if the server's version matches `base`.
    fn delete(&self, path: &str, base: &Base) -> Result<Outcome>;
}

impl<T: Remote + ?Sized> Remote for &T {
    fn changes(&self, after: u64) -> Result<Changes> {
        (**self).changes(after)
    }
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        (**self).get(path)
    }
    fn put(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome> {
        (**self).put(path, base, data)
    }
    fn delete(&self, path: &str, base: &Base) -> Result<Outcome> {
        (**self).delete(path, base)
    }
}

/// Who wins when both sides changed a file differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefer {
    /// The device: the computer that writes first.
    Local,
    /// The server: the devices that receive.
    Remote,
}

/// What a round did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub uploaded: usize,
    pub downloaded: usize,
    pub removed_local: usize,
    pub removed_remote: usize,
    /// Paths changed on both sides differently; settled by [`Prefer`].
    pub conflicts: Vec<String>,
    /// Paths not synced in this round (see the module docs).
    pub skipped: Vec<String>,
}

impl Report {
    /// Did the round change anything.
    pub fn is_idle(&self) -> bool {
        self.uploaded + self.downloaded + self.removed_local + self.removed_remote == 0
    }
}

/// How a round decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub prefer: Prefer,
    pub deletions: Deletions,
}

impl Options {
    /// The usual round: `prefer` settles conflicts, mass deletion is held.
    #[must_use]
    pub fn new(prefer: Prefer) -> Self {
        Self { prefer, deletions: Deletions::Guard }
    }
}

/// One sync round with the guard on; see the module docs.
pub fn sync(tree: &dyn Tree, state: &mut State, remote: &dyn Remote, prefer: Prefer) -> Result<Report> {
    sync_with(tree, state, remote, &Options::new(prefer))
}

/// One sync round; see the module docs. The state is saved before returning,
/// also when the round fails.
pub fn sync_with(tree: &dyn Tree, state: &mut State, remote: &dyn Remote, options: &Options) -> Result<Report> {
    let mut report = Report::default();
    let result = Round {
        tree,
        remote,
        prefer: options.prefer,
        deletions: &options.deletions,
        state: &mut *state,
        report: &mut report,
        retry: false,
    }
    .run();
    let saved = state.save();
    result?;
    saved?;
    Ok(report)
}

enum Action {
    /// Both sides agree: record the local file, or forget the path.
    Settle,
    Download,
    RemoveLocal,
    Upload(Base),
    DeleteRemote(Base),
}

/// A local file seen by the scan.
struct Scanned {
    info: FileInfo,
    hash: String,
    /// Differs from the base.
    changed: bool,
}

#[derive(Default)]
struct Scan {
    files: BTreeMap<String, Scanned>,
    /// Local files not synced (too large).
    ignored: BTreeSet<String>,
}

/// The decision for one path. `local` is the local hash now, `base` the base
/// hash, `remote` is the server's hash (`None` - deleted) and `remote_changed`
/// tells whether it differs from the base. The flag in the result is "both
/// sides changed differently".
fn decide(
    local: Option<&str>,
    local_changed: bool,
    base: Option<&str>,
    (remote_changed, remote): (bool, Option<&str>),
    prefer: Prefer,
) -> (Action, bool) {
    let push = |base: Base| {
        if local.is_some() { Action::Upload(base) } else { Action::DeleteRemote(base) }
    };
    let take = |remote: Option<&str>| {
        if remote.is_some() { Action::Download } else { Action::RemoveLocal }
    };
    match (local_changed, remote_changed) {
        (false, false) => (Action::Settle, false),
        (true, false) => {
            let expected = base.map_or(Base::Absent, |hash| Base::Hash(hash.to_owned()));
            (push(expected), false)
        }
        (false, true) => (take(remote), false),
        (true, true) if local == remote => (Action::Settle, false),
        (true, true) => match prefer {
            Prefer::Local => (push(Base::Any), true),
            Prefer::Remote => (take(remote), true),
        },
    }
}

struct Round<'a> {
    tree: &'a dyn Tree,
    remote: &'a dyn Remote,
    prefer: Prefer,
    deletions: &'a Deletions,
    state: &'a mut State,
    report: &'a mut Report,
    /// Some path was left for the next round: `remote_seq` must not advance.
    retry: bool,
}

impl Round<'_> {
    fn run(&mut self) -> Result<()> {
        let changes = self.fetch_changes()?;
        let scan = self.scan()?;
        let remote_changed = self.remote_changes(&changes, &scan);

        let mut candidates: BTreeSet<&str> = remote_changed.keys().map(String::as_str).collect();
        candidates.extend(scan.files.iter().filter(|(_, s)| s.changed).map(|(p, _)| p.as_str()));
        candidates.extend(
            self.state
                .files
                .keys()
                .filter(|p| !scan.files.contains_key(*p) && !scan.ignored.contains(*p))
                .map(String::as_str),
        );

        let mut plan = Vec::new();
        for path in candidates {
            let local = scan.files.get(path);
            let base = self.state.files.get(path).map(|s| s.hash.as_str());
            let local_changed = local.map_or(base.is_some(), |s| s.changed);
            let remote = remote_changed.get(path);
            let remote = (remote.is_some(), remote.and_then(|hash| hash.as_deref()));
            let (action, conflict) = decide(local.map(|s| s.hash.as_str()), local_changed, base, remote, self.prefer);
            if conflict {
                self.report.conflicts.push(path.to_owned());
            }
            plan.push((path.to_owned(), action));
        }
        if matches!(self.deletions, Deletions::Restore) {
            // The files that are missing here come back from the server.
            for (_, action) in &mut plan {
                if matches!(action, Action::DeleteRemote(_)) {
                    *action = Action::Download;
                }
            }
        }
        self.guard(&plan)?;
        // Removals first: a folder can turn into a file.
        plan.sort_by_key(|(path, action)| (!matches!(action, Action::RemoveLocal), path.clone()));
        for (path, action) in plan {
            self.execute(&path, action, &scan)?;
        }
        if !self.retry {
            self.state.remote_seq = changes.seq;
        }
        Ok(())
    }

    /// Stops the round if the plan deletes a large part of the vault.
    fn guard(&self, plan: &[(String, Action)]) -> Result<()> {
        let total = self.state.files.len();
        let paths = |wanted: fn(&Action) -> bool| -> Vec<&str> {
            plan.iter().filter(|(_, action)| wanted(action)).map(|(path, _)| path.as_str()).collect()
        };
        let on_server = paths(|a| matches!(a, Action::DeleteRemote(_)));
        let here = paths(|a| matches!(a, Action::RemoveLocal));
        let held = guard::check(self.deletions, Side::Server, &on_server, total)
            .or_else(|| guard::check(self.deletions, Side::Device, &here, total));
        held.map_or(Ok(()), |held| Err(Error::DeletionsHeld(held)))
    }

    fn fetch_changes(&mut self) -> Result<Changes> {
        let mut changes = self.remote.changes(self.state.remote_seq)?;
        if changes.seq < self.state.remote_seq {
            // The server went back in time: compare with everything it has.
            // What it no longer knows is not a deletion, upload it again.
            changes = self.remote.changes(0)?;
            self.state.remote_seq = 0;
            let known: HashSet<&str> = changes.entries.iter().map(|e| e.path.as_str()).collect();
            self.state.files.retain(|path, _| known.contains(path.as_str()));
        }
        Ok(changes)
    }

    /// The server's entries that differ from the base: path -> hash (`None` -
    /// deleted).
    fn remote_changes(&mut self, changes: &Changes, scan: &Scan) -> BTreeMap<String, Option<String>> {
        let mut out = BTreeMap::new();
        for entry in &changes.entries {
            if !is_synced(&entry.path) || (entry.hash.is_some() && entry.size > MAX_FILE_SIZE) {
                self.report.skipped.push(entry.path.clone());
                continue;
            }
            if scan.ignored.contains(&entry.path) {
                continue;
            }
            let base = self.state.files.get(&entry.path).map(|s| s.hash.as_str());
            if entry.hash.as_deref() != base {
                out.insert(entry.path.clone(), entry.hash.clone());
            }
        }
        out
    }

    /// Lists the tree and reads only the files that differ from the base.
    fn scan(&mut self) -> Result<Scan> {
        let mut scan = Scan::default();
        for info in self.tree.list()? {
            if !is_synced(&info.path) {
                continue;
            }
            if info.size > MAX_FILE_SIZE {
                self.report.skipped.push(info.path.clone());
                scan.ignored.insert(info.path);
                continue;
            }
            let base = self.state.files.get_mut(&info.path);
            if let Some(base) = &base
                && base.size == info.size
                && base.mtime == info.mtime
            {
                let hash = base.hash.clone();
                scan.files.insert(info.path.clone(), Scanned { info, hash, changed: false });
                continue;
            }
            let data = match self.tree.read(&info.path) {
                Ok(data) => data,
                // Gone since the listing: the same as not listed.
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let hash = hash_hex(&data);
            let mut changed = true;
            if let Some(base) = base
                && base.hash == hash
            {
                // Touched, not changed.
                base.size = info.size;
                base.mtime = info.mtime;
                changed = false;
            }
            scan.files.insert(info.path.clone(), Scanned { info, hash, changed });
        }
        Ok(scan)
    }

    /// Is the local file still what the scan saw.
    fn local_unchanged(&self, path: &str, scan: &Scan) -> Result<bool> {
        let now = self.tree.stat(path)?;
        Ok(match (now, scan.files.get(path)) {
            (None, None) => true,
            (Some(now), Some(seen)) => now.size == seen.info.size && now.mtime == seen.info.mtime,
            _ => false,
        })
    }

    fn skip(&mut self, path: &str, retry: bool) {
        self.report.skipped.push(path.to_owned());
        self.retry |= retry;
    }

    fn execute(&mut self, path: &str, action: Action, scan: &Scan) -> Result<()> {
        match action {
            Action::Settle => {
                match scan.files.get(path) {
                    Some(seen) => self.record(path, &seen.hash, &seen.info),
                    None => {
                        self.state.files.remove(path);
                    }
                }
                Ok(())
            }
            Action::Download => self.download(path, scan),
            Action::RemoveLocal => {
                if !self.local_unchanged(path, scan)? {
                    self.skip(path, true);
                    return Ok(());
                }
                self.tree.remove(path)?;
                self.state.files.remove(path);
                self.report.removed_local += 1;
                Ok(())
            }
            Action::Upload(base) => self.upload(path, &base, scan),
            Action::DeleteRemote(base) => match self.remote.delete(path, &base)? {
                Ok(_) => {
                    self.state.files.remove(path);
                    self.report.removed_remote += 1;
                    Ok(())
                }
                Err(conflict) => self.resolve(path, &base, conflict, scan),
            },
        }
    }

    fn record(&mut self, path: &str, hash: &str, info: &FileInfo) {
        self.state.files.insert(path.to_owned(), Synced { hash: hash.to_owned(), size: info.size, mtime: info.mtime });
    }

    fn download(&mut self, path: &str, scan: &Scan) -> Result<()> {
        if !self.local_unchanged(path, scan)? {
            self.skip(path, true);
            return Ok(());
        }
        let data = match self.remote.get(path) {
            Ok(data) => data,
            // Deleted since the listing: the next round sees the tombstone.
            Err(Error::NotFound(_)) => {
                self.skip(path, true);
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        // The hash of what we got, not of what the listing promised: the file
        // may have changed on the server since.
        let hash = hash_hex(&data);
        let info = self.tree.write(path, &data)?;
        self.record(path, &hash, &info);
        self.report.downloaded += 1;
        Ok(())
    }

    fn upload(&mut self, path: &str, base: &Base, scan: &Scan) -> Result<()> {
        let Some(seen) = scan.files.get(path) else {
            return Ok(());
        };
        let data = match self.tree.read(path) {
            Ok(data) => data,
            // Deleted since the scan: the next round sees it.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.skip(path, false);
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        if data.len() as u64 > MAX_FILE_SIZE {
            self.skip(path, false);
            return Ok(());
        }
        let hash = hash_hex(&data);
        match self.remote.put(path, base, &data)? {
            Ok(_) => {
                self.record(path, &hash, &seen.info);
                self.report.uploaded += 1;
                Ok(())
            }
            Err(conflict) => self.resolve(path, base, conflict, scan),
        }
    }

    /// The server refused a write: it has another version. Decides again,
    /// as for a path both sides changed, with the server's current version.
    fn resolve(&mut self, path: &str, base: &Base, conflict: Conflict, scan: &Scan) -> Result<()> {
        if matches!(base, Base::Any) {
            return Err(Error::Other(format!("{path}: the server refused a forced write")));
        }
        let remote_hash = conflict.current.and_then(|entry| entry.hash);
        let local = scan.files.get(path).map(|s| s.hash.as_str());
        let (action, is_conflict) = decide(local, true, None, (true, remote_hash.as_deref()), self.prefer);
        if is_conflict {
            self.report.conflicts.push(path.to_owned());
        }
        self.execute(path, action, scan)
    }
}
