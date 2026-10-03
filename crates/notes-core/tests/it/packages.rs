//! Пакеты Typst: не из белого списка — ошибка сборки; список разрешённых
//! меняется — пересобираются только заметки, читавшие пакеты.

use notes_core::figures::FigureOptions;
use notes_core::settings::{Platform, Schema, SettingsStore};
use notes_core::{LibrarySource, NoteId, Notes, NotesConfig};

use crate::common::repo;

#[test]
fn package_outside_whitelist_is_an_error_and_policy_versions_only_its_users() {
    let dir = tempfile::tempdir().unwrap();
    let head = "#import \"/_baluk/lib.typ\": *\n#show: note.with(title: [x])\n";
    std::fs::write(dir.path().join("Чужой.typ"), format!("#import \"@preview/fletcher:0.5.8\": *\n{head}")).unwrap();
    // Без библиотеки: она сама берёт CeTZ, то есть тоже читает пакеты.
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
    assert!(errors.iter().any(|e| e.contains("пакет @preview/fletcher:0.5.8 не из белого списка")), "{errors:?}");
    let before = (notes.version(&foreign, opts).unwrap(), notes.version(&own, opts).unwrap());

    // Разрешили другой пакет (сеть не нужна): заметка с пакетами устарела, без них — нет.
    let settings = tempfile::tempdir().unwrap();
    let schema = Schema::new(notes.themes().themes(), Platform::Desktop);
    let store = SettingsStore::open(settings.path().join("settings.json"), schema).unwrap();
    let mut device = store.device();
    device.packages = vec!["@preview/tablem:0.2.0".into()];
    notes.apply_device(&device);
    assert_ne!(notes.version(&foreign, opts).unwrap(), before.0, "заметка с пакетом пересобирается");
    assert_eq!(notes.version(&own, opts).unwrap(), before.1, "без пакетов — та же версия");
    assert!(!notes.page(&foreign, opts).unwrap().errors.is_empty(), "fletcher всё ещё не разрешён");
}
