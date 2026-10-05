//! Typst packages: one outside the allowlist is a build error; when the list
//! of allowed ones changes, only notes that read packages are rebuilt.

use notes_core::figures::FigureOptions;
use notes_core::settings::{Platform, Schema, SettingsStore};
use notes_core::{LibrarySource, NoteId, Notes, NotesConfig};

use crate::common::repo;

#[test]
fn package_outside_whitelist_is_an_error_and_policy_versions_only_its_users() {
    let dir = tempfile::tempdir().unwrap();
    let head = "#import \"/_baluk/lib.typ\": *\n#show: note.with(title: [x])\n";
    std::fs::write(dir.path().join("Чужой.typ"), format!("#import \"@preview/fletcher:0.5.8\": *\n{head}")).unwrap();
    // Without the library: the library itself takes CeTZ, so it reads packages too.
    std::fs::write(dir.path().join("Свой.typ"), "= Просто текст\n").unwrap();
    let notes = Notes::open(&NotesConfig {
        trash: None,
        vault: dir.path().to_path_buf(),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .unwrap();
    let (foreign, own) = (NoteId::new("Чужой").unwrap(), NoteId::new("Свой").unwrap());
    let opts = FigureOptions::default();
    let page = notes.page(&foreign, opts).unwrap();
    let errors: Vec<String> = page.errors.iter().map(ToString::to_string).collect();
    assert!(errors.iter().any(|e| e.contains("package @preview/fletcher:0.5.8 is not whitelisted")), "{errors:?}");
    let before = (notes.version(&foreign, opts).unwrap(), notes.version(&own, opts).unwrap());

    // Another package allowed (no network needed): the note with packages is stale, the one without is not.
    let settings = tempfile::tempdir().unwrap();
    let schema = Schema::new(notes.themes().themes(), Platform::Desktop);
    let store = SettingsStore::open(settings.path().join("settings.json"), schema).unwrap();
    let mut device = store.device();
    device.packages = vec!["@preview/tablem:0.2.0".into()];
    notes.apply_device(&device);
    assert_ne!(notes.version(&foreign, opts).unwrap(), before.0, "the note with a package is rebuilt");
    assert_eq!(notes.version(&own, opts).unwrap(), before.1, "without packages, the same version");
    assert!(!notes.page(&foreign, opts).unwrap().errors.is_empty(), "fletcher is still not allowed");
}
