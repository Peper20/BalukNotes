//! The core's vault as the sync engine's [`Tree`]: what a round writes or
//! removes goes through [`MemStorage`], so the core sees it (the vault events,
//! the journal the host persists), and the previous content is kept aside
//! until the host takes it ([`VaultTree::take_removed`]) to OPFS
//! `removed/<path>` (the last version of each path).

use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use notes_core::version::StableHasher;
use notes_store::storage::{MemStorage, Storage};
use notes_store::sync::path::is_synced;
use notes_store::sync::{FileInfo, Tree};

/// The vault in memory as a sync tree.
#[derive(Debug)]
pub struct VaultTree {
    vault: Arc<MemStorage>,
    /// What sync replaced or removed and the host has not taken yet.
    removed: Mutex<BTreeMap<String, Vec<u8>>>,
}

/// The "modification time" of a file: a fingerprint of its content. A file in
/// memory has no time that survives a restart of the worker, and the sync
/// state keeps the pair (size, time) of the last round: with the content's
/// fingerprint an edit is never mistaken for the synced version, and a
/// restart does not make every file look changed.
fn fingerprint(data: &[u8]) -> i64 {
    i64::from_ne_bytes(StableHasher::new().bytes(data).finish().to_ne_bytes())
}

fn invalid_path(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("not a synced path: {path:?}"))
}

impl VaultTree {
    pub fn new(vault: Arc<MemStorage>) -> Self {
        Self { vault, removed: Mutex::default() }
    }

    /// What sync replaced or removed since the last call: path and the old
    /// content, one per path (the last one).
    pub fn take_removed(&self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut *self.removed.lock().unwrap_or_else(PoisonError::into_inner)).into_iter().collect()
    }

    fn info(path: &str, data: &[u8]) -> FileInfo {
        FileInfo { path: path.to_owned(), size: data.len() as u64, mtime: fingerprint(data) }
    }

    /// Keeps the current content of `path` aside; `false`: there is none.
    fn keep_aside(&self, path: &str) -> bool {
        match self.vault.read(path) {
            Ok(old) => {
                self.removed.lock().unwrap_or_else(PoisonError::into_inner).insert(path.to_owned(), old);
                true
            }
            Err(_) => false,
        }
    }
}

impl Tree for VaultTree {
    fn list(&self) -> io::Result<Vec<FileInfo>> {
        Ok(self
            .vault
            .paths()
            .into_iter()
            .filter(|path| is_synced(path))
            .filter_map(|path| self.vault.read(&path).ok().map(|data| Self::info(&path, &data)))
            .collect())
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.vault.read(path)
    }

    fn write(&self, path: &str, data: &[u8]) -> io::Result<FileInfo> {
        if !is_synced(path) {
            return Err(invalid_path(path));
        }
        self.keep_aside(path);
        self.vault.write(path, data);
        Ok(Self::info(path, data))
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        if self.keep_aside(path) {
            self.vault.remove(path);
        }
        Ok(())
    }

    fn stat(&self, path: &str) -> io::Result<Option<FileInfo>> {
        Ok(self.vault.read(path).ok().map(|data| Self::info(path, &data)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_old_content_aside() {
        let vault = Arc::new(MemStorage::new());
        vault.write("a.typ", "1");
        vault.write(".baluk/settings.json", "{}");
        vault.write(".git/config", "x");
        let tree = VaultTree::new(vault.clone());
        let listed: Vec<_> = tree.list().unwrap().into_iter().map(|f| f.path).collect();
        assert_eq!(listed, [".baluk/settings.json", "a.typ"]);

        let before = tree.stat("a.typ").unwrap().unwrap();
        assert_eq!(before.mtime, tree.stat("a.typ").unwrap().unwrap().mtime, "stable");
        tree.write("a.typ", b"2").unwrap();
        assert_ne!(tree.stat("a.typ").unwrap().unwrap().mtime, before.mtime, "a same-size edit is seen");
        tree.write("b/c.typ", b"new").unwrap();
        tree.write("a.typ", b"3").unwrap();
        tree.remove("b/c.typ").unwrap();
        tree.remove("none.typ").unwrap();
        assert_eq!(vault.read("a.typ").unwrap(), b"3");
        assert!(tree.stat("b/c.typ").unwrap().is_none());
        assert_eq!(tree.take_removed(), [("a.typ".to_owned(), b"2".to_vec()), ("b/c.typ".to_owned(), b"new".to_vec())]);
        assert!(tree.take_removed().is_empty(), "taken once");
        assert!(tree.write("../x", b"").is_err());
    }
}
