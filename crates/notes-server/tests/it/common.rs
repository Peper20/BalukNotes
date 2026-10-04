//! Shared by the server end-to-end tests: one core for the `tests/vault` fixture.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use notes_core::{LibrarySource, Notes, NotesConfig};

pub static NOTES: LazyLock<Arc<Notes>> = LazyLock::new(|| {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::new(
        Notes::open(&NotesConfig {
            trash: None,
            vault: repo.join("tests/vault"),
            library: LibrarySource::Dir(repo.join("baluk")),
            font_dirs: vec![],
            cache: None,
        })
        .unwrap(),
    )
});
