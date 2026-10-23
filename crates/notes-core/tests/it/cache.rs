//! The disk cache of built notes on the test vault: warming builds every note
//! once and a new start finds them on disk.
//!
//! The module is named to sort first: libtest starts the tests of a binary in
//! the order of their names, and this one is the longest of the binary (it
//! warms the whole vault one note after another, about 20 s), so it starts at
//! once instead of after the others (`tools/lib/cargo-test.sh`).

use notes_core::figures::FigureOptions;
use notes_core::{LibrarySource, Notes, NotesConfig};
use notes_store::note_id::NoteId;

use crate::common::repo;

const OPTS: FigureOptions = FigureOptions { precision: Some(2) };

fn id(s: &str) -> NoteId {
    NoteId::new(s).unwrap()
}

#[test]
fn warm_builds_everything_once_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let open = || {
        Notes::open(&NotesConfig {
            trash: None,
            vault: repo().join("tests/vault"),
            library: LibrarySource::Dir(repo().join("baluk")),
            font_dirs: vec![],
            cache: Some(dir.path().to_path_buf()),
        })
        .unwrap()
    };
    let first = open();
    let all = first.entries().unwrap().len();
    first.hint_warm(vec![id("Книга")]);
    let stats = first.warm_pass();
    assert_eq!((stats.built, stats.skipped), (all, 0), "the first round builds everything");
    assert_eq!(first.memory().0, 0, "warming goes to disk only");
    assert_eq!(first.warm_pass().built, 0, "the second builds nothing");

    // A new start: what was built (and notes with errors) lies on disk.
    let failed =
        first.entries().unwrap().iter().filter(|e| !first.page(&e.id, OPTS).unwrap().errors.is_empty()).count();
    assert!(failed > 0, "tests/vault has a note with an error");
    let stats = open().warm_pass();
    assert_eq!((stats.built, stats.skipped), (0, all));
}
