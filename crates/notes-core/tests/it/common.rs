//! Общее сквозных тестов ядра: корень репозитория, одно ядро на хранилище-
//! фикстуру `tests/vault` и сборка заметки из одного файла.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use notes_core::figures::FigureOptions;
use notes_core::{LibrarySource, NoteId, NotePage, Notes, NotesConfig};

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Одно ядро на все тесты бинарника: шрифты, темы и сборки заметок фикстуры
/// (снимки, проверки хранилища) — один раз.
pub static NOTES: LazyLock<Notes> = LazyLock::new(|| {
    Notes::open(&NotesConfig {
        trash: None,
        vault: repo().join("tests/vault"),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .expect("тестовое хранилище открывается")
});

/// Собирает заметку из одного файла с библиотекой репозитория.
pub fn compile(source: &str) -> Arc<NotePage> {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("t.typ"), source).unwrap();
    let notes = Notes::open(&NotesConfig {
        trash: None,
        vault: vault.path().to_path_buf(),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .unwrap();
    notes.page(&NoteId::new("t").unwrap(), FigureOptions::default()).unwrap()
}
