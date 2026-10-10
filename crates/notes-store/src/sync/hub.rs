//! The server side of one vault, directory-backed.
//!
//! ```text
//! <dir>/files/               the vault tree, exactly as on a device (the truth)
//! <dir>/manifest.json        { seq, files: { path: { hash, size, mtime, seq } } }
//! <dir>/history/<seq>/<path> the content a write or a delete replaced
//! ```
//!
//! `seq` is the vault's change counter: every new version of a file (a write
//! with other content, a delete) gets the next number. `hash: null` in the
//! manifest is a tombstone, kept forever (a known limit: a vault with many
//! deletions grows its manifest).
//!
//! The manifest is a cache of `files/`: a file whose size or modification
//! time differs from the manifest (edited on the server by hand, restored from
//! a backup) is re-read and gets a new `seq`; a file that disappeared becomes
//! a tombstone. [`HubVault::open`] does it for the whole tree
//! ([`HubVault::rescan`]), and every operation does it for its own path first.
//! The previous content of a file changed by hand is not in `history`.
//!
//! Nothing is lost on the server: a write over a file and a delete first move
//! the old content to `history/<seq>/<path>` (there is no API to read it yet).
//!
//! One process owns a vault directory; the type is thread-safe (one lock per
//! vault, held for the whole operation).

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::engine::Remote;
use super::error::{Error, Result};
use super::path::check;
use super::protocol::{Base, Changes, Conflict, Entry, Outcome, hash_hex};
use super::tree::{FileInfo, move_file, remove_empty_parents, stat_file, walk};
use crate::fsutil::write_atomic;

/// The largest file in bytes. A bigger write is an error, bigger files in
/// `files/` are not synced.
pub const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    hash: Option<String>,
    size: u64,
    mtime: i64,
    seq: u64,
}

impl Record {
    fn entry(&self, path: &str) -> Entry {
        Entry { path: path.to_owned(), hash: self.hash.clone(), size: self.size, seq: self.seq }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    seq: u64,
    files: BTreeMap<String, Record>,
}

/// Does the writer's [`Base`] match the current hash (`None` - no live file).
fn base_matches(base: &Base, current: Option<&str>) -> bool {
    match base {
        Base::Any => true,
        Base::Absent => current.is_none(),
        Base::Hash(hash) => current == Some(hash.as_str()),
    }
}

/// One vault on the server.
#[derive(Debug)]
pub struct HubVault {
    dir: PathBuf,
    inner: Mutex<Manifest>,
}

impl HubVault {
    /// Opens (and creates) the vault in `dir`, then [`HubVault::rescan`]s.
    /// A broken `manifest.json` is an error and is never replaced: its numbers
    /// are what devices and `history/` refer to.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(dir.join("files"))?;
        let manifest_path = dir.join("manifest.json");
        let manifest = match fs::read(&manifest_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| Error::Corrupt { path: manifest_path.display().to_string(), reason: e.to_string() })?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Manifest::default(),
            Err(e) => return Err(e.into()),
        };
        let vault = Self { dir, inner: Mutex::new(manifest) };
        vault.rescan()?;
        Ok(vault)
    }

    fn files_dir(&self) -> PathBuf {
        self.dir.join("files")
    }

    /// The folder of the plain vault tree.
    pub fn files_path(&self) -> PathBuf {
        self.files_dir()
    }

    fn save(&self, manifest: &Manifest) -> Result<()> {
        let json = serde_json::to_vec(manifest).map_err(|e| Error::Other(e.to_string()))?;
        write_atomic(&self.dir.join("manifest.json"), &json)?;
        Ok(())
    }

    fn save_if(&self, manifest: &Manifest, changed: bool) -> Result<()> {
        if changed { self.save(manifest) } else { Ok(()) }
    }

    /// The vault's current change number (0 for a new vault).
    pub fn seq(&self) -> u64 {
        self.inner.lock().seq
    }

    /// Brings the manifest in line with `files/` (see the module docs).
    /// Call it when the files may have been edited by hand.
    pub fn rescan(&self) -> Result<()> {
        let found = walk(&self.files_dir(), None)?;
        let mut manifest = self.inner.lock();
        let mut changed = false;
        let mut seen = HashSet::new();
        for info in &found {
            changed |= self.reconcile(&mut manifest, &info.path, Some(info))?;
            seen.insert(info.path.as_str());
        }
        let gone: Vec<String> = manifest
            .files
            .iter()
            .filter(|(path, record)| record.hash.is_some() && !seen.contains(path.as_str()))
            .map(|(path, _)| path.clone())
            .collect();
        for path in gone {
            changed |= self.reconcile(&mut manifest, &path, None)?;
        }
        self.save_if(&manifest, changed)
    }

    /// Makes the record of `path` match the file seen on disk (`found`).
    /// Returns whether the manifest changed (the caller saves it).
    fn reconcile(&self, manifest: &mut Manifest, path: &str, found: Option<&FileInfo>) -> Result<bool> {
        let record = manifest.files.get(path);
        let live = record.is_some_and(|r| r.hash.is_some());
        let Some(info) = found else {
            if !live {
                return Ok(false);
            }
            manifest.seq += 1;
            let seq = manifest.seq;
            manifest.files.insert(path.to_owned(), Record { hash: None, size: 0, mtime: 0, seq });
            return Ok(true);
        };
        if live && record.is_some_and(|r| r.size == info.size && r.mtime == info.mtime) {
            return Ok(false);
        }
        if info.size > MAX_FILE_SIZE {
            tracing::warn!(path, size = info.size, "file over the sync limit, not synced");
            return Ok(false);
        }
        let data = match fs::read(self.files_dir().join(path)) {
            Ok(data) => data,
            // Gone while we looked: the next scan notices.
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let hash = hash_hex(&data);
        if let Some(record) = manifest.files.get_mut(path)
            && record.hash.as_deref() == Some(hash.as_str())
        {
            record.size = info.size;
            record.mtime = info.mtime;
            return Ok(true);
        }
        manifest.seq += 1;
        let seq = manifest.seq;
        manifest.files.insert(path.to_owned(), Record { hash: Some(hash), size: info.size, mtime: info.mtime, seq });
        Ok(true)
    }

    /// [`Self::reconcile`] against the disk, for one path.
    fn refresh(&self, manifest: &mut Manifest, path: &str) -> Result<bool> {
        let info = stat_file(&self.files_dir(), path)?;
        self.reconcile(manifest, path, info.as_ref())
    }

    /// The changes with `seq > after`, ordered by `seq`. Does not look at the
    /// disk: call [`HubVault::rescan`] first if files may have been edited by
    /// hand.
    pub fn changes(&self, after: u64) -> Changes {
        let manifest = self.inner.lock();
        let mut entries: Vec<Entry> = manifest
            .files
            .iter()
            .filter(|(_, record)| record.seq > after)
            .map(|(path, record)| record.entry(path))
            .collect();
        entries.sort_by_key(|entry| entry.seq);
        Changes { seq: manifest.seq, entries }
    }

    /// The live file; `NotFound` for a deleted or unknown one.
    pub fn read(&self, path: &str) -> Result<(Entry, Vec<u8>)> {
        check(path)?;
        let mut manifest = self.inner.lock();
        let changed = self.refresh(&mut manifest, path)?;
        self.save_if(&manifest, changed)?;
        let record = manifest
            .files
            .get(path)
            .filter(|record| record.hash.is_some())
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        let data = fs::read(self.files_dir().join(path)).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Error::NotFound(path.to_owned()),
            _ => e.into(),
        })?;
        Ok((record.entry(path), data))
    }

    /// Writes `data` if the server's version matches `base`. Identical
    /// content is not a new version: the current entry comes back whatever
    /// the base was. A path that would be a folder and a file at once is an
    /// error.
    pub fn write(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome> {
        check(path)?;
        let size = data.len() as u64;
        if size > MAX_FILE_SIZE {
            return Err(Error::TooLarge { path: path.to_owned(), size, limit: MAX_FILE_SIZE });
        }
        let mut manifest = self.inner.lock();
        let refreshed = self.refresh(&mut manifest, path)?;
        let current = manifest.files.get(path).cloned();
        let current_hash = current.as_ref().and_then(|r| r.hash.as_deref());
        let hash = hash_hex(data);
        if let Some(record) = &current
            && current_hash == Some(hash.as_str())
        {
            self.save_if(&manifest, refreshed)?;
            return Ok(Ok(record.entry(path)));
        }
        if !base_matches(base, current_hash) {
            self.save_if(&manifest, refreshed)?;
            return Ok(Err(Conflict { current: current.map(|record| record.entry(path)) }));
        }
        if let Some(reason) = collision(&manifest, path) {
            self.save_if(&manifest, refreshed)?;
            return Err(Error::InvalidPath { path: path.to_owned(), reason });
        }
        let seq = manifest.seq + 1;
        let abs = self.files_dir().join(path);
        if current_hash.is_some() {
            copy_to_history(&self.dir, seq, path, &abs)?;
        } else if abs.is_dir() {
            // An empty folder left by a hand edit.
            let _ = fs::remove_dir(&abs);
        }
        write_atomic(&abs, data)?;
        let info = stat_file(&self.files_dir(), path)?.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        let record = Record { hash: Some(hash), size, mtime: info.mtime, seq };
        let entry = record.entry(path);
        manifest.seq = seq;
        manifest.files.insert(path.to_owned(), record);
        self.save(&manifest)?;
        Ok(Ok(entry))
    }

    /// Deletes the file if the server's version matches `base`. Deleting a
    /// file that is already gone with `Absent` or `Any` is a success that
    /// changes nothing (the tombstone entry comes back).
    pub fn delete(&self, path: &str, base: &Base) -> Result<Outcome> {
        check(path)?;
        let mut manifest = self.inner.lock();
        let refreshed = self.refresh(&mut manifest, path)?;
        let current = manifest.files.get(path).cloned();
        let current_hash = current.as_ref().and_then(|r| r.hash.as_deref());
        if !base_matches(base, current_hash) {
            self.save_if(&manifest, refreshed)?;
            return Ok(Err(Conflict { current: current.map(|record| record.entry(path)) }));
        }
        if current_hash.is_none() {
            self.save_if(&manifest, refreshed)?;
            let seq = manifest.seq;
            return Ok(Ok(current.map_or_else(
                || Entry { path: path.to_owned(), hash: None, size: 0, seq },
                |record| record.entry(path),
            )));
        }
        let seq = manifest.seq + 1;
        let abs = self.files_dir().join(path);
        move_file(&abs, &self.dir.join("history").join(seq.to_string()).join(path))?;
        remove_empty_parents(&self.files_dir(), &abs);
        let record = Record { hash: None, size: 0, mtime: 0, seq };
        let entry = record.entry(path);
        manifest.seq = seq;
        manifest.files.insert(path.to_owned(), record);
        self.save(&manifest)?;
        Ok(Ok(entry))
    }
}

/// Copies the current content of `abs` to `history/<seq>/<path>`.
fn copy_to_history(dir: &Path, seq: u64, path: &str, abs: &Path) -> io::Result<()> {
    let dest = dir.join("history").join(seq.to_string()).join(path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(abs, dest).map(|_| ())
}

/// Why `path` cannot be a file: a live file is one of its parents, or lies
/// below it.
fn collision(manifest: &Manifest, path: &str) -> Option<&'static str> {
    let live = |p: &str| manifest.files.get(p).is_some_and(|r| r.hash.is_some());
    for (i, _) in path.match_indices('/') {
        if live(&path[..i]) {
            return Some("a file with the name of a parent folder exists");
        }
    }
    let prefix = format!("{path}/");
    let below = manifest
        .files
        .range(prefix.clone()..)
        .take_while(|(p, _)| p.starts_with(&prefix))
        .any(|(_, r)| r.hash.is_some());
    below.then_some("files exist below this name")
}

impl Remote for HubVault {
    fn changes(&self, after: u64) -> Result<Changes> {
        Ok(HubVault::changes(self, after))
    }

    fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.read(path).map(|(_, data)| data)
    }

    fn put(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome> {
        self.write(path, base, data)
    }

    fn delete(&self, path: &str, base: &Base) -> Result<Outcome> {
        HubVault::delete(self, path, base)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn open() -> (tempfile::TempDir, HubVault) {
        let dir = tempfile::tempdir().unwrap();
        let hub = HubVault::open(dir.path().join("v")).unwrap();
        (dir, hub)
    }

    fn put(hub: &HubVault, path: &str, base: &Base, data: &str) -> Outcome {
        hub.write(path, base, data.as_bytes()).unwrap()
    }

    /// A manual edit in `files/` with a modification time that surely differs.
    fn edit_by_hand(hub: &HubVault, path: &str, data: &str, secs: u64) {
        let abs = hub.files_path().join(path);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, data).unwrap();
        let file = File::options().write(true).open(abs).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
    }

    #[test]
    fn write_read_and_changes_in_order() {
        let (_dir, hub) = open();
        assert_eq!(hub.seq(), 0);
        let a = put(&hub, "a.typ", &Base::Absent, "A1").unwrap();
        let b = put(&hub, "sub/b.typ", &Base::Absent, "B1").unwrap();
        let a2 = put(&hub, "a.typ", &Base::Hash(a.hash.clone().unwrap()), "A2").unwrap();
        assert_eq!((a.seq, b.seq, a2.seq), (1, 2, 3));
        assert_eq!(hub.seq(), 3);
        let (entry, data) = hub.read("a.typ").unwrap();
        assert_eq!((entry, data), (a2.clone(), b"A2".to_vec()));
        let all = hub.changes(0);
        assert_eq!(all.seq, 3);
        let order: Vec<_> = all.entries.iter().map(|e| (e.path.as_str(), e.seq)).collect();
        assert_eq!(order, [("sub/b.typ", 2), ("a.typ", 3)]);
        let after = hub.changes(2);
        assert_eq!(after.entries, [a2]);
        assert!(hub.changes(3).entries.is_empty());
        assert!(matches!(hub.read("nope.typ"), Err(Error::NotFound(_))));
        assert_eq!(fs::read(hub.files_path().join("sub/b.typ")).unwrap(), b"B1");
    }

    #[test]
    fn bases_and_conflicts() {
        let (_dir, hub) = open();
        let first = put(&hub, "a.typ", &Base::Absent, "1").unwrap();
        // Absent on an existing file.
        let conflict = put(&hub, "a.typ", &Base::Absent, "2").unwrap_err();
        assert_eq!(conflict.current, Some(first.clone()));
        // A wrong hash.
        let wrong = Base::Hash(hash_hex(b"other"));
        assert_eq!(put(&hub, "a.typ", &wrong, "2").unwrap_err().current, Some(first.clone()));
        // Hash of the current version.
        let second = put(&hub, "a.typ", &Base::Hash(first.hash.clone().unwrap()), "2").unwrap();
        // Any wins over everything.
        let third = put(&hub, "a.typ", &Base::Any, "3").unwrap();
        assert_eq!((second.seq, third.seq), (2, 3));
        // A file the server never knew.
        assert_eq!(put(&hub, "new.typ", &wrong, "x").unwrap_err().current, None);
        assert!(put(&hub, "new.typ", &Base::Any, "x").is_ok());
        // Nothing changed by the refused writes.
        assert_eq!(hub.seq(), 4);
        assert_eq!(hub.read("a.typ").unwrap().1, b"3");
    }

    #[test]
    fn identical_rewrite_keeps_seq() {
        let (_dir, hub) = open();
        let first = put(&hub, "a.typ", &Base::Absent, "same").unwrap();
        let again = put(&hub, "a.typ", &Base::Any, "same").unwrap();
        assert_eq!(again, first);
        // Even with a base that does not match: the server already has it.
        let wrong = Base::Hash(hash_hex(b"x"));
        assert_eq!(put(&hub, "a.typ", &wrong, "same").unwrap(), first);
        assert_eq!(hub.seq(), 1);
        assert!(!hub.dir.join("history").exists());
    }

    #[test]
    fn delete_keeps_a_tombstone_and_history() {
        let (_dir, hub) = open();
        let v1 = put(&hub, "d/a.typ", &Base::Absent, "one").unwrap();
        // Wrong bases.
        let wrong = hub.delete("d/a.typ", &Base::Hash(hash_hex(b"x"))).unwrap();
        assert_eq!(wrong.unwrap_err().current, Some(v1.clone()));
        assert_eq!(hub.delete("d/a.typ", &Base::Absent).unwrap().unwrap_err().current, Some(v1.clone()));
        let gone = hub.delete("d/a.typ", &Base::Hash(v1.hash.clone().unwrap())).unwrap().unwrap();
        assert_eq!((gone.hash.clone(), gone.seq), (None, 2));
        assert!(!hub.files_path().join("d").exists());
        assert_eq!(fs::read(hub.dir.join("history/2/d/a.typ")).unwrap(), b"one");
        assert!(matches!(hub.read("d/a.typ"), Err(Error::NotFound(_))));
        // The tombstone is a change.
        assert_eq!(hub.changes(1).entries, std::slice::from_ref(&gone));
        // A repeated delete is a no-op; a hash base conflicts with the tombstone.
        assert_eq!(hub.delete("d/a.typ", &Base::Any).unwrap().unwrap(), gone);
        assert_eq!(hub.delete("d/a.typ", &Base::Absent).unwrap().unwrap(), gone);
        let conflict = hub.delete("d/a.typ", &Base::Hash(v1.hash.unwrap())).unwrap();
        assert_eq!(conflict.unwrap_err().current, Some(gone));
        // Never known: a no-op too.
        let unknown = hub.delete("x.typ", &Base::Any).unwrap().unwrap();
        assert_eq!((unknown.hash, hub.seq()), (None, 2));
        // Absent allows writing over the tombstone; the overwritten
        // content of a live file goes to history under the new seq.
        let v2 = put(&hub, "d/a.typ", &Base::Absent, "two").unwrap();
        assert_eq!(v2.seq, 3);
        put(&hub, "d/a.typ", &Base::Any, "three").unwrap();
        assert_eq!(fs::read(hub.dir.join("history/4/d/a.typ")).unwrap(), b"two");
    }

    #[test]
    fn reopens_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v");
        let (a, b) = {
            let hub = HubVault::open(&path).unwrap();
            let a = put(&hub, "a.typ", &Base::Absent, "A").unwrap();
            let b = put(&hub, "b.typ", &Base::Absent, "B").unwrap();
            hub.delete("b.typ", &Base::Any).unwrap().unwrap();
            (a, b)
        };
        let hub = HubVault::open(&path).unwrap();
        assert_eq!(hub.seq(), 3);
        assert_eq!(hub.read("a.typ").unwrap().0, a);
        let changes = hub.changes(0);
        assert_eq!(changes.entries.len(), 2);
        assert_eq!(changes.entries[1].hash, None);
        assert_ne!(changes.entries[1].seq, b.seq);
        // Nothing was re-numbered by the rescan on open.
        assert_eq!(HubVault::open(&path).unwrap().seq(), 3);
        // A broken manifest is not replaced.
        fs::write(path.join("manifest.json"), "{").unwrap();
        assert!(matches!(HubVault::open(&path), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn rescan_follows_manual_edits() {
        let (_dir, hub) = open();
        let a = put(&hub, "a.typ", &Base::Absent, "A").unwrap();
        put(&hub, "b.typ", &Base::Absent, "B").unwrap();
        put(&hub, "c.typ", &Base::Absent, "C").unwrap();
        edit_by_hand(&hub, "a.typ", "A edited", 1000);
        edit_by_hand(&hub, "new/n.typ", "N", 1000);
        fs::remove_file(hub.files_path().join("b.typ")).unwrap();
        // Touching without changing the content: no new seq.
        edit_by_hand(&hub, "c.typ", "C", 2000);
        // Not synced: ignored.
        edit_by_hand(&hub, ".git/x", "g", 1000);
        edit_by_hand(&hub, "z.typ.tmp", "t", 1000);
        assert_eq!(hub.seq(), 3);
        hub.rescan().unwrap();
        assert_eq!(hub.seq(), 6);
        let changes = hub.changes(3);
        let by_path: BTreeMap<_, _> = changes.entries.iter().map(|e| (e.path.as_str(), e.hash.is_some())).collect();
        assert_eq!(by_path, BTreeMap::from([("a.typ", true), ("b.typ", false), ("new/n.typ", true)]));
        assert_eq!(hub.read("a.typ").unwrap().1, b"A edited");
        assert_ne!(hub.read("a.typ").unwrap().0.hash, a.hash);
        // A second scan finds nothing.
        hub.rescan().unwrap();
        assert_eq!(hub.seq(), 6);
        // A single path is refreshed by an operation, without a rescan.
        edit_by_hand(&hub, "c.typ", "C edited", 3000);
        let c = hub.read("c.typ").unwrap();
        assert_eq!((c.0.seq, c.1), (7, b"C edited".to_vec()));
    }

    #[test]
    fn size_limit_and_bad_paths() {
        let (_dir, hub) = open();
        let big = vec![0u8; usize::try_from(MAX_FILE_SIZE).unwrap() + 1];
        assert!(matches!(hub.write("big.bin", &Base::Any, &big), Err(Error::TooLarge { .. })));
        let limit = vec![0u8; usize::try_from(MAX_FILE_SIZE).unwrap()];
        assert!(hub.write("max.bin", &Base::Any, &limit).unwrap().is_ok());
        for path in ["../x", "/abs", ".git/x", "a.tmp", ".baluk/cache"] {
            assert!(matches!(hub.write(path, &Base::Any, b""), Err(Error::InvalidPath { .. })), "{path}");
            assert!(hub.read(path).is_err());
            assert!(hub.delete(path, &Base::Any).is_err());
        }
        // A file and a folder with one name.
        put(&hub, "a/b.typ", &Base::Any, "x").unwrap();
        assert!(matches!(hub.write("a", &Base::Any, b"x"), Err(Error::InvalidPath { .. })));
        put(&hub, "f", &Base::Any, "x").unwrap();
        assert!(matches!(hub.write("f/g", &Base::Any, b"x"), Err(Error::InvalidPath { .. })));
        // After the delete the folder name is free.
        hub.delete("a/b.typ", &Base::Any).unwrap().unwrap();
        assert!(put(&hub, "a", &Base::Absent, "x").is_ok());
    }
}
