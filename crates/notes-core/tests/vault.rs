//! Сквозная проверка на тестовом хранилище `tests/vault` (каталог случаев —
//! `tests/vault/README.md`): настоящая
//! компиляция Typst с библиотекой `baluk/`. Нужен пакет cetz 0.4.2 в
//! кэше Typst (или сеть — он скачается).

use std::path::PathBuf;
use std::sync::LazyLock;

use notes_core::check::check;
use notes_core::figures::FigureOptions;
use notes_core::{LibrarySource, NoteId, NoteKind, Notes, NotesConfig};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Одно ядро на все тесты: загрузка шрифтов и тем — самое долгое.
static NOTES: LazyLock<Notes> = LazyLock::new(|| {
    Notes::open(&NotesConfig {
        vault: repo().join("tests/vault"),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .expect("тестовое хранилище открывается")
});

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
    // Браузеру — основные шрифты тем (первые в списках `font`), без запасных.
    assert_eq!(NOTES.themes().web_fonts(), ["Gentium Plus", "JetBrains Mono", "New Computer Modern Math"]);
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
    assert!(r.body.contains(r#"href="/n/Сеть/UFW""#), "ссылка на существующую заметку");
    assert!(r.body.contains(r#"href="/n/Сеть/SSH#Вход-по-ключу""#), "ссылка с якорем");
}

#[test]
fn figures_share_one_svg_across_themes() {
    let page = NOTES.page(&id("Сеть/SSH"), OPTS).unwrap();
    let r = page.rendered.as_ref().unwrap();
    assert_eq!(r.body.matches(r#"class="k-frame k-fig""#).count(), 1);
    assert!(!r.body.contains("k-frame-v"), "темы различаются только цветами — один SVG");
    assert!(r.body.contains("var(--kf"));
    assert!(r.styles.contains(r#":root[data-theme="night"] [data-k-figs=""#));
}

#[test]
fn figure_precision_changes_version_not_content() {
    let rounded = NOTES.page(&id("демо/визуализация"), OPTS).unwrap();
    let exact = NOTES.page(&id("демо/визуализация"), FigureOptions { precision: None }).unwrap();
    assert_ne!(rounded.version, exact.version, "клиент должен перезапросить страницу");
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
    assert!(!body.contains("color: #0101"), "опорные цвета не остаются в HTML");
}

#[test]
fn simple_parentheses_do_not_stretch() {
    let page = NOTES.page(&id("демо/компоненты"), OPTS).unwrap();
    let body = &page.rendered.as_ref().unwrap().body;
    // O(n^2): внутри нет высокого — скобки обычные, плотные.
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
        "якорь по метке («особый»), имена с + # % и глубокая вложенность — не битые"
    );
    let failing: Vec<_> = report.notes.iter().filter(|n| !n.errors.is_empty()).map(|n| n.id.as_str()).collect();
    assert_eq!(failing, ["Особые случаи/Ошибка компиляции"]);
    let warned: Vec<_> = report.notes.iter().filter(|n| !n.warnings.is_empty()).map(|n| n.id.as_str()).collect();
    assert_eq!(warned, ["Особые случаи/Предупреждение"]);
    assert_eq!(report.summary(), expected_summary(), "итог в tests/vault/README.md");
}

/// Ожидаемый итог `notes check` — строка «Итог: `…`» в `tests/vault/README.md`
/// (её же сверяет `tools/check.sh`).
fn expected_summary() -> String {
    let readme = std::fs::read_to_string(repo().join("tests/vault/README.md")).unwrap();
    let line = readme.lines().find_map(|l| l.strip_prefix("Итог: `")).expect("строка «Итог: `…`» в README");
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
    assert_eq!(ids, ["Итоги", "Итоги-2", "Итоги-3"], "одинаковые заголовки в главах");
    let labelled = r.headings.iter().find(|h| h.text == "Особый раздел").unwrap();
    assert_eq!(labelled.id, "особый", "метка <особый> становится id");
    assert_eq!(labelled.anchor, "Особый-раздел", "и по тексту тоже находится");
    assert!(r.body.contains("<img src=\"data:image/svg+xml"), "картинка из файла");
}

#[test]
fn swallowed_semicolon_is_a_warning() {
    let page = NOTES.page(&id("Особые случаи/Предупреждение"), OPTS).unwrap();
    let lint = page.warnings.iter().find(|w| w.message.contains("«;»")).expect("предупреждение о «;»");
    assert_eq!(lint.file.as_deref(), Some("/Особые случаи/Предупреждение.typ"));
    assert_eq!(lint.line, Some(15));
}

#[test]
fn lang_without_dictionary_is_a_warning() {
    let page = NOTES.page(&id("Особые случаи/Предупреждение"), OPTS).unwrap();
    let lint =
        page.warnings.iter().find(|w| w.message.contains("нет слов оформления")).expect("предупреждение о языке");
    assert_eq!((lint.line, lint.column), (Some(3), Some(18)), "место — аргумент lang:");
    // Свои слова — не предупреждение.
    let own = NOTES.page(&id("Особые случаи/Свои слова"), OPTS).unwrap();
    assert!(own.warnings.is_empty(), "{:?}", own.warnings);
    assert!(own.rendered.as_ref().unwrap().body.contains("Abb. 1."));
}

#[test]
fn theme_dependent_figure_keeps_variants() {
    let page = NOTES.page(&id("Рисунки/Темы и градиент"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let body = &page.rendered.as_ref().unwrap().body;
    assert!(body.contains("k-frame-v"), "форма зависит от темы — по варианту на тему");
    assert!(body.contains("var(--kf"), "обычный рисунок рядом всё равно склеен");
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
            vault: repo().join("tests/vault"),
            library: LibrarySource::Dir(repo().join("baluk")),
            font_dirs: vec![],
            cache: Some(dir.path().to_path_buf()),
        })
        .unwrap()
    };
    let first = open().page(&id("Сеть/UFW"), OPTS).unwrap();
    let files = walkdir(dir.path());
    assert_eq!(files.len(), 2, "страница записана в кэш: запись и отрисовка — {files:?}");
    let second = open().page(&id("Сеть/UFW"), OPTS).unwrap();
    assert_eq!(first.version, second.version);
    assert_eq!(first.rendered.as_ref().unwrap().body, second.rendered.as_ref().unwrap().body);
}

/// Все файлы каталога (рекурсивно).
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
    assert_eq!((stats.built, stats.skipped), (all, 0), "первый проход собирает всё");
    assert_eq!(first.memory().0, 0, "прогрев — только на диск");
    assert_eq!(first.warm_pass().built, 0, "второй — ничего");

    // Новый запуск: собранное (и заметки с ошибкой) лежит на диске.
    let failed =
        first.entries().unwrap().iter().filter(|e| !first.page(&e.id, OPTS).unwrap().errors.is_empty()).count();
    assert!(failed > 0, "в tests/vault есть заметка с ошибкой");
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
        vault: dir.path().to_path_buf(),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .unwrap();
    let page = notes.page(&id("Граф"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let body = &page.rendered.as_ref().unwrap().body;
    assert!(body.contains(r#"class="k-graph""#) && body.contains("data-k-graph"), "разметка для клиента");
    assert!(body.contains("&quot;id&quot;:&quot;A&quot;") && !body.contains("&quot;id&quot;:&quot;C&quot;"));

    // Правка, не меняющая граф (текст, новая заметка без связей в той же
    // папке, — вне соседей B), заметку с графом не трогает.
    std::fs::write(dir.path().join("A.typ"), format!("{head}Текст. #see(\"B\")\n")).unwrap();
    std::fs::write(dir.path().join("D.typ"), head).unwrap();
    assert_eq!(notes.version(&id("Граф"), OPTS).unwrap(), page.version, "версия — по ответу графа");

    // Новая заметка ссылается на B — граф заметки устарел и пересобирается.
    std::fs::write(dir.path().join("C.typ"), format!("{head}#see(\"B\")\n")).unwrap();
    assert_ne!(notes.version(&id("Граф"), OPTS).unwrap(), page.version, "граф изменился");
    let body = notes.page(&id("Граф"), OPTS).unwrap().rendered.clone().unwrap().body.clone();
    assert!(body.contains("&quot;id&quot;:&quot;C&quot;"));
}

#[test]
fn concurrent_requests_share_one_build() {
    let notes = Notes::open(&NotesConfig {
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
    assert!(std::sync::Arc::ptr_eq(&a, &b), "второй запрос дождался первой сборки, а не собрал заново");
}

#[test]
fn search_finds_sections_with_exact_anchors() {
    let hits = NOTES.search("итоги второй", 10).unwrap();
    let first = &hits[0];
    assert_eq!(first.id.as_str(), "Книга");
    assert_eq!(first.heading.as_deref(), Some("Итоги"));
    assert_eq!(first.anchor.as_deref(), Some("Итоги-2"), "повтор заголовка — id как у отрисовки");

    let hits = NOTES.search("ОСОБЫЙ раздел", 10).unwrap();
    assert_eq!(hits[0].anchor.as_deref(), Some("особый"), "метка заголовка — его id");

    let hits = NOTES.search("ssh-keygen", 10).unwrap();
    assert!(hits.iter().any(|h| h.id.as_str() == "Сеть/SSH"), "код в тексте ищется");
    assert!(hits[0].snippet.iter().any(|f| f.hit && f.text == "ssh-keygen"));

    assert!(NOTES.search("нетакогословавхранилище", 10).unwrap().is_empty());
    assert!(NOTES.search("   ", 10).unwrap().is_empty());
}

#[test]
fn search_in_book_lists_all_sections_in_text_order() {
    let book = id("Книга");
    let all = NOTES.search("итоги", 50).unwrap();
    let inside = NOTES.search_in(&book, "итоги", 50).unwrap();
    assert!(inside.iter().all(|h| h.id == book), "только эта книга");
    assert!(inside.len() >= all.iter().filter(|h| h.id == book).count(), "не меньше, чем в общем поиске");
    let anchors: Vec<_> = inside.iter().filter_map(|h| h.anchor.as_deref()).collect();
    let first = anchors.iter().position(|a| *a == "Итоги").unwrap();
    let second = anchors.iter().position(|a| *a == "Итоги-2").unwrap();
    assert!(first < second, "по порядку текста: {anchors:?}");
    assert!(NOTES.search_in(&id("Нет такой"), "итоги", 5).is_err());
}

#[test]
fn computed_links_come_from_built_pages() {
    let notes = Notes::open(&NotesConfig {
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
    assert!(!from(&notes).contains(&"Особые случаи/Ссылки".to_owned()), "в исходнике путь вычисляемый");
    notes.page(&id("Особые случаи/Ссылки"), OPTS).unwrap();
    assert!(from(&notes).contains(&"Особые случаи/Ссылки".to_owned()), "после сборки — из её ссылок");
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
    // Якорь — как в ссылке: текст заголовка, его слаг или метка.
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
    assert!(body.contains(r#"data-doc="book" lang="en""#), "язык — атрибутом <article>");
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
        assert!(body.contains(word), "нет «{word}»");
    }
    for word in ["Определение", "Теорема", "Шаг", "Ответ", "Рис.", "Глава", "Сложность", "кадр"]
    {
        assert!(!body.contains(word), "русское «{word}» в английской заметке");
    }
}

#[test]
fn storage_in_memory_compiles_and_follows_edits() {
    let mem = std::sync::Arc::new(notes_core::storage::MemStorage::new());
    let head = "#import \"/_baluk/lib.typ\": *\n#show: note.with(title: [x])\n";
    mem.write("A.typ", format!("{head}= Раз\n#include \"часть.typ\"\n"));
    mem.write("часть.typ", "первая часть");
    let config = NotesConfig {
        vault: PathBuf::new(),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    };
    let notes = Notes::with_storage(mem.clone(), &config).unwrap();
    let page = notes.page(&id("A"), OPTS).unwrap();
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    assert!(page.rendered.as_ref().unwrap().body.contains("первая часть"));

    // Правка включённого файла — новая версия и новый текст.
    mem.write("часть.typ", "вторая часть");
    assert_ne!(notes.version(&id("A"), OPTS).unwrap(), page.version);
    assert!(notes.page(&id("A"), OPTS).unwrap().rendered.as_ref().unwrap().body.contains("вторая часть"));
}
