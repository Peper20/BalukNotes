//! End-to-end check on the test vault `tests/vault` (the catalogue of cases:
//! `tests/vault/README.md`): real Typst compiling with the `baluk/` library.
//! Needs the package cetz 0.4.2 in the Typst cache (or the network: it gets
//! downloaded).

use std::path::PathBuf;

use notes_core::check::check;
use notes_core::figures::FigureOptions;
use notes_core::vault_graph::GraphFilter;

use crate::common::{NOTES, repo};
use notes_core::{LibrarySource, NoteId, NoteKind, Notes, NotesConfig};

const OPTS: FigureOptions = FigureOptions { precision: Some(2) };

fn id(s: &str) -> NoteId {
    NoteId::new(s).unwrap()
}

#[test]
fn themes_come_from_library() {
    let names = NOTES.themes().names();
    assert_eq!(names, ["classic", "night"]);
    assert!(NOTES.themes().themes()[1].dark);
    assert!(NOTES.themes().css().contains("--k-box-def:"));
    // The browser gets the theme fonts, main and fallback (every `font` list).
    let fonts: Vec<_> = NOTES.themes().web_fonts().iter().map(|f| (f.name.as_str(), f.math)).collect();
    assert_eq!(
        fonts,
        [
            ("DejaVu Sans Mono", false),
            ("Gentium Plus", false),
            ("JetBrains Mono", false),
            ("New Computer Modern", false),
            ("New Computer Modern Math", true)
        ]
    );
}

#[test]
fn note_renders_with_anchors_links_and_tags() {
    let page = NOTES.page(&id("Сеть/SSH"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    assert_eq!(page.kind, NoteKind::Note);
    let r = page.rendered.as_ref().unwrap();

    assert_eq!(r.title.as_deref(), Some("SSH"));
    assert_eq!(r.tags, ["сеть", "безопасность"]);
    let texts: Vec<_> = r.headings.iter().map(|h| (h.level, h.text.as_str(), h.id.as_str())).collect();
    assert_eq!(
        texts,
        [
            (2, "Как это работает", "Как-это-работает"),
            (2, "Вход по ключу", "Вход-по-ключу"),
            (2, "Смена порта", "Смена-порта")
        ]
    );
    assert!(r.body.contains(r#"href="/n/Сеть/UFW""#), "a link to an existing note");
    assert!(r.body.contains(r#"href="/n/Сеть/SSH#Вход-по-ключу""#), "a link with an anchor");
}

#[test]
fn figures_share_one_svg_across_themes() {
    let page = NOTES.page(&id("Сеть/SSH"), OPTS).unwrap();
    let r = page.rendered.as_ref().unwrap();
    assert_eq!(r.body.matches(r#"class="k-frame k-fig""#).count(), 1);
    assert!(!r.body.contains("k-frame-v"), "the themes differ only in colors: one SVG");
    assert!(r.body.contains("var(--kf"));
    assert!(r.styles.contains(r#":root[data-theme="night"] [data-k-figs=""#));
}

#[test]
fn figure_precision_changes_version_not_content() {
    let rounded = NOTES.page(&id("демо/визуализация"), OPTS).unwrap();
    let exact = NOTES.page(&id("демо/визуализация"), FigureOptions { precision: None }).unwrap();
    assert_ne!(rounded.version, exact.version, "the client must request the page again");
    let (rounded, exact) = (rounded.rendered.as_ref().unwrap(), exact.rendered.as_ref().unwrap());
    assert!(rounded.body.len() < exact.body.len());
    assert_eq!(rounded.headings, exact.headings);
}

#[test]
fn code_uses_theme_variables() {
    let page = NOTES.page(&id("демо/компоненты"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let body = &page.rendered.as_ref().unwrap().body;
    assert!(body.contains("var(--k-code-key)"));
    assert!(!body.contains("color: #0101"), "marker colors do not stay in the HTML");
}

#[test]
fn simple_parentheses_do_not_stretch() {
    let page = NOTES.page(&id("демо/компоненты"), OPTS).unwrap();
    let body = &page.rendered.as_ref().unwrap().body;
    // O(n^2): nothing tall inside, so the brackets are plain and tight.
    assert!(body.contains(r#"<mo stretchy="false">(</mo>"#));
}

#[test]
fn version_is_stable_without_changes() {
    let v1 = NOTES.version(&id("Сеть/UFW"), OPTS).unwrap();
    assert_eq!(v1, NOTES.version(&id("Сеть/UFW"), OPTS).unwrap());
    assert_eq!(v1, NOTES.page(&id("Сеть/UFW"), OPTS).unwrap().version);
}

#[test]
fn check_finds_exactly_the_planted_problems() {
    let report = check(&NOTES).unwrap();
    let broken: Vec<_> = report
        .notes
        .iter()
        .flat_map(|n| n.broken_links.iter().map(move |l| (n.id.as_str(), l.target.as_str(), l.anchor.as_deref())))
        .collect();
    assert_eq!(
        broken,
        [
            ("Особые случаи/Ошибка компиляции", "Нет/Из несобравшейся", None),
            ("Особые случаи/Ссылки", "Нет/Такой заметки", None),
            ("Особые случаи/Ссылки", "Сеть/SSH", Some("Нет такого раздела")),
            ("Сеть/UFW", "Сеть/Nginx", None),
        ],
        "an anchor by label (\"особый\"), names with + # % and deep nesting are not broken"
    );
    let failing: Vec<_> = report.notes.iter().filter(|n| !n.errors.is_empty()).map(|n| n.id.as_str()).collect();
    assert_eq!(failing, ["Особые случаи/Ошибка компиляции"]);
    let warned: Vec<_> = report.notes.iter().filter(|n| !n.warnings.is_empty()).map(|n| n.id.as_str()).collect();
    assert_eq!(warned, ["Особые случаи/Предупреждение"]);
    let folders: Vec<_> = report.folders.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(folders, ["Глубоко/а/б/_folder.toml"]);
    assert_eq!(report.summary(), expected_summary(), "the summary in tests/vault/README.md");
}

/// The expected `notes check` summary: the line "Итог: `...`" in
/// `tests/vault/README.md` (`tools/check.sh` checks it too).
fn expected_summary() -> String {
    let readme = std::fs::read_to_string(repo().join("tests/vault/README.md")).unwrap();
    let line = readme.lines().find_map(|l| l.strip_prefix("Итог: `")).expect("the line \"Итог: `...`\" in the README");
    line.trim_end_matches('`').to_owned()
}

#[test]
fn service_files_are_hidden() {
    let ids: Vec<_> = NOTES.entries().unwrap().into_iter().map(|e| e.id.as_str().to_owned()).collect();
    assert!(ids.iter().all(|id| !id.contains("Скрыто") && !id.contains("черновик")), "{ids:?}");
    assert!(ids.contains(&"Глубоко/а/б/в/г/Дно".to_owned()));
    assert!(ids.contains(&"Имена/C++ и C#".to_owned()));
}

#[test]
fn book_headings_are_unique_and_labels_are_ids() {
    let page = NOTES.page(&id("Книга"), OPTS).unwrap();
    assert_eq!(page.kind, NoteKind::Book);
    let r = page.rendered.as_ref().unwrap();
    let ids: Vec<_> = r.headings.iter().filter(|h| h.anchor == "Итоги").map(|h| h.id.as_str()).collect();
    assert_eq!(ids, ["Итоги", "Итоги-2", "Итоги-3"], "equal headings in chapters");
    let labelled = r.headings.iter().find(|h| h.text == "Особый раздел").unwrap();
    assert_eq!(labelled.id, "особый", "the label <особый> becomes the id");
    assert_eq!(labelled.anchor, "Особый-раздел", "and it is found by text too");
    assert!(r.body.contains("<img src=\"data:image/svg+xml"), "an image from a file");
}

#[test]
fn swallowed_semicolon_is_a_warning() {
    let page = NOTES.page(&id("Особые случаи/Предупреждение"), OPTS).unwrap();
    let lint = page.warnings.iter().find(|w| w.message.contains("\";\" after")).expect("the warning about \";\"");
    assert_eq!(lint.file.as_deref(), Some("/Особые случаи/Предупреждение.typ"));
    assert_eq!(lint.line, Some(15));
}

#[test]
fn lang_without_dictionary_is_a_warning() {
    let page = NOTES.page(&id("Особые случаи/Предупреждение"), OPTS).unwrap();
    let lint = page.warnings.iter().find(|w| w.message.contains("no styling words")).expect("the language warning");
    assert_eq!((lint.line, lint.column), (Some(3), Some(18)), "the place is the lang: argument");
    // Own words are not a warning.
    let own = NOTES.page(&id("Особые случаи/Свои слова"), OPTS).unwrap();
    assert!(own.warnings.is_empty(), "{:?}", own.warnings);
    assert!(own.rendered.as_ref().unwrap().body.contains("Abb. 1."));
}

#[test]
fn theme_dependent_figure_keeps_variants() {
    let page = NOTES.page(&id("Рисунки/Темы и градиент"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let body = &page.rendered.as_ref().unwrap().body;
    assert!(body.contains("k-frame-v"), "the shape depends on the theme: one variant per theme");
    assert!(body.contains("var(--kf"), "an ordinary figure next to it is still merged");
}

#[test]
fn note_without_library_renders() {
    let page = NOTES.page(&id("Особые случаи/Без шаблона"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let r = page.rendered.as_ref().unwrap();
    assert_eq!(r.headings.len(), 2);
    assert!(r.body.contains("<math"));
}

#[test]
fn missing_note_is_not_found() {
    assert!(matches!(NOTES.page(&id("Нет/такой"), OPTS), Err(notes_core::Error::NotFound(_))));
}

#[test]
fn disk_cache_survives_restart() {
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
    let first = open().page(&id("Сеть/UFW"), OPTS).unwrap();
    let files = walkdir(dir.path());
    assert_eq!(files.len(), 2, "the page is written to the cache: the record and the rendering, {files:?}");
    let second = open().page(&id("Сеть/UFW"), OPTS).unwrap();
    assert_eq!(first.version, second.version);
    assert_eq!(first.rendered.as_ref().unwrap().body, second.rendered.as_ref().unwrap().body);
}

/// All files of a directory (recursively).
fn walkdir(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().is_dir() {
            out.extend(walkdir(&e.path()));
        } else {
            out.push(e.path());
        }
    }
    out
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

#[test]
fn vault_graph_follows_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let head = "#import \"/_baluk/lib.typ\": *\n#show: note.with(title: [x])\n";
    std::fs::write(dir.path().join("A.typ"), format!("{head}#see(\"B\")\n")).unwrap();
    std::fs::write(dir.path().join("B.typ"), head).unwrap();
    std::fs::write(dir.path().join("Граф.typ"), format!("{head}#vault-graph(around: \"B\")\n")).unwrap();
    let notes = Notes::open(&NotesConfig {
        trash: None,
        vault: dir.path().to_path_buf(),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .unwrap();
    let page = notes.page(&id("Граф"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let body = &page.rendered.as_ref().unwrap().body;
    assert!(body.contains(r#"class="k-graph""#) && body.contains("data-k-graph"), "markup for the client");
    assert!(body.contains("&quot;id&quot;:&quot;A&quot;") && !body.contains("&quot;id&quot;:&quot;C&quot;"));

    // An edit that does not change the graph (text, a new note without links in
    // the same folder, outside the neighbours of B) leaves the graph note alone.
    std::fs::write(dir.path().join("A.typ"), format!("{head}Текст. #see(\"B\")\n")).unwrap();
    std::fs::write(dir.path().join("D.typ"), head).unwrap();
    assert_eq!(notes.version(&id("Граф"), OPTS).unwrap(), page.version, "the version follows the graph answer");

    // A new note links to B: the note's graph is stale and gets rebuilt.
    std::fs::write(dir.path().join("C.typ"), format!("{head}#see(\"B\")\n")).unwrap();
    assert_ne!(notes.version(&id("Граф"), OPTS).unwrap(), page.version, "the graph changed");
    let body = notes.page(&id("Граф"), OPTS).unwrap().rendered.clone().unwrap().body.clone();
    assert!(body.contains("&quot;id&quot;:&quot;C&quot;"));
}

#[test]
fn concurrent_requests_share_one_build() {
    let notes = Notes::open(&NotesConfig {
        trash: None,
        vault: repo().join("tests/vault"),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .unwrap();
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(|| notes.page(&id("демо/визуализация"), OPTS).unwrap());
        let b = s.spawn(|| notes.page(&id("демо/визуализация"), OPTS).unwrap());
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(std::sync::Arc::ptr_eq(&a, &b), "the second request waited for the first build instead of building again");
}

#[test]
fn search_finds_sections_with_exact_anchors() {
    let hits = NOTES.search("итоги второй", 10).unwrap();
    let first = &hits[0];
    assert_eq!(first.id.as_str(), "Книга");
    assert_eq!(first.heading.as_deref(), Some("Итоги"));
    assert_eq!(first.anchor.as_deref(), Some("Итоги-2"), "a repeated heading: the id as in the rendering");

    let hits = NOTES.search("ОСОБЫЙ раздел", 10).unwrap();
    assert_eq!(hits[0].anchor.as_deref(), Some("особый"), "a heading label is its id");

    let hits = NOTES.search("ssh-keygen", 10).unwrap();
    assert!(hits.iter().any(|h| h.id.as_str() == "Сеть/SSH"), "code in text is searched");
    assert!(hits[0].snippet.iter().any(|f| f.hit && f.text == "ssh-keygen"));

    assert!(NOTES.search("нетакогословавхранилище", 10).unwrap().is_empty());
    assert!(NOTES.search("   ", 10).unwrap().is_empty());
}

#[test]
fn search_in_book_lists_all_sections_in_text_order() {
    let book = id("Книга");
    let all = NOTES.search("итоги", 50).unwrap();
    let inside = NOTES.search_in(&book, "итоги", 50).unwrap();
    assert!(inside.iter().all(|h| h.id == book), "only this book");
    assert!(inside.len() >= all.iter().filter(|h| h.id == book).count(), "no fewer than in the general search");
    let anchors: Vec<_> = inside.iter().filter_map(|h| h.anchor.as_deref()).collect();
    let first = anchors.iter().position(|a| *a == "Итоги").unwrap();
    let second = anchors.iter().position(|a| *a == "Итоги-2").unwrap();
    assert!(first < second, "in text order: {anchors:?}");
    assert!(NOTES.search_in(&id("Нет такой"), "итоги", 5).is_err());
}

#[test]
fn computed_links_come_from_built_pages() {
    let notes = Notes::open(&NotesConfig {
        trash: None,
        vault: repo().join("tests/vault"),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .unwrap();
    let target = id("Формулы и теги");
    let from = |notes: &Notes| -> Vec<String> {
        notes.index().unwrap().backlinks(&target).into_iter().map(|b| b.from.to_string()).collect()
    };
    assert!(!from(&notes).contains(&"Особые случаи/Ссылки".to_owned()), "the path in the source is computed");
    notes.page(&id("Особые случаи/Ссылки"), OPTS).unwrap();
    assert!(from(&notes).contains(&"Особые случаи/Ссылки".to_owned()), "after the build: from its links");
}

#[test]
fn index_has_titles_and_tags() {
    let index = NOTES.index().unwrap();
    let book = index.outline(&id("Книга")).unwrap();
    assert_eq!(book.title.as_deref(), Some("Тестовая книга"));
    assert_eq!(book.tags, ["книга", "фикстура"]);
    let plain = index.outline(&id("Особые случаи/Без шаблона")).unwrap();
    assert_eq!(plain.title, None);
}

#[test]
fn preview_of_note_and_section() {
    let p = NOTES.preview(&id("Сеть/SSH"), None).unwrap();
    assert_eq!(p.title, "SSH");
    assert_eq!(p.heading, None);
    assert!(p.text.starts_with("SSH — протокол"), "{}", p.text);
    // The anchor as in a link: the heading text, its slug or its label.
    for anchor in ["Смена порта", "Смена-порта"] {
        let p = NOTES.preview(&id("Сеть/SSH"), Some(anchor)).unwrap();
        assert_eq!(p.heading.as_deref(), Some("Смена порта"), "{anchor}");
    }
    let p = NOTES.preview(&id("Книга"), Some("особый")).unwrap();
    assert_eq!(p.heading.as_deref(), Some("Особый раздел"));
    let p = NOTES.preview(&id("Книга"), Some("Итоги-2")).unwrap();
    assert!(p.text.contains("второй главы"), "{}", p.text);
    assert!(NOTES.preview(&id("Нет/такой"), None).is_err());
}

#[test]
fn decoration_words_follow_note_language() {
    let page = NOTES.page(&id("Особые случаи/English"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let body = &page.rendered.as_ref().unwrap().body;
    assert!(body.contains(r#"data-doc="book" lang="en""#), "the language is an <article> attribute");
    for word in [
        r#"<div class="k-title-kind">Notes</div>"#,
        r#"data-word="Chapter""#,
        "Definition 1.1 (limit).",
        "Theorem 1.2.",
        "Why this holds.",
        "Example 1.1 (squeeze).",
        "Step 1. Estimate.",
        "Answer:",
        "In this chapter",
        "Key points",
        "Check yourself",
        "Answers",
        "Mistake",
        "How to avoid",
        "Complexity:",
        "Fig. 2.1.",
        "frame 1 of 2",
    ] {
        assert!(body.contains(word), "no \"{word}\"");
    }
    for word in ["Определение", "Теорема", "Шаг", "Ответ", "Рис.", "Глава", "Сложность", "кадр"]
    {
        assert!(!body.contains(word), "the Russian \"{word}\" in an English note");
    }
}

#[test]
fn storage_in_memory_compiles_and_follows_edits() {
    let mem = std::sync::Arc::new(notes_core::storage::MemStorage::new());
    let head = "#import \"/_baluk/lib.typ\": *\n#show: note.with(title: [x])\n";
    mem.write("A.typ", format!("{head}= Раз\n#include \"часть.typ\"\n"));
    mem.write("часть.typ", "первая часть");
    let config = NotesConfig {
        trash: None,
        vault: PathBuf::new(),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    };
    let notes = Notes::with_storage(mem.clone(), &config).unwrap();
    let page = notes.page(&id("A"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    assert!(page.rendered.as_ref().unwrap().body.contains("первая часть"));

    // Editing an included file: a new version and new text.
    mem.write("часть.typ", "вторая часть");
    assert_ne!(notes.version(&id("A"), OPTS).unwrap(), page.version);
    assert!(notes.page(&id("A"), OPTS).unwrap().rendered.as_ref().unwrap().body.contains("вторая часть"));
}

/// Deletion: a note is a file, a book is a whole folder; the list and links
/// update at once (with a watcher that keeps the walk too).
#[test]
fn delete_note_and_book() {
    let mem = std::sync::Arc::new(notes_core::storage::MemStorage::new());
    let head = "#import \"/_baluk/lib.typ\": *\n#show: note.with(title: [x])\n";
    mem.write("A.typ", format!("{head}#see(\"Сеть/B\")"));
    mem.write("Сеть/B.typ", head);
    mem.write("Книга/main.typ", "#import \"/_baluk/lib.typ\": *\n#show: book.with(title: [К])\n#include \"01.typ\"\n");
    mem.write("Книга/01.typ", "= Глава");
    let config = NotesConfig {
        trash: None,
        vault: PathBuf::new(),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    };
    let notes = Notes::with_storage(mem.clone(), &config).unwrap();
    assert!(notes.watch());
    assert!(notes.index().unwrap().exists("Сеть/B"));

    notes.delete(&id("Сеть/B")).unwrap();
    let index = notes.index().unwrap();
    assert!(!index.exists("Сеть/B"), "the list updates at once, without waiting for the watcher");
    assert!(index.exists("A"));

    notes.delete(&id("Книга")).unwrap();
    assert!(
        notes_core::storage::Storage::list(&*mem).unwrap().iter().all(|f| !f.starts_with("Книга/")),
        "a book goes as a whole folder"
    );
    let ids: Vec<String> = notes.entries().unwrap().into_iter().map(|e| e.id.to_string()).collect();
    assert_eq!(ids, ["A"]);

    assert!(matches!(notes.delete(&id("Нет")), Err(notes_core::Error::NotFound(_))));

    mem.write("Папка/Вложенная/Заметка.typ", "= З");
    mem.write("Папка/Другая.typ", "= Д");
    assert!(notes.index().unwrap().exists("Папка/Вложенная/Заметка"));
    assert!(
        matches!(notes.delete_folder(&id("Папка/Другая")), Err(notes_core::Error::NotFound(_))),
        "a file is not a folder"
    );
    notes.delete_folder(&id("Папка")).unwrap();
    let ids: Vec<String> = notes.entries().unwrap().into_iter().map(|e| e.id.to_string()).collect();
    assert_eq!(ids, ["A"], "the folder goes whole, with nested ones");
    assert!(matches!(notes.delete_folder(&id("Папка")), Err(notes_core::Error::NotFound(_))));
}

/// The graph with chapters: a book is the root with chapters around it; a link
/// into a book section goes to that section's chapter, a chapter's link goes
/// from the chapter; a tag filter covers chapter tags too.
#[test]
fn book_chapters_on_graph() {
    let index = NOTES.index().unwrap();
    let plain = index.graph();
    assert!(plain.nodes.iter().all(|n| n.chapter.is_none()), "without chapters: as before");
    let g = index.graph_of(true);
    let chapters: Vec<_> = g
        .nodes
        .iter()
        .filter_map(|n| Some((n.id.as_str(), n.title.as_str(), n.chapter.as_ref()?.anchor.as_str())))
        .filter(|(id, ..)| id.starts_with("Книга/"))
        .collect();
    assert_eq!(
        chapters,
        [
            ("Книга/.1", "Основы", "гл-основы"),
            ("Книга/.2", "Продолжение", "Продолжение"),
            ("Книга/.3", "Приложение", "Приложение")
        ]
    );
    let edge = |from: &str, to: &str| g.edges.iter().find(|e| e.from == from && e.to == to);
    for c in ["Книга/.1", "Книга/.2", "Книга/.3"] {
        assert!(edge("Книга", c).is_some_and(|e| e.chapter), "book - {c}");
    }
    // Inside the book: the first chapter links to the second (by text and by label).
    assert_eq!(edge("Книга/.1", "Книга/.2").map(|e| (e.count, e.chapter)), Some((2, false)));
    // Outside: "Ссылки" links into a section of the second chapter.
    assert!(edge("Особые случаи/Ссылки", "Книга/.2").is_some());
    assert!(edge("Особые случаи/Ссылки", "Книга").is_none(), "a link with an anchor goes to the chapter");

    let shown = |tag: &str| {
        let filter = GraphFilter { tag: Some(tag.into()), chapters: true, ..GraphFilter::default() };
        let mut ids: Vec<String> = index.graph_layout(&filter).nodes.into_iter().map(|n| n.id).collect();
        ids.retain(|id| id.starts_with("Книга"));
        ids
    };
    // The root's tag is on every chapter; a chapter's tag only on it (and on the book, as before).
    assert_eq!(shown("книга"), ["Книга", "Книга/.1", "Книга/.2", "Книга/.3"]);
    assert_eq!(shown("код"), ["Книга", "Книга/.2"]);
    let layout = index.graph_layout(&GraphFilter { chapters: true, ..GraphFilter::default() });
    let group = |id: &str| layout.nodes.iter().find(|n| n.id == id).map(|n| n.group.clone());
    assert_eq!(group("Книга/.1"), group("Книга"), "a chapter has its book's colors");
}
