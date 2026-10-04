//! Shared by the core end-to-end tests: the repository root, one core for the
//! `tests/vault` fixture, and building a note from one file.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use notes_core::figures::FigureOptions;
use notes_core::{LibrarySource, NoteId, NotePage, Notes, NotesConfig};

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One core for all tests of the binary: fonts, themes and the fixture's note
/// builds (snapshots, vault checks) happen once.
pub static NOTES: LazyLock<Notes> = LazyLock::new(|| {
    Notes::open(&NotesConfig {
        trash: None,
        vault: repo().join("tests/vault"),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .expect("the test vault opens")
});

/// Builds a note from one file with the repository library.
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
