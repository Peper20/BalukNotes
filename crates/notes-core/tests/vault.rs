//! Сквозная проверка на тестовом хранилище `examples/vault`: настоящая
//! компиляция Typst с библиотекой `konspekt/`. Нужен пакет cetz 0.4.2 в
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
        vault: repo().join("examples/vault"),
        library: LibrarySource::Dir(repo().join("konspekt")),
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
    assert_eq!(names, ["классика", "ночь"]);
    assert!(NOTES.themes().themes()[1].dark);
    assert!(NOTES.themes().css().contains("--k-box-def:"));
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
    assert!(r.styles.contains(r#":root[data-theme="ночь"] [data-k-figs=""#));
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
fn check_finds_broken_links_only() {
    let report = check(&NOTES).unwrap();
    let broken: Vec<_> = report
        .notes
        .iter()
        .flat_map(|n| n.broken_links.iter().map(move |l| (n.id.as_str(), l.target.as_str())))
        .collect();
    assert_eq!(broken, [("Сеть/UFW", "Сеть/Nginx")]);
    assert!(report.notes.iter().all(|n| n.errors.is_empty()), "заметки собираются");
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
            vault: repo().join("examples/vault"),
            library: LibrarySource::Dir(repo().join("konspekt")),
            font_dirs: vec![],
            cache: Some(dir.path().to_path_buf()),
        })
        .unwrap()
    };
    let first = open().page(&id("Сеть/UFW"), OPTS).unwrap();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "страница записана в кэш");
    let second = open().page(&id("Сеть/UFW"), OPTS).unwrap();
    assert_eq!(first.version, second.version);
    assert_eq!(first.rendered.as_ref().unwrap().body, second.rendered.as_ref().unwrap().body);
}
