//! Публичный интерфейс библиотеки `baluk`: имена, которые видит заметка
//! после `#import "/_baluk/lib.typ": *`. Случайное переименование или
//! пропавший реэкспорт при перекройке модулей библиотеки ловит снимок
//! `tests/snapshots/baluk-api.txt`:
//!
//!   cargo test -p notes-core --test library                       # сравнить
//!   UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test library    # обновить
//!
//! Имя добавлено или убрано намеренно — обновить снимок и README библиотеки
//! (второй тест проверяет, что каждое имя там упомянуто).

use std::path::PathBuf;
use std::sync::Arc;

use notes_core::figures::FigureOptions;
use notes_core::{LibrarySource, NoteId, NotePage, Notes, NotesConfig};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Собирает заметку из одного файла с библиотекой репозитория.
fn compile(source: &str) -> Arc<NotePage> {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("t.typ"), source).unwrap();
    let notes = Notes::open(&NotesConfig {
        vault: vault.path().to_path_buf(),
        library: LibrarySource::Dir(repo().join("baluk")),
        font_dirs: vec![],
        cache: None,
    })
    .unwrap();
    notes.page(&NoteId::new("t").unwrap(), FigureOptions::default()).unwrap()
}

/// Имена модуля `lib.typ` по алфавиту — так, как их видит заметка.
fn public_names() -> Vec<String> {
    let page = compile("#import \"/_baluk/lib.typ\" as baluk\n#dictionary(baluk).keys().sorted().join(\" \")\n");
    assert!(page.errors.is_empty(), "lib.typ не собрался: {:?}", page.errors);
    let rendered = page.rendered.as_ref().expect("нет отрисовки");
    let body = &rendered.body;
    let text = body.trim().trim_start_matches("<p>").trim_end_matches("</p>");
    assert!(!text.contains('<'), "неожиданная разметка: {body}");
    text.split_whitespace().map(str::to_owned).collect()
}

/// Без `_` в начале: служебные имена (`_color`, `_num`) — не интерфейс.
fn is_public(name: &str) -> bool {
    !name.starts_with('_')
}

#[test]
fn public_api_matches_snapshot() {
    let names: Vec<String> = public_names().into_iter().filter(|n| is_public(n)).collect();
    let actual = names.join("\n") + "\n";
    let path = repo().join("tests/snapshots/baluk-api.txt");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    if actual == expected {
        return;
    }
    let old: Vec<&str> = expected.lines().collect();
    let gone: Vec<&str> = old.iter().copied().filter(|n| !names.iter().any(|m| m == n)).collect();
    let new: Vec<&str> = names.iter().map(String::as_str).filter(|n| !old.contains(n)).collect();
    panic!(
        "публичные имена baluk изменились — пропали: {gone:?}, появились: {new:?}.\n\
         Если так задумано: UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test library \
         и поправьте baluk/README.md"
    );
}

/// Каждое публичное имя упомянуто в README библиотеки — в коде (`…` или
/// блоке ```): новая функция без описания не проходит.
#[test]
fn every_public_name_is_documented() {
    let readme = std::fs::read_to_string(repo().join("baluk/README.md")).unwrap();
    let code = code_spans(&readme);
    let missing: Vec<String> =
        public_names().into_iter().filter(|n| is_public(n) && !code.iter().any(|c| has_word(c, n))).collect();
    assert!(missing.is_empty(), "не описаны в baluk/README.md: {missing:?}");
}

/// Куски кода Markdown: блоки ``` … ``` и `…` в строке.
fn code_spans(md: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for (i, part) in md.split("```").enumerate() {
        if i % 2 == 1 {
            out.push(part);
        } else {
            out.extend(part.split('`').skip(1).step_by(2));
        }
    }
    out
}

/// `name` в тексте целым словом: соседи — не буквы, не цифры, не `-` и `_`.
fn has_word(text: &str, name: &str) -> bool {
    let is_name_char = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    text.match_indices(name).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + name.len()..].chars().next();
        !before.is_some_and(is_name_char) && !after.is_some_and(is_name_char)
    })
}

/// `words:` шаблона: неизвестный ключ и не строка — ошибка с подсказкой.
#[test]
fn own_words_are_checked() {
    let error = |words: &str| {
        let page =
            compile(&format!("#import \"/_baluk/lib.typ\": *\n#show: note.with(lang: \"de\", words: {words})\nText\n"));
        page.errors.first().map(|e| e.message.clone()).unwrap_or_default()
    };
    assert!(error("(figur: \"Abb.\")").contains("нет слова «figur»"), "{}", error("(figur: \"Abb.\")"));
    assert!(error("(figure: [Abb.])").contains("— строка"));
    assert!(error("\"Abb.\"").contains("words — словарь"));
    assert_eq!(error("(figure: \"Abb.\")"), "");
}

/// `frames(…, pdf:)`: номера кадров от 1 до числа кадров.
#[test]
fn frames_pdf_is_checked() {
    let error = |pdf: &str| {
        let page = compile(&format!(
            "#import \"/_baluk/lib.typ\": *\n#show: note.with()\n#frames(n => [#n], n: (from: 1, to: 5), pdf: {pdf})\n"
        ));
        page.errors.first().map(|e| e.message.clone()).unwrap_or_default()
    };
    assert_eq!(error("(1, 3, 5)"), "");
    assert_eq!(error("auto"), "");
    for bad in ["(0, 2)", "(6,)", "()", "\"strip\"", "(1.5,)"] {
        assert!(error(bad).contains("номера кадров от 1 до 5"), "{bad}: {}", error(bad));
    }
}
