//! The vault behind an interface: where the core gets note files from.
//!
//! All access to vault files goes through [`Storage`]: the file list, reading,
//! file info (size and modification time make the note version, see
//! [`crate::version`]). Today the vault is a directory on disk
//! ([`DirStorage`]); later (M5, sync) a database will stand next to it, and the
//! rest of the core will not have to change. Core layer tests run on
//! [`MemStorage`]: in memory, without a disk.
//!
//! A vault may report file changes ([`Storage::watch`]): a directory does it
//! through the OS watcher (`notify`), and then the file list comes from memory
//! while nothing changes in the directory. Without that the core walks the
//! files.
//!
//! Paths are relative, separated by `/`, with no leading `/`: `Network/SSH.typ`.
//! The design library (`/_baluk/`) and Typst packages are not the vault: the
//! compiler reads them ([`crate::world`]).

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;

/// Info about a vault file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMeta {
    pub is_dir: bool,
    pub len: u64,
    /// Modification time (`None` for a directory and where there is none).
    pub modified: Option<SystemTime>,
}

/// Where a vault reports changes: the paths of changed files (as in
/// [`Storage::list`], but internal `_` ones too), or `None`: the watcher broke
/// and changes may have been lost (from then on, no watcher).
pub type ChangeSink = Arc<dyn Fn(Option<Vec<String>>) + Send + Sync>;

/// The watcher runs while this object lives.
pub type WatchGuard = Box<dyn Any + Send + Sync>;

/// The vault files.
pub trait Storage: Send + Sync + fmt::Debug {
    /// Where the vault is: for the log and the disk cache key (different vaults
    /// may have notes with the same paths).
    fn location(&self) -> String;

    /// All files (not directories) except internal ones: a path with a name
    /// starting with `_` or `.` is not listed. Any order.
    fn list(&self) -> io::Result<Vec<String>>;

    /// All directories except internal ones (as in [`Self::list`]), empty ones
    /// too. By default, the directories that contain files. Any order.
    fn dirs(&self) -> io::Result<Vec<String>> {
        let mut out = std::collections::BTreeSet::new();
        for file in self.list()? {
            out.extend(file.match_indices('/').map(|(i, _)| file[..i].to_owned()));
        }
        Ok(out.into_iter().collect())
    }

    /// Info about a file or directory. `NotFound` if there is none.
    fn stat(&self, path: &str) -> io::Result<FileMeta>;

    /// The file contents.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;

    /// A path for error messages: the full one on disk, otherwise as is.
    fn display(&self, path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    /// Creates a **new** file (directories appear by themselves); if the file
    /// exists, `AlreadyExists` - nothing is overwritten. A read-only vault
    /// gives `Unsupported` (the default).
    fn create(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let _ = (path, data);
        Err(read_only())
    }

    /// New contents of an **existing** file (`NotFound` if there is none).
    /// A read-only vault gives `Unsupported` (the default).
    fn rewrite(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let _ = (path, data);
        Err(read_only())
    }

    /// Renames (moves) a file or directory; `AlreadyExists` if `to` exists;
    /// directories on the way to `to` appear by themselves. A read-only vault
    /// gives `Unsupported` (the default).
    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let _ = (from, to);
        Err(read_only())
    }

    /// Moves a file or a directory (a book) to the trash, where it can be
    /// restored from. `NotFound` if there is none; a read-only vault gives
    /// `Unsupported` (the default).
    fn trash(&self, path: &str) -> io::Result<()> {
        let _ = path;
        Err(read_only())
    }

    /// Reports file changes to `sink`. `Ok(None)` if the vault cannot (the
    /// default).
    fn watch(&self, sink: ChangeSink) -> io::Result<Option<WatchGuard>> {
        let _ = sink;
        Ok(None)
    }
}

/// The error of a write to a read-only vault.
fn read_only() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "the vault is read-only")
}

/// A Typst source (`.typ`).
pub fn is_typ(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| e == "typ")
}

/// An internal name (the `_baluk` library, `.git`): not in the file list.
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('_') || name.starts_with('.')
}

/// A vault that is a directory on disk.
#[derive(Debug, Clone)]
pub struct DirStorage {
    root: PathBuf,
    /// The trash: `None` is the system one; a directory gets the deleted items moved into it.
    trash: Option<PathBuf>,
    /// The file list, until the watcher reports a change.
    listed: Arc<Mutex<Listed>>,
}

#[derive(Debug, Default)]
struct Listed {
    /// The watcher runs, so the list can be kept.
    watching: bool,
    files: Option<Vec<String>>,
    dirs: Option<Vec<String>>,
}

impl Listed {
    /// Files or directories changed: the lists are built again.
    fn forget(&mut self) {
        self.files = None;
        self.dirs = None;
    }
}

impl DirStorage {
    /// The directory must exist; the path is made canonical.
    pub fn open(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = fs::canonicalize(root.as_ref())?;
        if !root.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotADirectory, "the vault is not a directory"));
        }
        Ok(Self { root, trash: None, listed: Arc::default() })
    }

    /// Deleted items ([`Storage::trash`]) go to the directory `dir` instead of
    /// the system trash (tests: keep the user's trash clean); `None` - the system one.
    #[must_use]
    pub fn with_trash(mut self, dir: Option<PathBuf>) -> Self {
        self.trash = dir;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The path on disk; `..`, absolute paths and prefixes are not allowed.
    fn full(&self, path: &str) -> io::Result<PathBuf> {
        let rel = Path::new(path);
        if rel.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
            Ok(self.root.join(rel))
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, format!("path outside the vault: {path}")))
        }
    }

    /// Files go to `out`, directories to `dirs`.
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>, dirs: &mut Vec<String>) -> io::Result<()> {
        for item in fs::read_dir(dir)? {
            let item = item?;
            let name = item.file_name();
            let Some(name) = name.to_str() else { continue }; // not UTF-8, not a note
            if is_hidden(name) {
                continue;
            }
            let child = if rel.is_empty() { name.to_owned() } else { format!("{rel}/{name}") };
            // The type comes from the directory entry, without following links.
            if item.file_type()?.is_dir() {
                Self::walk(&item.path(), &child, out, dirs)?;
                dirs.push(child);
            } else {
                out.push(child);
            }
        }
        Ok(())
    }
}

impl Storage for DirStorage {
    fn location(&self) -> String {
        self.root.display().to_string()
    }

    fn list(&self) -> io::Result<Vec<String>> {
        // Under the lock: a change during the walk waits for its end and drops
        // the kept list.
        let mut listed = self.listed.lock();
        if let Some(files) = &listed.files {
            return Ok(files.clone());
        }
        let (mut out, mut dirs) = (Vec::new(), Vec::new());
        Self::walk(&self.root, "", &mut out, &mut dirs)?;
        if listed.watching {
            listed.files = Some(out.clone());
            listed.dirs = Some(dirs);
        }
        Ok(out)
    }

    fn dirs(&self) -> io::Result<Vec<String>> {
        let mut listed = self.listed.lock();
        if let Some(dirs) = &listed.dirs {
            return Ok(dirs.clone());
        }
        let (mut out, mut dirs) = (Vec::new(), Vec::new());
        Self::walk(&self.root, "", &mut out, &mut dirs)?;
        if listed.watching {
            listed.files = Some(out);
            listed.dirs = Some(dirs.clone());
        }
        Ok(dirs)
    }

    fn watch(&self, sink: ChangeSink) -> io::Result<Option<WatchGuard>> {
        use notify::event::{EventKind, ModifyKind};
        use notify::{RecursiveMode, Watcher};
        let root = self.root.clone();
        let listed = self.listed.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            // Reading files (compiling, the walk itself) is not a change.
            if matches!(&event, Ok(e) if e.kind.is_access()) {
                return;
            }
            let mut state = listed.lock();
            // Editing contents does not change the file list.
            if !matches!(&event, Ok(e) if matches!(e.kind, EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Metadata(_)))) {
                state.forget();
            }
            match event {
                // The OS event queue overflowed (inotify): what changed is unknown.
                Ok(event) if event.need_rescan() => {
                    tracing::warn!("vault watcher: events lost; walking the files from now on");
                    state.watching = false;
                    drop(state);
                    sink(None);
                }
                Ok(event) => {
                    drop(state);
                    let paths: Vec<String> = event
                        .paths
                        .iter()
                        .filter_map(|p| p.strip_prefix(&root).ok())
                        .filter_map(|p| p.to_str().map(|p| p.replace(std::path::MAIN_SEPARATOR, "/")))
                        .filter(|p| !p.is_empty() && !p.split('/').any(|name| name.starts_with('.')))
                        .collect();
                    if !paths.is_empty() {
                        sink(Some(paths));
                    }
                }
                Err(e) => {
                    tracing::warn!("vault watcher: {e}; walking the files from now on");
                    state.watching = false;
                    drop(state);
                    sink(None);
                }
            }
        })
        .map_err(io::Error::other)?;
        watcher.watch(&self.root, RecursiveMode::Recursive).map_err(io::Error::other)?;
        let mut state = self.listed.lock();
        state.watching = true;
        state.forget();
        Ok(Some(Box::new(Unwatch { _watcher: watcher, listed: self.listed.clone() })))
    }

    fn stat(&self, path: &str) -> io::Result<FileMeta> {
        let meta = fs::metadata(self.full(path)?)?;
        Ok(FileMeta { is_dir: meta.is_dir(), len: meta.len(), modified: meta.modified().ok() })
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        fs::read(self.full(path)?)
    }

    fn display(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    fn create(&self, path: &str, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let full = self.full(path)?;
        if let Some(dir) = full.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&full)?;
        // The file list is built again, without waiting for the watcher event.
        self.listed.lock().forget();
        file.write_all(data)
    }

    fn rewrite(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let full = self.full(path)?;
        if !fs::metadata(&full)?.is_file() {
            return Err(io::Error::new(io::ErrorKind::IsADirectory, path.to_owned()));
        }
        fs::write(full, data)
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let (src, dst) = (self.full(from)?, self.full(to)?);
        if src == self.root || dst == self.root {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "the vault root cannot be renamed"));
        }
        fs::symlink_metadata(&src)?;
        // `ssh` -> `SSH` on Windows and macOS: `to` "exists" because it is the same file.
        if fs::symlink_metadata(&dst).is_ok() && !same_file(&src, &dst) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("already exists: {to}")));
        }
        if let Some(dir) = dst.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::rename(src, dst)?;
        self.listed.lock().forget();
        Ok(())
    }

    fn trash(&self, path: &str) -> io::Result<()> {
        let full = self.full(path)?;
        if full == self.root {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "the whole vault cannot be deleted"));
        }
        move_to_trash(&full, self.trash.as_deref())?;
        // The file list is built again, without waiting for the watcher event.
        self.listed.lock().forget();
        Ok(())
    }
}

/// The same file (another letter case on a case-insensitive file system).
#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::symlink_metadata(a), fs::symlink_metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// The same file: Windows file systems ignore case.
#[cfg(not(unix))]
fn same_file(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// Moves a file or directory to the system trash (`dir` = `None`) or to the
/// directory `dir` (tests), where it can be restored.
pub(crate) fn move_to_trash(full: &Path, dir: Option<&Path>) -> io::Result<()> {
    fs::symlink_metadata(full)?;
    match dir {
        None => system_trash(full),
        Some(dir) => {
            fs::create_dir_all(dir)?;
            let name = full.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let target = (1..10_000)
                .map(|n| dir.join(if n == 1 { name.clone() } else { format!("{name} ({n})") }))
                .find(|p| !p.exists())
                .ok_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "no free name in the trash"))?;
            fs::rename(full, target)
        }
    }
}

/// The system trash (freedesktop on Linux, the Windows and macOS trash).
#[cfg(not(any(target_os = "android", target_os = "ios", target_family = "wasm")))]
fn system_trash(path: &Path) -> io::Result<()> {
    trash::delete(path).map_err(io::Error::other)
}

/// No system trash (phone, browser): nowhere to delete to, so deletion is not possible yet.
#[cfg(any(target_os = "android", target_os = "ios", target_family = "wasm"))]
fn system_trash(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "this device has no trash"))
}

/// Stops the watcher: the file list is not kept any more.
struct Unwatch {
    _watcher: notify::RecommendedWatcher,
    listed: Arc<Mutex<Listed>>,
}

impl Drop for Unwatch {
    fn drop(&mut self) {
        let mut state = self.listed.lock();
        state.watching = false;
        state.forget();
    }
}

/// An in-memory vault for tests. The modification time is a write counter:
/// every write changes the file version. Reports writes ([`Storage::watch`]).
#[derive(Default)]
pub struct MemStorage {
    files: Mutex<MemFiles>,
    sink: Mutex<Option<ChangeSink>>,
}

impl fmt::Debug for MemStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemStorage").field("files", &self.files).finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct MemFiles {
    files: BTreeMap<String, (Vec<u8>, u64)>,
    clock: u64,
}

impl MemStorage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes a file (directories appear by themselves).
    pub fn write(&self, path: &str, data: impl Into<Vec<u8>>) {
        let mut f = self.files.lock();
        f.clock += 1;
        let clock = f.clock;
        f.files.insert(path.to_owned(), (data.into(), clock));
        drop(f);
        self.changed(path);
    }

    pub fn remove(&self, path: &str) {
        self.files.lock().files.remove(path);
        self.changed(path);
    }

    /// The watcher broke: changes may have been lost (for tests).
    pub fn lose_changes(&self) {
        let sink = self.sink.lock().clone();
        if let Some(sink) = sink {
            sink(None);
        }
    }

    fn changed(&self, path: &str) {
        let sink = self.sink.lock().clone();
        if let Some(sink) = sink {
            sink(Some(vec![path.to_owned()]));
        }
    }
}

impl Storage for MemStorage {
    fn location(&self) -> String {
        format!("memory:{:p}", std::ptr::from_ref(self))
    }

    fn list(&self) -> io::Result<Vec<String>> {
        let f = self.files.lock();
        Ok(f.files.keys().filter(|p| !p.split('/').any(is_hidden)).cloned().collect())
    }

    fn stat(&self, path: &str) -> io::Result<FileMeta> {
        let f = self.files.lock();
        if let Some((data, clock)) = f.files.get(path) {
            let modified = SystemTime::UNIX_EPOCH + Duration::from_nanos(*clock);
            return Ok(FileMeta { is_dir: false, len: data.len() as u64, modified: Some(modified) });
        }
        let prefix = format!("{path}/");
        if path.is_empty() || f.files.keys().any(|p| p.starts_with(&prefix)) {
            return Ok(FileMeta { is_dir: true, len: 0, modified: None });
        }
        Err(not_found(path))
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        if let Some((data, _)) = self.files.lock().files.get(path) {
            return Ok(data.clone());
        }
        match self.stat(path)? {
            FileMeta { is_dir: true, .. } => Err(io::Error::new(io::ErrorKind::IsADirectory, path.to_owned())),
            FileMeta { is_dir: false, .. } => Err(not_found(path)),
        }
    }

    fn watch(&self, sink: ChangeSink) -> io::Result<Option<WatchGuard>> {
        *self.sink.lock() = Some(sink);
        Ok(Some(Box::new(())))
    }

    fn trash(&self, path: &str) -> io::Result<()> {
        let prefix = format!("{path}/");
        let mut f = self.files.lock();
        let gone: Vec<String> = f.files.keys().filter(|p| *p == path || p.starts_with(&prefix)).cloned().collect();
        if gone.is_empty() || path.is_empty() {
            return Err(not_found(path));
        }
        for p in &gone {
            f.files.remove(p);
        }
        drop(f);
        self.changed(path);
        Ok(())
    }

    fn rewrite(&self, path: &str, data: &[u8]) -> io::Result<()> {
        if !self.files.lock().files.contains_key(path) {
            return Err(not_found(path));
        }
        self.write(path, data);
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let prefix = format!("{from}/");
        let is_moved = |p: &String| *p == from || p.starts_with(&prefix);
        let mut f = self.files.lock();
        if from.is_empty() || to.is_empty() || !f.files.keys().any(is_moved) {
            return Err(not_found(from));
        }
        let to_prefix = format!("{to}/");
        if from != to && f.files.keys().any(|p| p == to || p.starts_with(&to_prefix)) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("already exists: {to}")));
        }
        let moved: Vec<_> = f.files.extract_if(.., |p, _| is_moved(p)).collect();
        for (old, data) in moved {
            f.files.insert(format!("{to}{}", &old[from.len()..]), data);
        }
        drop(f);
        self.changed(from);
        self.changed(to);
        Ok(())
    }

    fn create(&self, path: &str, data: &[u8]) -> io::Result<()> {
        if self.files.lock().files.contains_key(path) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("file already exists: {path}")));
        }
        self.write(path, data);
        Ok(())
    }
}

/// `NotFound` for a missing in-memory file.
fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("no file {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(storage: &dyn Storage) {
        let mut list = storage.list().unwrap();
        list.sort();
        assert_eq!(list, ["a.typ", "Сеть/SSH.typ"]);
        assert_eq!(storage.read("Сеть/SSH.typ").unwrap(), b"ssh");
        assert!(storage.stat("Сеть").unwrap().is_dir);
        let meta = storage.stat("a.typ").unwrap();
        assert!(!meta.is_dir);
        assert_eq!(meta.len, 1);
        assert_eq!(storage.stat("нет.typ").unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(storage.read("_baluk/lib.typ").is_ok(), "internal files are readable but not listed");
    }

    #[test]
    fn dir_and_memory_agree() {
        let dir = tempfile::tempdir().unwrap();
        for (f, text) in [("a.typ", "a"), ("Сеть/SSH.typ", "ssh"), ("_baluk/lib.typ", ""), (".git/x", "")] {
            let p = dir.path().join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        let disk = DirStorage::open(dir.path()).unwrap();
        check(&disk);
        assert!(disk.read("../x").is_err(), "nothing outside the directory");

        let mem = MemStorage::new();
        for (f, text) in [("a.typ", "a"), ("Сеть/SSH.typ", "ssh"), ("_baluk/lib.typ", ""), (".git/x", "")] {
            mem.write(f, text);
        }
        check(&mem);
    }

    /// `create`: a new file with its directories; an existing one is not overwritten.
    #[test]
    fn create_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let disk = DirStorage::open(dir.path()).unwrap();
        let mem = MemStorage::new();
        for storage in [&disk as &dyn Storage, &mem] {
            storage.create("Новая/папка/x.typ", b"x").unwrap();
            assert_eq!(storage.read("Новая/папка/x.typ").unwrap(), b"x");
            assert!(storage.list().unwrap().contains(&"Новая/папка/x.typ".to_owned()), "the list is rebuilt");
            let again = storage.create("Новая/папка/x.typ", b"y").unwrap_err();
            assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(storage.read("Новая/папка/x.typ").unwrap(), b"x");
        }
        assert!(disk.create("../x.typ", b"").is_err(), "nothing outside the directory");
    }

    /// `trash`: a file or a whole directory; into a trash directory without
    /// overwriting a same-named item; not the root or foreign paths.
    #[test]
    fn trash_moves_away() {
        let dir = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let disk = DirStorage::open(dir.path()).unwrap().with_trash(Some(bin.path().to_owned()));
        let mem = MemStorage::new();
        for storage in [&disk as &dyn Storage, &mem] {
            for f in ["a.typ", "Книга/main.typ", "Книга/01.typ", "Книга2/main.typ"] {
                storage.create(f, b"x").unwrap();
            }
            storage.trash("a.typ").unwrap();
            storage.trash("Книга").unwrap();
            let mut list = storage.list().unwrap();
            list.sort();
            assert_eq!(list, ["Книга2/main.typ"], "the list is rebuilt");
            assert_eq!(storage.trash("a.typ").unwrap_err().kind(), io::ErrorKind::NotFound);
            assert!(storage.trash("").is_err(), "not the whole vault");
        }
        assert!(disk.trash("../x").is_err(), "nothing outside the directory");
        assert!(bin.path().join("Книга/01.typ").is_file());
        disk.create("a.typ", b"y").unwrap();
        disk.trash("a.typ").unwrap();
        assert_eq!(fs::read(bin.path().join("a.typ")).unwrap(), b"x", "the earlier item is not overwritten");
        assert_eq!(fs::read(bin.path().join("a.typ (2)")).unwrap(), b"y");
    }

    /// Waits until `ok()` becomes true (OS events do not come at once).
    fn eventually(ok: impl Fn() -> bool) -> bool {
        (0..200).any(|_| {
            ok() || {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
        })
    }

    #[test]
    fn dir_watch_reports_changes_and_keeps_list() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.typ"), "a").unwrap();
        let disk = DirStorage::open(dir.path()).unwrap();
        let seen = Arc::new(Mutex::new(Vec::<Option<Vec<String>>>::new()));
        let sink_seen = seen.clone();
        let guard =
            disk.watch(Arc::new(move |paths| sink_seen.lock().push(paths))).unwrap().expect("a directory can watch");
        assert_eq!(disk.list().unwrap(), ["a.typ"]);
        assert!(disk.listed.lock().files.is_some(), "the list is kept");

        fs::create_dir(dir.path().join("Сеть")).unwrap();
        fs::write(dir.path().join("Сеть/SSH.typ"), "ssh").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/x"), "").unwrap();
        // A file in a just created folder may come as one event of the folder.
        assert!(eventually(|| seen.lock().iter().flatten().flatten().any(|p| p.starts_with("Сеть"))));
        fs::write(dir.path().join("a.typ"), "aa").unwrap();
        assert!(eventually(|| seen.lock().iter().flatten().flatten().any(|p| p == "a.typ")));
        assert!(
            !seen.lock().iter().flatten().flatten().any(|p| p.starts_with(".git")),
            "internal . names are not events"
        );
        let mut list = disk.list().unwrap();
        list.sort();
        assert_eq!(list, ["a.typ", "Сеть/SSH.typ"], "a change dropped the kept list");

        drop(guard);
        assert!(disk.listed.lock().files.is_none());
        disk.list().unwrap();
        assert!(disk.listed.lock().files.is_none(), "without a watcher the list is not kept");
    }

    #[test]
    fn memory_write_changes_time() {
        let mem = MemStorage::new();
        mem.write("a.typ", "1");
        let first = mem.stat("a.typ").unwrap();
        mem.write("a.typ", "1");
        assert_ne!(first.modified, mem.stat("a.typ").unwrap().modified);
        mem.remove("a.typ");
        assert!(mem.stat("a.typ").is_err());
    }
}
