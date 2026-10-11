//! The user's vaults: several independent note folders, each with its own name
//! (like Obsidian vaults).
//!
//! All vaults live in the data directory: `<data>/vaults/<name>/`. The vault
//! name is the name of its folder ([`VaultName`]: no `/`, no characters
//! forbidden in Windows file names, not internal). Each has its own rendering
//! cache (the key is the vault path, see [`crate::cache`]).
//!
//! There is no default vault (the user's decision): the user creates and names
//! even the first one, and commands work in an explicitly named vault.
//!
//! Vault settings live in the vault itself, [`SETTINGS_FILE`] (like
//! `.obsidian/`), and move with the folder ([`crate::settings::VaultSettings`]).
//! Internal names starting with `.` are never notes ([`crate::storage::is_hidden`]).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use notes_store::names::{MAX_NAME, VaultName};

use crate::{Error, Result};

/// The vaults directory inside the data directory.
pub const VAULTS_DIR: &str = "vaults";
/// Vault settings: the path inside the vault folder.
pub const SETTINGS_FILE: &str = ".baluk/settings.json";

/// The vaults in the data directory: list, create, rename, move to the trash,
/// path by name.
#[derive(Debug, Clone)]
pub struct Vaults {
    /// `<data>/vaults`.
    root: PathBuf,
}

impl Vaults {
    /// The vaults of the data directory `data` (the vaults directory may not exist yet).
    pub fn new(data: &Path) -> Self {
        Self { root: data.join(VAULTS_DIR) }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The vault folder.
    pub fn path(&self, name: &VaultName) -> PathBuf {
        self.root.join(name.as_str())
    }

    /// Whether the vault exists.
    pub fn exists(&self, name: &VaultName) -> bool {
        self.path(name).is_dir()
    }

    /// A vault by name: its name if it exists; if not, or the name is invalid,
    /// an error listing the existing ones.
    pub fn find(&self, name: &str) -> Result<VaultName> {
        match VaultName::new(name) {
            Ok(name) if self.exists(&name) => Ok(name),
            _ => Err(Error::VaultNotFound { name: name.to_owned(), known: self.list()? }),
        }
    }

    /// All vaults in alphabetical order. Folders with internal or invalid names
    /// are skipped; no vaults directory means none.
    pub fn list(&self) -> Result<Vec<VaultName>> {
        let items = match fs::read_dir(&self.root) {
            Ok(items) => items,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::io(&self.root, e)),
        };
        let mut out = Vec::new();
        for item in items {
            let item = item.map_err(|e| Error::io(&self.root, e))?;
            let is_dir = item.file_type().map_err(|e| Error::io(item.path(), e))?.is_dir();
            if let (true, Some(name)) = (is_dir, item.file_name().to_str()) {
                out.extend(VaultName::new(name).ok());
            }
        }
        out.sort();
        Ok(out)
    }

    /// Renames a vault (its folder); an error if a vault with the new name
    /// exists. Close an open vault first ([`crate::Notes::close`]).
    pub fn rename(&self, from: &VaultName, to: &VaultName) -> Result<()> {
        let (old, new) = (self.path(from), self.path(to));
        if !old.is_dir() {
            return Err(Error::VaultNotFound { name: from.to_string(), known: self.list()? });
        }
        // A name in another case on a case-insensitive file system is the same
        // folder: renaming is allowed.
        if new.exists() && from.as_str().to_lowercase() != to.as_str().to_lowercase() {
            return Err(Error::VaultExists(to.to_string()));
        }
        fs::rename(&old, &new).map_err(|e| Error::io(&old, e))
    }

    /// Moves the whole vault to the system trash (`trash` = `None`) or to the
    /// directory `trash` (tests). Close an open vault first.
    pub fn trash(&self, name: &VaultName, trash: Option<&Path>) -> Result<()> {
        let path = self.path(name);
        if !path.is_dir() {
            return Err(Error::VaultNotFound { name: name.to_string(), known: self.list()? });
        }
        crate::storage::move_to_trash(&path, trash).map_err(|e| Error::io(&path, e))
    }

    /// Creates a **new** empty vault; an error if it already exists.
    pub fn create(&self, name: &VaultName) -> Result<PathBuf> {
        fs::create_dir_all(&self.root).map_err(|e| Error::io(&self.root, e))?;
        let path = self.path(name);
        match fs::create_dir(&path) {
            Ok(()) => Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(Error::VaultExists(name.to_string())),
            Err(e) => Err(Error::io(&path, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> VaultName {
        VaultName::new(s).unwrap()
    }

    #[test]
    fn invalid_name_error() {
        // The store's error becomes the core one with the same message.
        let err = Error::from(VaultName::new("a/b").unwrap_err());
        assert!(matches!(err, Error::InvalidVault { reason, .. } if reason.starts_with("no characters")));
        assert_eq!(err.to_string(), r#"invalid vault name "a/b": no characters / \ : * ? " < > |"#);
        let found: Result<VaultName> = VaultName::new(" x").map_err(Error::from);
        assert!(found.is_err());
    }

    #[test]
    fn create_list_find() {
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        assert!(vaults.list().unwrap().is_empty(), "no vaults directory yet");
        vaults.create(&name("Учёба")).unwrap();
        vaults.create(&name("Работа")).unwrap();
        assert!(matches!(vaults.create(&name("Учёба")), Err(Error::VaultExists(_))));
        // Files and internal folders are not vaults.
        fs::write(vaults.root().join("файл"), "").unwrap();
        fs::create_dir(vaults.root().join(".trash")).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("Работа"), name("Учёба")]);
        assert_eq!(vaults.find("Учёба").unwrap(), name("Учёба"));
        let Err(Error::VaultNotFound { known, .. }) = vaults.find("Нет") else { panic!("expected VaultNotFound") };
        assert_eq!(known.len(), 2);
        assert!(vaults.find("../x").is_err());
    }

    #[test]
    fn rename_and_trash() {
        let data = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        vaults.create(&name("Учёба")).unwrap();
        vaults.create(&name("Работа")).unwrap();
        fs::write(vaults.path(&name("Учёба")).join("a.typ"), "= A").unwrap();
        assert!(matches!(vaults.rename(&name("Учёба"), &name("Работа")), Err(Error::VaultExists(_))));
        assert!(matches!(vaults.rename(&name("Нет"), &name("Другое")), Err(Error::VaultNotFound { .. })));
        vaults.rename(&name("Учёба"), &name("Учёба 2026")).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("Работа"), name("Учёба 2026")]);
        assert!(vaults.path(&name("Учёба 2026")).join("a.typ").is_file(), "notes moved with the folder");
        vaults.trash(&name("Работа"), Some(bin.path())).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("Учёба 2026")]);
        assert!(bin.path().join("Работа").is_dir(), "the vault is in the trash and can be restored");
        assert!(matches!(vaults.trash(&name("Работа"), Some(bin.path())), Err(Error::VaultNotFound { .. })));
    }
}
