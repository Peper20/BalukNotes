//! Хранилище: папка с `.typ`, она же корень Typst-проекта.
//!
//! - **Заметка** — файл `путь/Имя.typ`, идентификатор `путь/Имя`.
//! - **Книга** — папка с `main.typ`, идентификатор — путь к папке. Файлы
//!   внутри книги (главы, код) заметками не считаются.
//! - Служебное пропускается: имена на `_` (библиотека `_konspekt`) и на `.`.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{Error, Result};

/// Главный файл книги.
pub const BOOK_MAIN: &str = "main.typ";

/// Путь заметки от корня хранилища без `.typ`, через `/`: `Сеть/SSH`.
///
/// Проверяется при создании, поэтому из него всегда можно безопасно
/// получить путь внутри хранилища (без `..`, абсолютных путей и служебных
/// каталогов).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct NoteId(String);

impl NoteId {
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        let invalid = |reason| Err(Error::InvalidId { id: id.clone(), reason });
        if id.is_empty() {
            return invalid("пустой путь");
        }
        if id.starts_with('/') || id.ends_with('/') {
            return invalid("путь без / в начале и в конце");
        }
        if id.contains('\\') {
            return invalid("разделитель — /");
        }
        for segment in id.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." {
                return invalid("пустые сегменты, . и .. не допускаются");
            }
            if segment.starts_with('_') || segment.starts_with('.') {
                return invalid("имена на _ и . — служебные");
            }
        }
        if Path::new(&id).extension().is_some_and(|e| e == "typ") {
            return invalid("путь без .typ");
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Последний сегмент — имя для показа: `Сеть/SSH` → `SSH`.
    pub fn name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// Папка заметки (`Сеть` для `Сеть/SSH`), для корня — пусто.
    pub fn parent(&self) -> &str {
        self.0.rsplit_once('/').map_or("", |(p, _)| p)
    }
}

impl fmt::Display for NoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    Note,
    Book,
}

/// Заметка или книга в хранилище.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Entry {
    pub id: NoteId,
    pub kind: NoteKind,
    /// Главный файл относительно корня хранилища.
    #[serde(skip)]
    pub main: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Открывает существующее хранилище.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let root = fs::canonicalize(root).map_err(|e| Error::io(root, e))?;
        if !root.is_dir() {
            return Err(Error::io(
                &root,
                std::io::Error::new(std::io::ErrorKind::NotADirectory, "хранилище — не каталог"),
            ));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Все заметки и книги, по алфавиту путей.
    pub fn entries(&self) -> Result<Vec<Entry>> {
        let mut out = Vec::new();
        self.scan(&self.root, &mut out)?;
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Заметка или книга по идентификатору.
    pub fn entry(&self, id: &NoteId) -> Result<Entry> {
        let note = PathBuf::from(format!("{id}.typ"));
        if self.root.join(&note).is_file() {
            return Ok(Entry { id: id.clone(), kind: NoteKind::Note, main: note });
        }
        let book = Path::new(id.as_str()).join(BOOK_MAIN);
        if self.root.join(&book).is_file() {
            return Ok(Entry { id: id.clone(), kind: NoteKind::Book, main: book });
        }
        Err(Error::NotFound(id.to_string()))
    }

    /// Исходники заметки относительно корня: у заметки — её файл, у книги —
    /// все `.typ` в папке (кроме служебных `_*` и `.*`), по алфавиту.
    pub fn files_of(&self, entry: &Entry) -> Result<Vec<PathBuf>> {
        match entry.kind {
            NoteKind::Note => Ok(vec![entry.main.clone()]),
            NoteKind::Book => {
                let mut out = Vec::new();
                let dir = entry.main.parent().expect("main.typ книги лежит в её папке");
                self.typ_files(dir, &mut out)?;
                out.sort();
                Ok(out)
            }
        }
    }

    fn typ_files(&self, rel: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        let dir = self.root.join(rel);
        for item in fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))? {
            let item = item.map_err(|e| Error::io(&dir, e))?;
            let name = item.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('_') || name.starts_with('.') {
                continue;
            }
            let child = rel.join(name);
            if item.file_type().map_err(|e| Error::io(item.path(), e))?.is_dir() {
                self.typ_files(&child, out)?;
            } else if child.extension().is_some_and(|e| e == "typ") {
                out.push(child);
            }
        }
        Ok(())
    }

    fn scan(&self, dir: &Path, out: &mut Vec<Entry>) -> Result<()> {
        let read = fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
        for item in read {
            let item = item.map_err(|e| Error::io(dir, e))?;
            let path = item.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue; // не UTF-8 — не заметка
            };
            if name.starts_with('_') || name.starts_with('.') {
                continue;
            }
            let rel = path.strip_prefix(&self.root).expect("scan идёт внутри корня");
            let file_type = item.file_type().map_err(|e| Error::io(&path, e))?;
            if file_type.is_dir() {
                if path.join(BOOK_MAIN).is_file() {
                    if let Some(id) = rel_to_id(rel) {
                        out.push(Entry { id, kind: NoteKind::Book, main: rel.join(BOOK_MAIN) });
                    }
                } else {
                    self.scan(&path, out)?;
                }
            } else if path.extension().is_some_and(|e| e == "typ")
                && let Some(id) = rel_to_id(&rel.with_extension(""))
            {
                out.push(Entry { id, kind: NoteKind::Note, main: rel.to_path_buf() });
            }
        }
        Ok(())
    }
}

/// Относительный путь → идентификатор (`/`-разделители на любой ОС).
fn rel_to_id(rel: &Path) -> Option<NoteId> {
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    NoteId::new(parts?.join("/")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_id_validation() {
        assert!(NoteId::new("Сеть/SSH").is_ok());
        assert!(NoteId::new("SSH").is_ok());
        for bad in ["", "/SSH", "Сеть/", "a//b", "../x", "a/./b", "_konspekt/lib", ".git/x", "a\\b", "SSH.typ"] {
            assert!(NoteId::new(bad).is_err(), "{bad} должен быть отвергнут");
        }
    }

    #[test]
    fn note_id_parts() {
        let id = NoteId::new("Сеть/SSL/acme").unwrap();
        assert_eq!(id.name(), "acme");
        assert_eq!(id.parent(), "Сеть/SSL");
        assert_eq!(NoteId::new("SSH").unwrap().parent(), "");
    }

    #[test]
    fn scan_notes_and_books() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for f in
            ["Сеть/SSH.typ", "Сеть/UFW.typ", "Матан/main.typ", "Матан/01-глава.typ", "_konspekt/lib.typ", "код.cpp"]
        {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "").unwrap();
        }
        let vault = Vault::open(root).unwrap();
        let got: Vec<_> = vault.entries().unwrap().into_iter().map(|e| (e.id.to_string(), e.kind)).collect();
        assert_eq!(
            got,
            [
                ("Матан".into(), NoteKind::Book),
                ("Сеть/SSH".into(), NoteKind::Note),
                ("Сеть/UFW".into(), NoteKind::Note),
            ]
        );
        let book = vault.entry(&NoteId::new("Матан").unwrap()).unwrap();
        assert_eq!(book.main, Path::new("Матан/main.typ"));
        assert_eq!(vault.files_of(&book).unwrap(), [Path::new("Матан/01-глава.typ"), Path::new("Матан/main.typ")]);
        assert!(matches!(vault.entry(&NoteId::new("Нет").unwrap()), Err(Error::NotFound(_))));
    }
}
