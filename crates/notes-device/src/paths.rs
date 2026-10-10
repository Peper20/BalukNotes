//! Where the sync data of a device lives (see the crate docs).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use notes_core::VaultName;

use crate::{Error, Result};

/// The sync files of one data directory.
#[derive(Debug, Clone)]
pub struct Paths {
    data: PathBuf,
}

impl Paths {
    #[must_use]
    pub fn new(data: impl Into<PathBuf>) -> Self {
        Self { data: data.into() }
    }

    #[must_use]
    pub fn data(&self) -> &Path {
        &self.data
    }

    /// `<data>/sync`.
    #[must_use]
    pub fn dir(&self) -> PathBuf {
        self.data.join("sync")
    }

    #[must_use]
    pub fn account_file(&self) -> PathBuf {
        self.dir().join("account.json")
    }

    fn vaults_dir(&self) -> PathBuf {
        self.dir().join("vaults")
    }

    /// The engine's state of a linked vault.
    #[must_use]
    pub fn state_file(&self, vault: &VaultName) -> PathBuf {
        self.vaults_dir().join(format!("{vault}.json"))
    }

    #[must_use]
    pub fn status_file(&self, vault: &VaultName) -> PathBuf {
        self.dir().join("status").join(format!("{vault}.json"))
    }

    #[must_use]
    pub fn lock_file(&self, vault: &VaultName) -> PathBuf {
        self.dir().join("locks").join(format!("{vault}.lock"))
    }

    /// Where sync puts local files it replaced or removed.
    #[must_use]
    pub fn removed_dir(&self, vault: &VaultName) -> PathBuf {
        self.dir().join("removed").join(vault.as_str())
    }

    /// The vault folder: `<data>/vaults/<vault>`.
    #[must_use]
    pub fn vault_dir(&self, vault: &VaultName) -> PathBuf {
        notes_core::Vaults::new(&self.data).path(vault)
    }

    /// The vaults of this data directory.
    pub fn local_vaults(&self) -> Result<Vec<VaultName>> {
        Ok(notes_core::Vaults::new(&self.data).list()?)
    }

    #[must_use]
    pub fn is_linked(&self, vault: &VaultName) -> bool {
        self.state_file(vault).is_file()
    }

    /// The linked vaults, in alphabetical order.
    pub fn linked(&self) -> Result<Vec<VaultName>> {
        let dir = self.vaults_dir();
        let items = match fs::read_dir(&dir) {
            Ok(items) => items,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::io(dir, e)),
        };
        let mut out = Vec::new();
        for item in items {
            let item = item.map_err(|e| Error::io(&dir, e))?;
            let name = item.file_name();
            let stem = name.to_str().and_then(|n| n.strip_suffix(".json"));
            out.extend(stem.and_then(|s| VaultName::new(s).ok()));
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_by_vault() {
        let paths = Paths::new("/d");
        let vault = VaultName::new("Учёба").unwrap();
        assert_eq!(paths.account_file(), Path::new("/d/sync/account.json"));
        assert_eq!(paths.state_file(&vault), Path::new("/d/sync/vaults/Учёба.json"));
        assert_eq!(paths.removed_dir(&vault), Path::new("/d/sync/removed/Учёба"));
        assert_eq!(paths.vault_dir(&vault), Path::new("/d/vaults/Учёба"));
    }

    #[test]
    fn linked_list() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        assert!(paths.linked().unwrap().is_empty());
        let (b, a) = (VaultName::new("b").unwrap(), VaultName::new("a").unwrap());
        for v in [&b, &a] {
            fs::create_dir_all(paths.state_file(v).parent().unwrap()).unwrap();
            fs::write(paths.state_file(v), "{}").unwrap();
        }
        fs::write(paths.dir().join("vaults/notes.txt"), "").unwrap();
        assert_eq!(paths.linked().unwrap(), [a.clone(), b]);
        assert!(paths.is_linked(&a));
        assert!(!paths.is_linked(&VaultName::new("c").unwrap()));
    }
}
