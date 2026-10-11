//! The device's copy of a vault: [`Tree`], the directory implementation
//! [`DirTree`] and the in-memory one for tests [`MemTree`].
//!
//! The user's notes must never be lost by a sync mistake, so a tree never
//! deletes: the content that sync removes or overwrites is moved aside. A
//! [`DirTree`] moves it to `<removed_dir>/<unix time>/<path>`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use super::path::{is_skipped_dir, is_synced};
use crate::fsutil::write_atomic;

/// A file of a tree as a scan sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    /// The modification time, nanoseconds since the Unix epoch. Together with
    /// the size it tells whether the file changed without reading it.
    pub mtime: i64,
}

/// The device's copy of a vault. Paths are checked by [`super::path`] before
/// they get here. All methods take `&self`: the user may edit files while a
/// sync round runs.
pub trait Tree {
    /// Every synced regular file.
    fn list(&self) -> io::Result<Vec<FileInfo>>;
    /// The file at `path`; `NotFound` if there is none.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
    /// Writes atomically, creating folders. The previous content of an
    /// existing file is moved aside, not lost. Returns the new file info.
    fn write(&self, path: &str, data: &[u8]) -> io::Result<FileInfo>;
    /// Moves the file aside (does not delete). No file - not an error.
    fn remove(&self, path: &str) -> io::Result<()>;
    /// The info of the file, `None` if there is none.
    fn stat(&self, path: &str) -> io::Result<Option<FileInfo>>;
}

/// The modification time in nanoseconds since the Unix epoch.
pub(crate) fn mtime_nanos(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()).unwrap_or(i64::MAX),
        Err(before) => -i64::try_from(before.duration().as_nanos()).unwrap_or(i64::MAX),
    }
}

/// The info of a regular file; `None` for no file, a folder or a symlink.
pub(crate) fn stat_file(root: &Path, path: &str) -> io::Result<Option<FileInfo>> {
    match fs::symlink_metadata(root.join(path)) {
        Ok(meta) if meta.is_file() => {
            Ok(Some(FileInfo { path: path.to_owned(), size: meta.len(), mtime: mtime_nanos(meta.modified()?) }))
        }
        Ok(_) => Ok(None),
        // A parent is a file: there is no such path.
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Every synced regular file under `root`, ordered by path. Symlinks are not
/// followed, `skip` (a folder inside `root`) is not entered. A missing `root`
/// is an empty tree.
pub(crate) fn walk(root: &Path, skip: Option<&Path>) -> io::Result<Vec<FileInfo>> {
    let mut found = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let file_type = entry.file_type()?;
            let path = format!("{prefix}{name}");
            if file_type.is_dir() {
                if !is_skipped_dir(&name) && skip != Some(entry.path().as_path()) {
                    stack.push((entry.path(), format!("{path}/")));
                }
            } else if file_type.is_file() && is_synced(&path) {
                let meta = match entry.metadata() {
                    Ok(meta) => meta,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e),
                };
                found.push(FileInfo { path, size: meta.len(), mtime: mtime_nanos(meta.modified()?) });
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// Moves a file, creating the folders of the destination. Falls back to
/// copy + delete when a rename is impossible (another filesystem).
pub(crate) fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

/// Removes empty folders from the one holding `file` up to (excluding) `root`.
pub(crate) fn remove_empty_parents(root: &Path, file: &Path) {
    let mut dir = file.parent();
    while let Some(current) = dir {
        if current == root || !current.starts_with(root) || fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

fn invalid_path(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("not a synced path: {path:?}"))
}

/// A vault folder on disk.
#[derive(Debug, Clone)]
pub struct DirTree {
    root: PathBuf,
    removed: PathBuf,
}

impl DirTree {
    /// `root` is the vault folder, `removed_dir` is where removed and
    /// overwritten content goes (it may be inside `root`: it is not listed).
    pub fn new(root: impl Into<PathBuf>, removed_dir: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), removed: removed_dir.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn removed_dir(&self) -> &Path {
        &self.removed
    }

    fn resolve(&self, path: &str) -> io::Result<PathBuf> {
        if is_synced(path) { Ok(self.root.join(path)) } else { Err(invalid_path(path)) }
    }

    /// Moves the file to `<removed>/<unix time>/<path>`; a second move of the
    /// same path in the same second gets `<unix time>-<n>`.
    fn move_aside(&self, path: &str, abs: &Path) -> io::Result<()> {
        let now = mtime_nanos(SystemTime::now()) / 1_000_000_000;
        let mut n = 0u32;
        let dest = loop {
            let stamp = if n == 0 { now.to_string() } else { format!("{now}-{n}") };
            let dest = self.removed.join(stamp).join(path);
            if fs::symlink_metadata(&dest).is_err() {
                break dest;
            }
            n += 1;
        };
        move_file(abs, &dest)
    }

    /// Deletes the `removed_dir` entries (`<unix time>` folders) older than
    /// `max_age` at `now`. Returns how many were deleted. Folders with other
    /// names are left alone.
    pub fn prune_removed(&self, max_age: Duration, now: SystemTime) -> io::Result<usize> {
        let entries = match fs::read_dir(&self.removed) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };
        let limit = mtime_nanos(now) / 1_000_000_000 - i64::try_from(max_age.as_secs()).unwrap_or(i64::MAX);
        let mut pruned = 0;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let stamp = name.split('-').next().unwrap_or_default();
            let Ok(stamp) = stamp.parse::<i64>() else {
                continue;
            };
            if stamp < limit && entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
                pruned += 1;
            }
        }
        Ok(pruned)
    }
}

impl Tree for DirTree {
    fn list(&self) -> io::Result<Vec<FileInfo>> {
        walk(&self.root, Some(&self.removed))
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        fs::read(self.resolve(path)?)
    }

    fn write(&self, path: &str, data: &[u8]) -> io::Result<FileInfo> {
        let abs = self.resolve(path)?;
        if stat_file(&self.root, path)?.is_some() {
            self.move_aside(path, &abs)?;
        }
        write_atomic(&abs, data)?;
        stat_file(&self.root, path)?.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        let abs = self.resolve(path)?;
        if stat_file(&self.root, path)?.is_some() {
            self.move_aside(path, &abs)?;
            remove_empty_parents(&self.root, &abs);
        }
        Ok(())
    }

    fn stat(&self, path: &str) -> io::Result<Option<FileInfo>> {
        self.resolve(path)?;
        stat_file(&self.root, path)
    }
}

#[derive(Debug, Default)]
struct MemInner {
    files: BTreeMap<String, (Vec<u8>, i64)>,
    removed: Vec<(String, Vec<u8>)>,
    clock: i64,
}

/// A tree in memory, for tests. Every change gets a new modification time
/// from a counter, so there is nothing to sleep for.
#[derive(Debug, Default)]
pub struct MemTree {
    inner: Mutex<MemInner>,
}

impl MemTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// An edit by the user: sets the content and a new modification time,
    /// keeps nothing aside.
    pub fn set(&self, path: &str, data: &[u8]) {
        let mut inner = self.inner.lock();
        inner.clock += 1;
        let clock = inner.clock;
        inner.files.insert(path.to_owned(), (data.to_vec(), clock));
    }

    /// An edit with an explicit modification time.
    pub fn set_with_mtime(&self, path: &str, data: &[u8], mtime: i64) {
        self.inner.lock().files.insert(path.to_owned(), (data.to_vec(), mtime));
    }

    /// A deletion by the user, keeps nothing aside.
    pub fn delete(&self, path: &str) {
        self.inner.lock().files.remove(path);
    }

    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        self.inner.lock().files.get(path).map(|(data, _)| data.clone())
    }

    /// Every file with its content.
    pub fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        self.inner.lock().files.iter().map(|(path, (data, _))| (path.clone(), data.clone())).collect()
    }

    /// What sync moved aside: path and old content, in order.
    pub fn removed(&self) -> Vec<(String, Vec<u8>)> {
        self.inner.lock().removed.clone()
    }
}

impl Tree for MemTree {
    fn list(&self) -> io::Result<Vec<FileInfo>> {
        Ok(self
            .inner
            .lock()
            .files
            .iter()
            .filter(|(path, _)| is_synced(path))
            .map(|(path, (data, mtime))| FileInfo { path: path.clone(), size: data.len() as u64, mtime: *mtime })
            .collect())
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.get(path).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, path: &str, data: &[u8]) -> io::Result<FileInfo> {
        if !is_synced(path) {
            return Err(invalid_path(path));
        }
        let mut inner = self.inner.lock();
        inner.clock += 1;
        let clock = inner.clock;
        if let Some((old, _)) = inner.files.insert(path.to_owned(), (data.to_vec(), clock)) {
            inner.removed.push((path.to_owned(), old));
        }
        Ok(FileInfo { path: path.to_owned(), size: data.len() as u64, mtime: clock })
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        let mut inner = self.inner.lock();
        if let Some((old, _)) = inner.files.remove(path) {
            inner.removed.push((path.to_owned(), old));
        }
        Ok(())
    }

    fn stat(&self, path: &str) -> io::Result<Option<FileInfo>> {
        Ok(self.inner.lock().files.get(path).map(|(data, mtime)| FileInfo {
            path: path.to_owned(),
            size: data.len() as u64,
            mtime: *mtime,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    fn write_file(root: &Path, path: &str, data: &[u8]) {
        let abs = root.join(path);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(abs, data).unwrap();
    }

    fn removed_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let Ok(entries) = fs::read_dir(&current) else {
                continue;
            };
            for entry in entries {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(dir).unwrap();
                    // Drop the `<unix time>` folder.
                    let rel: PathBuf = rel.components().skip(1).collect();
                    out.push((rel.to_string_lossy().into_owned(), fs::read(&path).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn list_skips_what_is_not_synced() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(root, "a.typ", b"a");
        write_file(root, "sub/b.typ", b"b");
        write_file(root, "_folder.toml", b"f");
        write_file(root, "img/p.png", b"p");
        write_file(root, ".baluk/settings.json", b"{}");
        write_file(root, ".baluk/cache.bin", b"c");
        write_file(root, ".git/config", b"g");
        write_file(root, ".trash/old.typ", b"t");
        write_file(root, "a.typ.tmp", b"tmp");
        write_file(root, "sub/.DS_Store", b"j");
        write_file(root, "Thumbs.db", b"j");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("a.typ"), root.join("link.typ")).unwrap();
            std::os::unix::fs::symlink(root.join("sub"), root.join("linkdir")).unwrap();
        }
        let tree = DirTree::new(root, dir.path().join("elsewhere"));
        let paths: Vec<_> = tree.list().unwrap().into_iter().map(|f| f.path).collect();
        assert_eq!(paths, [".baluk/settings.json", "_folder.toml", "a.typ", "img/p.png", "sub/b.typ"]);
        let info = tree.stat("a.typ").unwrap().unwrap();
        assert_eq!((info.size, tree.stat("nope").unwrap()), (1, None));
        assert!(tree.stat("a.typ.tmp").is_err());
        assert!(tree.read("../x").is_err());
        let empty = DirTree::new(root.join("missing"), root.join("r"));
        assert!(empty.list().unwrap().is_empty());
    }

    #[test]
    fn removed_dir_inside_the_root_is_not_listed() {
        let dir = tempfile::tempdir().unwrap();
        let tree = DirTree::new(dir.path(), dir.path().join("removed"));
        tree.write("a.typ", b"1").unwrap();
        tree.write("a.typ", b"2").unwrap();
        assert_eq!(tree.list().unwrap().len(), 1);
        assert_eq!(removed_files(&dir.path().join("removed")).len(), 1);
    }

    #[test]
    fn write_creates_folders_and_keeps_the_old_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("vault");
        let removed = dir.path().join("removed");
        let tree = DirTree::new(&root, &removed);
        let info = tree.write("a/b/c.typ", b"one").unwrap();
        assert_eq!((info.size, info.path.as_str()), (3, "a/b/c.typ"));
        assert!(removed_files(&removed).is_empty());
        tree.write("a/b/c.typ", b"two").unwrap();
        tree.write("a/b/c.typ", b"three").unwrap();
        assert_eq!(tree.read("a/b/c.typ").unwrap(), b"three");
        let kept = removed_files(&removed);
        assert_eq!(kept, [("a/b/c.typ".to_owned(), b"one".to_vec()), ("a/b/c.typ".to_owned(), b"two".to_vec()),]);
        assert!(!root.join("a/b/c.typ.tmp").exists());
    }

    #[test]
    fn remove_moves_aside_and_cleans_empty_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("vault");
        let removed = dir.path().join("removed");
        let tree = DirTree::new(&root, &removed);
        tree.write("a/b/c.typ", b"c").unwrap();
        tree.write("a/keep.typ", b"k").unwrap();
        tree.remove("a/b/c.typ").unwrap();
        assert!(!root.join("a/b").exists());
        assert!(root.join("a/keep.typ").exists());
        assert_eq!(removed_files(&removed), [("a/b/c.typ".to_owned(), b"c".to_vec())]);
        tree.remove("a/keep.typ").unwrap();
        assert!(!root.join("a").exists());
        assert!(root.exists());
        // No file: not an error.
        tree.remove("a/keep.typ").unwrap();
    }

    #[test]
    fn prune_removed_by_age() {
        let dir = tempfile::tempdir().unwrap();
        let removed = dir.path().join("removed");
        write_file(&removed, "100/old.typ", b"o");
        write_file(&removed, "100-1/old.typ", b"o");
        write_file(&removed, "9000000/new.typ", b"n");
        write_file(&removed, "notes/x.typ", b"x");
        let tree = DirTree::new(dir.path().join("vault"), &removed);
        let day = Duration::from_hours(24);
        let now = UNIX_EPOCH + Duration::from_hours(2500);
        assert_eq!(tree.prune_removed(day, now).unwrap(), 2);
        assert!(!removed.join("100").exists());
        assert!(removed.join("9000000/new.typ").exists());
        assert!(removed.join("notes/x.typ").exists());
        assert_eq!(tree.prune_removed(day, now).unwrap(), 0);
        let none = DirTree::new(dir.path(), dir.path().join("no-such"));
        assert_eq!(none.prune_removed(day, now).unwrap(), 0);
    }

    #[test]
    fn mtime_comes_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let tree = DirTree::new(dir.path(), dir.path().join("r"));
        tree.write("a.typ", b"x").unwrap();
        let file = File::options().write(true).open(dir.path().join("a.typ")).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(1000)).unwrap();
        assert_eq!(tree.stat("a.typ").unwrap().unwrap().mtime, 1_000_000_000_000);
    }

    #[test]
    fn mem_tree_behaves_like_a_dir_tree() {
        let tree = MemTree::new();
        tree.set("a.typ", b"1");
        let before = tree.stat("a.typ").unwrap().unwrap();
        tree.set("a.typ", b"1");
        assert_ne!(tree.stat("a.typ").unwrap().unwrap().mtime, before.mtime);
        tree.write("a.typ", b"2").unwrap();
        tree.remove("a.typ").unwrap();
        assert_eq!(tree.removed(), [("a.typ".to_owned(), b"1".to_vec()), ("a.typ".to_owned(), b"2".to_vec())]);
        assert!(tree.list().unwrap().is_empty());
        assert!(tree.write(".git/x", b"").is_err());
    }
}
