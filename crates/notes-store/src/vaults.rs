//! The user's vaults: several independent note folders, each with its own name
//! (like Obsidian vaults).
//!
//! All vaults live in the data directory: `<data>/vaults/<name>/`. The vault
//! name is the name of its folder ([`VaultName`]: no `/`, no characters
//! forbidden in Windows file names, not internal). Each has its own rendering
//! cache (the key is the vault path).
//!
//! There is no default vault (the user's decision): the user creates and names
//! even the first one, and commands work in an explicitly named vault.
//!
//! Vault settings live in the vault itself, [`SETTINGS_FILE`] (like
//! `.obsidian/`), and move with the folder (`notes_core::settings::VaultSettings`).
//! Internal names starting with `.` are never notes ([`crate::names::is_hidden`]).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::names::{VaultName, quoted_list};

/// Why an operation on the vaults failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no vault \"{name}\"; there are: {}", quoted_list(known))]
    NotFound { name: String, known: Vec<VaultName> },

    #[error("vault \"{0}\" already exists")]
    Exists(String),

    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Error {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

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
            _ => Err(Error::NotFound { name: name.to_owned(), known: self.list()? }),
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

    /// Renames a vault (its folder); an error if another vault has the new name.
    /// Close an open vault first (`Notes::close` of the core).
    pub fn rename(&self, from: &VaultName, to: &VaultName) -> Result<()> {
        let (old, new) = (self.path(from), self.path(to));
        if !old.is_dir() {
            return Err(Error::NotFound { name: from.to_string(), known: self.list()? });
        }
        // On a case-insensitive file system a name that differs only in case is
        // the same folder: the rename goes through.
        if is_other_entry(&old, &new).map_err(|e| Error::io(&new, e))? {
            return Err(Error::Exists(to.to_string()));
        }
        fs::rename(&old, &new).map_err(|e| Error::io(&old, e))
    }

    /// Moves the whole vault to the system trash (`trash` = `None`) or to the
    /// directory `trash` (tests). Close an open vault first.
    pub fn trash(&self, name: &VaultName, trash: Option<&Path>) -> Result<()> {
        let path = self.path(name);
        if !path.is_dir() {
            return Err(Error::NotFound { name: name.to_string(), known: self.list()? });
        }
        crate::storage::move_to_trash(&path, trash).map_err(|e| Error::io(&path, e))
    }

    /// Creates a **new** empty vault; an error if it already exists.
    pub fn create(&self, name: &VaultName) -> Result<PathBuf> {
        fs::create_dir_all(&self.root).map_err(|e| Error::io(&self.root, e))?;
        let path = self.path(name);
        match fs::create_dir(&path) {
            Ok(()) => Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(Error::Exists(name.to_string())),
            Err(e) => Err(Error::io(&path, e)),
        }
    }
}

/// Whether `new` is an existing entry other than `old`. On a case-insensitive
/// file system a name that differs only in case is `old` itself.
fn is_other_entry(old: &Path, new: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(new) {
        Ok(_) => Ok(!same_entry(old, new)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// Whether two existing paths are the same entry, by device and inode of the
/// entries themselves: a link to a folder is an entry of its own.
#[cfg(unix)]
fn same_entry(a: &Path, b: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let (a, b) = (fs::symlink_metadata(a)?, fs::symlink_metadata(b)?);
    Ok((a.dev(), a.ino()) == (b.dev(), b.ino()))
}

/// Elsewhere the canonical paths, which spell the name in its stored case.
#[cfg(not(unix))]
fn same_entry(a: &Path, b: &Path) -> io::Result<bool> {
    Ok(fs::canonicalize(a)? == fs::canonicalize(b)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> VaultName {
        VaultName::new(s).unwrap()
    }

    #[test]
    fn create_list_find() {
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        assert!(vaults.list().unwrap().is_empty(), "no vaults directory yet");
        vaults.create(&name("Учёба")).unwrap();
        vaults.create(&name("Работа")).unwrap();
        assert!(matches!(vaults.create(&name("Учёба")), Err(Error::Exists(_))));
        // Files and internal folders are not vaults.
        fs::write(vaults.root().join("файл"), "").unwrap();
        fs::create_dir(vaults.root().join(".trash")).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("Работа"), name("Учёба")]);
        assert_eq!(vaults.find("Учёба").unwrap(), name("Учёба"));
        let Err(Error::NotFound { known, .. }) = vaults.find("Нет") else { panic!("expected VaultNotFound") };
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
        assert!(matches!(vaults.rename(&name("Учёба"), &name("Работа")), Err(Error::Exists(_))));
        assert!(matches!(vaults.rename(&name("Нет"), &name("Другое")), Err(Error::NotFound { .. })));
        vaults.rename(&name("Учёба"), &name("Учёба 2026")).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("Работа"), name("Учёба 2026")]);
        assert!(vaults.path(&name("Учёба 2026")).join("a.typ").is_file(), "notes moved with the folder");
        vaults.trash(&name("Работа"), Some(bin.path())).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("Учёба 2026")]);
        assert!(bin.path().join("Работа").is_dir(), "the vault is in the trash and can be restored");
        assert!(matches!(vaults.trash(&name("Работа"), Some(bin.path())), Err(Error::NotFound { .. })));
    }

    #[test]
    fn rename_onto_other_vault_by_case() {
        // Two folders whose names differ only in case need a case-sensitive file system.
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        vaults.create(&name("Work")).unwrap();
        vaults.create(&name("work")).unwrap();
        fs::write(vaults.path(&name("Work")).join("a.typ"), "= A").unwrap();
        assert!(matches!(vaults.rename(&name("Work"), &name("work")), Err(Error::Exists(_))));
        assert!(vaults.path(&name("Work")).join("a.typ").is_file(), "the renamed vault is untouched");
        assert!(vaults.path(&name("work")).is_dir(), "the other vault is not replaced");
        assert_eq!(vaults.list().unwrap(), [name("Work"), name("work")]);
    }

    #[test]
    fn rename_by_case_only() {
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        vaults.create(&name("Work")).unwrap();
        fs::write(vaults.path(&name("Work")).join("a.typ"), "= A").unwrap();
        vaults.rename(&name("Work"), &name("WORK")).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("WORK")]);
        assert!(vaults.path(&name("WORK")).join("a.typ").is_file(), "notes moved with the folder");
    }

    #[test]
    fn rename_to_own_name() {
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        vaults.create(&name("Work")).unwrap();
        fs::write(vaults.path(&name("Work")).join("a.typ"), "= A").unwrap();
        vaults.rename(&name("Work"), &name("Work")).unwrap();
        assert!(vaults.path(&name("Work")).join("a.typ").is_file(), "the same folder is kept");
    }

    #[cfg(unix)]
    #[test]
    fn rename_onto_link_to_vault() {
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        vaults.create(&name("Work")).unwrap();
        std::os::unix::fs::symlink(vaults.path(&name("Work")), vaults.root().join("link")).unwrap();
        assert!(matches!(vaults.rename(&name("Work"), &name("link")), Err(Error::Exists(_))));
        assert!(vaults.path(&name("Work")).is_dir(), "a link is another entry, the folder is untouched");
    }
}
