//! Хранилища пользователя: несколько независимых папок заметок, у каждой —
//! своё имя (как хранилища Obsidian).
//!
//! Все хранилища лежат в каталоге данных: `<данные>/vaults/<имя>/`. Имя
//! хранилища — имя его папки ([`VaultName`]: без `/` и знаков, которые
//! запрещены в именах файлов Windows, не служебное). Кэш отрисовки у
//! каждого свой (ключ — путь хранилища, см. [`crate::cache`]).
//!
//! Хранилища по умолчанию нет (решение пользователя): даже первое создаёт
//! и называет пользователь, а команды работают в хранилище, названном явно.
//!
//! Настройки хранилища — в нём самом, [`SETTINGS_FILE`] (как `.obsidian/`):
//! переезжают вместе с папкой ([`crate::settings::VaultSettings`]).
//! Служебные имена на `.` заметками не бывают ([`crate::storage::is_hidden`]).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::is_hidden;
use crate::{Error, Result};

/// Каталог хранилищ в каталоге данных.
pub const VAULTS_DIR: &str = "vaults";
/// Настройки хранилища — путь в его папке.
pub const SETTINGS_FILE: &str = ".baluk/settings.json";
/// Самое длинное имя хранилища, знаков.
pub const MAX_NAME: usize = 64;

/// Имя хранилища — оно же имя его папки. Проверяется при создании: из него
/// всегда можно безопасно получить путь в каталоге хранилищ.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(try_from = "String", into = "String")]
pub struct VaultName(String);

impl VaultName {
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let invalid = |reason| Err(Error::InvalidVault { name: name.clone(), reason });
        if name.trim().is_empty() {
            return invalid("пустое имя");
        }
        if name.trim() != name {
            return invalid("пробелы в начале или в конце");
        }
        if name.chars().count() > MAX_NAME {
            return invalid("длиннее 64 знаков");
        }
        if is_hidden(&name) {
            return invalid("имена на _ и . — служебные");
        }
        // Имя папки на любой ОС (Windows запрещает эти знаки и точку в конце).
        if name.chars().any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c)) {
            return invalid(r#"без знаков / \ : * ? " < > |"#);
        }
        if name.ends_with('.') {
            return invalid("точка в конце");
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VaultName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for VaultName {
    type Error = Error;

    fn try_from(name: String) -> Result<Self> {
        Self::new(name)
    }
}

impl From<VaultName> for String {
    fn from(name: VaultName) -> Self {
        name.0
    }
}

/// Хранилища в каталоге данных: список, создание, переименование, удаление
/// в корзину, путь по имени.
#[derive(Debug, Clone)]
pub struct Vaults {
    /// `<данные>/vaults`.
    root: PathBuf,
}

impl Vaults {
    /// Хранилища каталога данных `data` (каталог хранилищ может ещё не существовать).
    pub fn new(data: &Path) -> Self {
        Self { root: data.join(VAULTS_DIR) }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Папка хранилища.
    pub fn path(&self, name: &VaultName) -> PathBuf {
        self.root.join(name.as_str())
    }

    /// Есть ли хранилище.
    pub fn exists(&self, name: &VaultName) -> bool {
        self.path(name).is_dir()
    }

    /// Хранилище по имени: есть — его имя, нет или имя неверное — ошибка
    /// со списком имеющихся.
    pub fn find(&self, name: &str) -> Result<VaultName> {
        match VaultName::new(name) {
            Ok(name) if self.exists(&name) => Ok(name),
            _ => Err(Error::VaultNotFound { name: name.to_owned(), known: self.list()? }),
        }
    }

    /// Все хранилища по алфавиту. Папки со служебными и неверными именами
    /// пропускаются; нет каталога хранилищ — пусто.
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

    /// Переименовать хранилище (папку); хранилище с новым именем уже есть —
    /// ошибка. Открытое хранилище сначала закрыть ([`crate::Notes::close`]).
    pub fn rename(&self, from: &VaultName, to: &VaultName) -> Result<()> {
        let (old, new) = (self.path(from), self.path(to));
        if !old.is_dir() {
            return Err(Error::VaultNotFound { name: from.to_string(), known: self.list()? });
        }
        // Имя другого регистра на нечувствительной к регистру ФС — та же
        // папка: переименовать можно.
        if new.exists() && from.as_str().to_lowercase() != to.as_str().to_lowercase() {
            return Err(Error::VaultExists(to.to_string()));
        }
        fs::rename(&old, &new).map_err(|e| Error::io(&old, e))
    }

    /// Хранилище целиком — в корзину системы (`trash` = `None`) или в
    /// каталог `trash` (тесты). Открытое хранилище сначала закрыть.
    pub fn trash(&self, name: &VaultName, trash: Option<&Path>) -> Result<()> {
        let path = self.path(name);
        if !path.is_dir() {
            return Err(Error::VaultNotFound { name: name.to_string(), known: self.list()? });
        }
        crate::storage::move_to_trash(&path, trash).map_err(|e| Error::io(&path, e))
    }

    /// Создать **новое** пустое хранилище; такое уже есть — ошибка.
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
    fn names() {
        for ok in ["Заметки", "Учёба 2026", "C++", "a.b", "x-y_z"] {
            assert_eq!(name(ok).as_str(), ok);
        }
        for bad in ["", "  ", " a", "a ", ".git", "_baluk", "a/b", r"a\b", "a:b", "a?", "a.", "a\nb"] {
            assert!(VaultName::new(bad).is_err(), "{bad:?}");
        }
        assert!(VaultName::new("я".repeat(MAX_NAME)).is_ok());
        assert!(VaultName::new("я".repeat(MAX_NAME + 1)).is_err());
        // Из JSON — с той же проверкой.
        assert!(serde_json::from_str::<VaultName>(r#""a/b""#).is_err());
        assert_eq!(serde_json::from_str::<VaultName>(r#""Учёба""#).unwrap(), name("Учёба"));
    }

    #[test]
    fn create_list_find() {
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        assert!(vaults.list().unwrap().is_empty(), "каталога хранилищ ещё нет");
        vaults.create(&name("Учёба")).unwrap();
        vaults.create(&name("Работа")).unwrap();
        assert!(matches!(vaults.create(&name("Учёба")), Err(Error::VaultExists(_))));
        // Файлы и служебные папки — не хранилища.
        fs::write(vaults.root().join("файл"), "").unwrap();
        fs::create_dir(vaults.root().join(".trash")).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("Работа"), name("Учёба")]);
        assert_eq!(vaults.find("Учёба").unwrap(), name("Учёба"));
        let Err(Error::VaultNotFound { known, .. }) = vaults.find("Нет") else { panic!("нет такого") };
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
        assert!(vaults.path(&name("Учёба 2026")).join("a.typ").is_file(), "заметки переехали с папкой");
        vaults.trash(&name("Работа"), Some(bin.path())).unwrap();
        assert_eq!(vaults.list().unwrap(), [name("Учёба 2026")]);
        assert!(bin.path().join("Работа").is_dir(), "хранилище — в корзине, его можно вернуть");
        assert!(matches!(vaults.trash(&name("Работа"), Some(bin.path())), Err(Error::VaultNotFound { .. })));
    }
}
