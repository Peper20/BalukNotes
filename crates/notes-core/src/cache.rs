//! The disk cache of builds: after a restart, a note whose files did not
//! change is not compiled again (for a book that is seconds).
//!
//! Two files per note in the vault's directory (`<cache>/<vault hash>/`):
//! - `<path hash>.json`, the **record** ([`Record`]): the version and the file
//!   list, errors and warnings, the build time (warming order), the rendering
//!   tag; it is small, read by the "is it built" check and by warming;
//! - `<path hash>.raw.json`, the raw rendering (before figure processing,
//!   which depends on the settings). The record and the rendering file carry
//!   the same tag, so a failure between writing the two files gives a miss,
//!   not someone else's page.
//!
//! Build errors are recorded too (with the earlier good rendering, if there
//! was one): a note with an error is not rebuilt on every start.
//!
//! A record is good if **the same rendering code** made it - the [`stamp`]:
//! the format version, the hash of the rendering sources (`build.rs`), the
//! embedded library and the font set - and the version of the note files
//! matches the current one. Rebuilding the server or the client keeps the cache.
//!
//! The cache is not the source of truth: any read or write error is just a
//! miss (with a warning in the log), and the directory can be deleted at any
//! time. Cleanup: [`DiskCache::prune`] (deleted notes, old foreign records,
//! the size limit).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::diag::Diagnostic;
use crate::fsutil::write_atomic;
use crate::render::{LinkRef, Rendered};
use crate::vault::NoteId;
use crate::version::{Dep, StableHasher};

/// The record format version: change it on any change of [`Record`] or
/// [`Rendered`].
pub const FORMAT: u32 = 4;

/// Disk cache limits, device settings ([`crate::settings::Device`]); the
/// defaults are those of a computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskLimits {
    /// The size limit (all vaults together).
    pub size: u64,
    /// Foreign records (another vault, another app build) not updated for this
    /// long are removed on cleanup.
    pub foreign_ttl: Duration,
}

impl Default for DiskLimits {
    fn default() -> Self {
        Self { size: 512 << 20, foreign_ttl: Duration::from_hours(14 * 24) }
    }
}

/// What is known about a note build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// The version of the note files at the time of reading ([`crate::version::combine`]).
    pub files: String,
    pub deps: Vec<Dep>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// How long the build took, in ms.
    pub build_ms: u64,
    /// The tag of the raw rendering: `None` means there is no rendering (an
    /// error, and no good build before it).
    pub raw: Option<String>,
    /// Links from the last good rendering (`Rendered.links`), computed ones
    /// too: the link index adds them to what it parses from the sources.
    #[serde(default)]
    pub links: Vec<LinkRef>,
}

/// The record file.
#[derive(Serialize, Deserialize)]
struct RecordFile {
    stamp: String,
    id: String,
    #[serde(flatten)]
    record: Record,
}

/// The rendering file.
#[derive(Serialize, Deserialize)]
struct RawFile {
    tag: String,
    raw: Rendered,
}

/// The disk cache of one vault.
#[derive(Debug)]
pub struct DiskCache {
    /// The cache root (all vaults).
    root: PathBuf,
    /// This vault's directory.
    dir: PathBuf,
    /// The tag of the rendering code.
    stamp: String,
}

/// The result of a cleanup.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pruned {
    /// How many files were removed.
    pub removed: usize,
    /// How many bytes are left.
    pub bytes: u64,
}

impl DiskCache {
    /// `vault` is where the vault is ([`crate::storage::Storage::location`]):
    /// different vaults may have notes with the same paths.
    pub fn new(root: impl Into<PathBuf>, vault: &str, stamp: String) -> Self {
        let root = root.into();
        let dir = root.join(StableHasher::new().str(vault).hex());
        Self { root, dir, stamp }
    }

    fn base(&self, id: &NoteId) -> PathBuf {
        // The file name is a hash of the path: note paths may have any characters.
        self.dir.join(StableHasher::new().str(id.as_str()).hex())
    }

    fn record_path(&self, id: &NoteId) -> PathBuf {
        self.base(id).with_extension("json")
    }

    fn raw_path(&self, id: &NoteId) -> PathBuf {
        self.base(id).with_extension("raw.json")
    }

    /// The note record, if the same rendering code made it. Whether it is good
    /// for the current files is the caller's call (by `files` and `deps`).
    pub fn record(&self, id: &NoteId) -> Option<Record> {
        let file: RecordFile = read_json(&self.record_path(id))?;
        (file.stamp == self.stamp && file.id == id.as_str()).then_some(file.record)
    }

    /// The raw rendering for a record (the tags must match).
    pub fn raw(&self, id: &NoteId, record: &Record) -> Option<Rendered> {
        let tag = record.raw.as_ref()?;
        let file: RawFile = read_json(&self.raw_path(id))?;
        (&file.tag == tag).then_some(file.raw)
    }

    /// Writes a build. `raw` is a new rendering (its tag is `record.raw`);
    /// `None` means the rendering did not change (disk has the one in the tag).
    pub fn store(&self, id: &NoteId, record: &Record, raw: Option<&Rendered>) {
        #[derive(Serialize)]
        struct RawRef<'a> {
            tag: &'a str,
            raw: &'a Rendered,
        }
        #[derive(Serialize)]
        struct RecordRef<'a> {
            stamp: &'a str,
            id: &'a str,
            #[serde(flatten)]
            record: &'a Record,
        }
        let result = (|| {
            // The rendering first, then the record: a failure between them is a miss.
            if let (Some(raw), Some(tag)) = (raw, &record.raw) {
                write_atomic(&self.raw_path(id), &serde_json::to_vec(&RawRef { tag, raw })?)?;
            }
            let file = RecordRef { stamp: &self.stamp, id: id.as_str(), record };
            write_atomic(&self.record_path(id), &serde_json::to_vec(&file)?)
        })();
        if let Err(e) = result {
            tracing::warn!("cache for {id} not written: {e}");
        }
    }

    /// Cleanup: records of deleted notes (`alive` says whether a note exists),
    /// orphans and junk, foreign records older than `limits.foreign_ttl`, files
    /// of the old format; then, if the cache is bigger than `limits.size`, the
    /// oldest records.
    pub fn prune(&self, alive: &dyn Fn(&str) -> bool, limits: DiskLimits) -> Pruned {
        let limit = limits.size;
        let now = SystemTime::now();
        let old = |mtime: SystemTime| now.duration_since(mtime).is_ok_and(|age| age > limits.foreign_ttl);
        let mut pruned = Pruned::default();
        let remove = |path: &Path, pruned: &mut Pruned| {
            let result = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
            match result {
                Ok(()) => pruned.removed += 1,
                Err(e) => tracing::warn!("cache {}: not removed: {e}", path.display()),
            }
        };
        // Groups that can be removed whole: (time, size, files).
        let mut groups: Vec<(SystemTime, u64, Vec<PathBuf>)> = Vec::new();
        for item in list(&self.root) {
            if item.path == self.dir {
                continue;
            }
            if item.is_dir {
                let files = list(&item.path);
                let newest = files.iter().map(|f| f.mtime).max().unwrap_or(item.mtime);
                if old(newest) {
                    remove(&item.path, &mut pruned);
                } else {
                    groups.push((newest, files.iter().map(|f| f.len).sum(), vec![item.path]));
                }
            } else {
                remove(&item.path, &mut pruned); // the old format: files right in the root
            }
        }
        // Our own vault: a record + a rendering by name.
        let mut own: HashMap<String, (Option<Item>, Option<Item>)> = HashMap::new();
        for item in list(&self.dir) {
            let name = item.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(base) = name.strip_suffix(".raw.json") {
                own.entry(base.to_owned()).or_default().1 = Some(item);
            } else if let Some(base) = name.strip_suffix(".json") {
                own.entry(base.to_owned()).or_default().0 = Some(item);
            } else {
                remove(&item.path, &mut pruned); // `.tmp` after a failure and the like
            }
        }
        for (record, raw) in own.into_values() {
            let head = record.as_ref().and_then(|r| read_json::<RecordHead>(&r.path));
            let keep = match (&record, &head) {
                (Some(r), Some(h)) => {
                    let ours = h.stamp == self.stamp;
                    alive(&h.id) && (ours || !old(r.mtime)) && (h.raw.is_some() == raw.is_some() || !ours)
                }
                _ => false, // an orphan or a broken record
            };
            let paths: Vec<PathBuf> = record.iter().chain(raw.iter()).map(|i| i.path.clone()).collect();
            if keep {
                let bytes = record.iter().chain(raw.iter()).map(|i| i.len).sum();
                groups.push((record.as_ref().map_or(now, |r| r.mtime), bytes, paths));
            } else {
                for p in &paths {
                    remove(p, &mut pruned);
                }
            }
        }
        // The size limit: the oldest first.
        groups.sort_by_key(|g| g.0);
        let mut total: u64 = groups.iter().map(|g| g.1).sum();
        for (_, bytes, paths) in &groups {
            if total <= limit {
                break;
            }
            for p in paths {
                remove(p, &mut pruned);
            }
            total -= bytes;
        }
        pruned.bytes = total;
        pruned
    }
}

/// The head of a record, for cleanup.
#[derive(Deserialize)]
struct RecordHead {
    stamp: String,
    id: String,
    raw: Option<String>,
}

#[derive(Debug)]
struct Item {
    path: PathBuf,
    is_dir: bool,
    len: u64,
    mtime: SystemTime,
}

fn list(dir: &Path) -> Vec<Item> {
    let Ok(read) = fs::read_dir(dir) else { return vec![] };
    read.flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some(Item { path: e.path(), is_dir: meta.is_dir(), len: meta.len(), mtime })
        })
        .collect()
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!("cache {}: {e}", path.display());
            return None;
        }
    };
    match serde_json::from_slice(&data) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("cache {}: {e}; will rebuild", path.display());
            None
        }
    }
}

/// The tag of the rendering code: the format version, the app version, the hash
/// of the rendering sources, fonts and Typst (`build.rs`); `extra` is the
/// embedded library and the font set (the caller computes them).
pub fn stamp(extra: &[u64]) -> String {
    let mut h = StableHasher::new();
    h.u64(u64::from(FORMAT)).str(env!("CARGO_PKG_VERSION")).str(env!("NOTES_RENDER_HASH"));
    for x in extra {
        h.u64(*x);
    }
    format!("{FORMAT}-{}", h.hex())
}

/// A new rendering tag for a record.
pub fn new_tag(files: &str) -> String {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    StableHasher::new().str(files).bytes(&nanos.to_le_bytes()).hex()
}

/// The cache directory inside the data directory (`<data>/cache`; rendering in
/// `pages/`, fonts in `fonts/`).
pub fn default_dir(data: &Path) -> PathBuf {
    data.join("cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(body: &str) -> Rendered {
        Rendered {
            title: Some("T".into()),
            styles: String::new(),
            body: body.into(),
            headings: vec![],
            links: vec![],
            tags: vec![],
            sanitizer: None,
        }
    }

    fn record(files: &str, raw: Option<&str>) -> Record {
        Record {
            files: files.into(),
            deps: vec![Dep::Vault("a.typ".into())],
            errors: vec![],
            warnings: vec![],
            build_ms: 7,
            raw: raw.map(Into::into),
            links: vec![],
        }
    }

    #[test]
    fn round_trip_and_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path(), "/хранилище", "м1".into());
        let id = NoteId::new("Сеть/SSH").unwrap();
        assert!(cache.record(&id).is_none());

        let rec = record("v1", Some("t1"));
        cache.store(&id, &rec, Some(&rendered("<p>x</p>")));
        assert_eq!(cache.record(&id).as_ref(), Some(&rec));
        assert_eq!(cache.raw(&id, &rec).unwrap().body, "<p>x</p>");

        // An error: the record is new, the rendering is the earlier one (same tag).
        let failed = Record { errors: vec![Diagnostic::error("сломано")], ..record("v2", Some("t1")) };
        cache.store(&id, &failed, None);
        assert_eq!(cache.record(&id).unwrap().errors.len(), 1);
        assert_eq!(cache.raw(&id, &failed).unwrap().body, "<p>x</p>");
        assert!(cache.raw(&id, &record("v2", Some("чужая"))).is_none(), "tags do not match: a miss");

        let other_build = DiskCache::new(dir.path(), "/хранилище", "м2".into());
        assert!(other_build.record(&id).is_none(), "another rendering code");
        let other_vault = DiskCache::new(dir.path(), "/другое", "м1".into());
        assert!(other_vault.record(&id).is_none(), "another vault");

        fs::write(cache.record_path(&id), "испорчено").unwrap();
        assert!(cache.record(&id).is_none());
    }

    #[test]
    fn prune_removes_dead_orphans_and_old_format() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path(), "/хранилище", "м1".into());
        let id = |s| NoteId::new(s).unwrap();
        for name in ["Живая", "Удалённая"] {
            cache.store(&id(name), &record("v", Some("t")), Some(&rendered("x")));
        }
        fs::write(dir.path().join("0123456789abcdef.json"), "{}").unwrap(); // the old format
        let orphan = cache.raw_path(&id("Сирота"));
        fs::write(&orphan, "{}").unwrap();
        let fresh_foreign = DiskCache::new(dir.path(), "/другое", "м1".into());
        fresh_foreign.store(&id("Чужая"), &record("v", None), None);

        let pruned = cache.prune(&|id| id == "Живая", DiskLimits::default());
        assert_eq!(pruned.removed, 4, "the deleted one (2 files), an orphan, the old format");
        assert!(cache.record(&id("Живая")).is_some());
        assert!(cache.record(&id("Удалённая")).is_none());
        assert!(!orphan.exists());
        assert!(fresh_foreign.record(&id("Чужая")).is_some(), "a fresh foreign record is left alone");

        // The size limit: no more than the limit stays, the oldest goes first.
        let pruned = cache.prune(&|_| true, DiskLimits { size: 0, ..DiskLimits::default() });
        assert_eq!(pruned.bytes, 0);
        assert!(cache.record(&id("Живая")).is_none());
    }

    #[test]
    fn stamp_depends_on_library_and_fonts() {
        assert_eq!(stamp(&[1, 2]), stamp(&[1, 2]));
        assert_ne!(stamp(&[1, 2]), stamp(&[1, 3]));
        assert!(stamp(&[]).starts_with(&format!("{FORMAT}-")));
    }
}
