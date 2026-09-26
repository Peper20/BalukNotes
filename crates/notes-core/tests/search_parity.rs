//! Поиск статического сайта (`app/src/lib/site-search.ts`) повторяет поиск
//! ядра по тому же индексу ([`notes_core::search::documents`]). Этот тест
//! записывает эталон — индекс `tests/vault` и ответы ядра на запросы — в
//! `tests/snapshots/search.json`; Vitest сверяет с ним клиента.
//!
//!   cargo test -p notes-core --test search_parity                       # сравнить
//!   UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test search_parity    # обновить

use std::path::PathBuf;

use notes_core::{LibrarySource, Notes, NotesConfig};
use serde_json::json;

/// Запросы: слово, два слова, регистр и «ё», название, заголовок, ничего.
const QUERIES: &[&str] = &["ssh", "порт", "Смена ПОРТА", "ключ сервер", "ёлка", "книга", "формулы", "e", "нет-такого-слова"];
const LIMIT: usize = 50;

#[test]
fn site_search_reference() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let notes = Notes::open(&NotesConfig {
        vault: repo.join("tests/vault"),
        library: LibrarySource::Dir(repo.join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .unwrap();
    let docs = notes.search_documents(|_| true).unwrap();
    let cases: Vec<_> = QUERIES
        .iter()
        .map(|q| json!({ "query": q, "limit": LIMIT, "hits": notes.search(q, LIMIT).unwrap() }))
        .collect();
    let actual = serde_json::to_string_pretty(&json!({ "docs": docs, "cases": cases })).unwrap() + "\n";

    let path = repo.join("tests/snapshots/search.json");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    if old == actual {
        return;
    }
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, actual).unwrap();
    } else {
        panic!(
            "индекс или ответы поиска изменились — проверьте и обновите: \
             UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test search_parity"
        );
    }
}
