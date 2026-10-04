//! Хранилище: папка с `.typ`, она же корень Typst-проекта.
//!
//! - **Заметка** — файл `путь/Имя.typ`, идентификатор `путь/Имя`.
//! - **Книга** — папка с `main.typ`, идентификатор — путь к папке. Файлы
//!   внутри книги (главы, код) заметками не считаются.
//! - Служебное пропускается: имена на `_` (библиотека `_baluk`) и на `.`.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::storage::{DirStorage, Storage, is_typ};
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

/// Хранилище: заметки и книги поверх [`Storage`].
#[derive(Debug, Clone)]
pub struct Vault {
    storage: Arc<dyn Storage>,
}

impl Vault {
    /// Открывает существующий каталог-хранилище.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let storage = DirStorage::open(root).map_err(|e| Error::io(root, e))?;
        Ok(Self::new(Arc::new(storage)))
    }

    /// Хранилище поверх любого [`Storage`] (в тестах — [`crate::storage::MemStorage`]).
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    pub fn storage(&self) -> &Arc<dyn Storage> {
        &self.storage
    }

    /// Где хранилище (для журнала).
    pub fn location(&self) -> String {
        self.storage.location()
    }

    /// Ошибка ввода-вывода с путём файла.
    pub(crate) fn io_error(&self, path: &str, e: std::io::Error) -> Error {
        Error::io(self.storage.display(path), e)
    }

    /// Прочитать файл хранилища как текст.
    pub(crate) fn read_text(&self, path: &str) -> Result<String> {
        let data = self.storage.read(path).map_err(|e| self.io_error(path, e))?;
        String::from_utf8(data)
            .map_err(|e| self.io_error(path, std::io::Error::new(std::io::ErrorKind::InvalidData, e.utf8_error())))
    }

    fn is_file(&self, path: &str) -> bool {
        self.storage.stat(path).is_ok_and(|m| !m.is_dir)
    }

    /// Все заметки и книги, по алфавиту путей.
    ///
    /// Книга — каталог с `main.typ` (ближайший к корню: книга в книге не
    /// ищется); `.typ` вне книг — заметки.
    pub fn entries(&self) -> Result<Vec<Entry>> {
        let files = self.storage.list().map_err(|e| self.io_error("", e))?;
        let books: BTreeSet<&str> = files.iter().filter_map(|f| f.strip_suffix(&format!("/{BOOK_MAIN}"))).collect();
        let mut out = Vec::new();
        let mut seen_books = BTreeSet::new();
        for file in &files {
            if !is_typ(file) {
                continue;
            }
            match book_of(&books, file) {
                Some(book) => {
                    if seen_books.insert(book)
                        && let Some(id) = rel_to_id(Path::new(book))
                    {
                        out.push(Entry { id, kind: NoteKind::Book, main: Path::new(book).join(BOOK_MAIN) });
                    }
                }
                None => {
                    if let Some(id) = rel_to_id(&Path::new(file).with_extension("")) {
                        out.push(Entry { id, kind: NoteKind::Note, main: PathBuf::from(file) });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Папки хранилища по алфавиту путей: с заметками и пустые (в них нет
    /// файлов, кроме служебных). Каталог только с файлами не заметок (`code/`,
    /// `img/` рядом с заметкой) — не папка; книга и всё, что в ней, — тоже;
    /// служебные и с недопустимыми для [`NoteId`] именами пропускаются.
    pub fn folders(&self) -> Result<Vec<String>> {
        let files = self.storage.list().map_err(|e| self.io_error("", e))?;
        let books: BTreeSet<&str> = files.iter().filter_map(|f| f.strip_suffix(&format!("/{BOOK_MAIN}"))).collect();
        let mut out = BTreeSet::new();
        for dir in self.storage.dirs().map_err(|e| self.io_error("", e))? {
            if books.contains(dir.as_str()) || book_of(&books, &dir).is_some() || NoteId::new(dir.as_str()).is_err() {
                continue;
            }
            let prefix = format!("{dir}/");
            let mut inside = files.iter().filter(|f| f.starts_with(&prefix)).peekable();
            let empty = inside.peek().is_none();
            if empty || inside.any(|f| is_typ(f)) {
                out.extend(crate::folders::ancestors(&prefix).map(str::to_owned));
            }
        }
        Ok(out.into_iter().collect())
    }

    /// Заметка или книга по идентификатору.
    pub fn entry(&self, id: &NoteId) -> Result<Entry> {
        let note = format!("{id}.typ");
        if self.is_file(&note) {
            return Ok(Entry { id: id.clone(), kind: NoteKind::Note, main: PathBuf::from(note) });
        }
        let book = format!("{id}/{BOOK_MAIN}");
        if self.is_file(&book) {
            return Ok(Entry { id: id.clone(), kind: NoteKind::Book, main: PathBuf::from(book) });
        }
        Err(Error::NotFound(id.to_string()))
    }

    /// Исходники заметки относительно корня (через `/`): у заметки — её
    /// файл, у книги — все `.typ` в папке (кроме служебных `_*` и `.*`), по
    /// алфавиту путей.
    pub fn files_of(&self, entry: &Entry) -> Result<Vec<String>> {
        let main = entry.main.to_string_lossy().replace('\\', "/");
        match entry.kind {
            NoteKind::Note => Ok(vec![main]),
            NoteKind::Book => {
                // A book's main file is `<id>/main.typ` (see `entry`): its sources are under `<id>/`.
                let dir = format!("{}/", entry.id);
                let dir = dir.as_str();
                let files = self.storage.list().map_err(|e| self.io_error(dir, e))?;
                let mut out: Vec<String> = files.into_iter().filter(|f| f.starts_with(dir) && is_typ(f)).collect();
                out.sort_by(|a, b| Path::new(a).cmp(Path::new(b)));
                Ok(out)
            }
        }
    }

    /// Размер исходников: у заметки — главный файл, у книги — все её `.typ`.
    pub fn source_size(&self, entry: &Entry) -> u64 {
        self.files_of(entry).unwrap_or_default().iter().map(|f| self.storage.stat(f).map_or(0, |m| m.len)).sum()
    }
}

/// Книга, в которую входит файл: ближайший к корню каталог с `main.typ`.
fn book_of<'a>(books: &BTreeSet<&'a str>, file: &str) -> Option<&'a str> {
    let mut end = 0;
    while let Some(i) = file[end..].find('/') {
        end += i;
        if let Some(book) = books.get(&file[..end]) {
            return Some(book);
        }
        end += 1;
    }
    None
}

/// Относительный путь → идентификатор (`/`-разделители на любой ОС).
fn rel_to_id(rel: &Path) -> Option<NoteId> {
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    NoteId::new(parts?.join("/")).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn note_id_validation() {
        assert!(NoteId::new("Сеть/SSH").is_ok());
        assert!(NoteId::new("SSH").is_ok());
        for bad in ["", "/SSH", "Сеть/", "a//b", "../x", "a/./b", "_baluk/lib", ".git/x", "a\\b", "SSH.typ"] {
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
        for f in ["Сеть/SSH.typ", "Сеть/UFW.typ", "Матан/main.typ", "Матан/01-глава.typ", "_baluk/lib.typ", "код.cpp"]
        {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "").unwrap();
        }
        for d in ["Пустая/Вложенная", "Матан/рисунки", "_служебная", "Сеть/code", "Ресурсы/img"]
        {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join("Сеть/code/main.cpp"), "").unwrap();
        fs::write(root.join("Ресурсы/img/a.png"), "").unwrap();
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
        assert_eq!(vault.files_of(&book).unwrap(), ["Матан/01-глава.typ", "Матан/main.typ"]);
        assert!(matches!(vault.entry(&NoteId::new("Нет").unwrap()), Err(Error::NotFound(_))));
        assert_eq!(
            vault.folders().unwrap(),
            ["Пустая", "Пустая/Вложенная", "Сеть"],
            "пустые — тоже; книга и код рядом с заметкой — не папки"
        );
    }
}
