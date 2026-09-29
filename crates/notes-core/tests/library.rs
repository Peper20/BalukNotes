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
        trash: None,
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

/// Правила написания заметок (`docs/writing.md`) и навык `/baluk-note`
/// (`skills/baluk-note/SKILL.md`) не устарели: каждый вызов `#имя` из них —
/// публичное имя библиотеки (кроме ключевых слов Typst).
#[test]
fn writing_guide_uses_public_names() {
    const KEYWORDS: [&str; 5] = ["import", "include", "show", "set", "let"];
    let names = public_names();
    for file in ["docs/writing.md", "skills/baluk-note/SKILL.md"] {
        let guide = std::fs::read_to_string(repo().join(file)).unwrap();
        let mut unknown: Vec<&str> = guide
            .split('#')
            .skip(1)
            .map(|rest| {
                let end = rest.find(|c: char| !(c.is_ascii_lowercase() || c == '-')).unwrap_or(rest.len());
                rest[..end].trim_end_matches('-')
            })
            .filter(|name| !name.is_empty() && !KEYWORDS.contains(name) && !names.iter().any(|n| n == name))
            .collect();
        unknown.dedup();
        assert!(unknown.is_empty(), "{file}: нет в библиотеке {unknown:?} — поправьте правила");
    }
}

/// Оболочки (Claude Code, opencode) подставляют в текст навыка аргументы
/// вызова: `$ARGUMENTS` — просьба пользователя (без неё модель не знает
/// задачи), `$0`, `$1`… — отдельные слова. Поэтому `$ARGUMENTS` в навыке
/// есть, а `$` с цифрой (формула `$0$`) — нет: её испортит подстановка.
#[test]
fn skill_gets_arguments_and_has_no_positional_placeholders() {
    let skill = std::fs::read_to_string(repo().join("skills/baluk-note/SKILL.md")).unwrap();
    assert!(skill.contains("$ARGUMENTS"), "SKILL.md: нет $ARGUMENTS — просьба пользователя не попадёт к модели");
    let bad: Vec<&str> =
        skill.lines().filter(|l| l.split('$').skip(1).any(|r| r.starts_with(|c: char| c.is_ascii_digit()))).collect();
    assert!(bad.is_empty(), "SKILL.md: «$цифра» заменится аргументом вызова: {bad:?}");
}

/// Примеры навыка `/baluk-note` (`skills/baluk-note/examples/`) показывают
/// каждое публичное имя: слабая модель пишет по образцу, а не по описанию.
/// Что они собираются без ошибок — шаг `baluk-note` в `tools/check.sh`.
#[test]
fn skill_examples_use_every_public_name() {
    // Своя тема — правка библиотеки, а не заметки: в примерах её нет.
    const NOT_FOR_NOTES: [&str; 2] = ["customize", "themes"];
    let mut code = String::new();
    let mut dirs = vec![repo().join("skills/baluk-note/examples")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "typ") {
                code += &std::fs::read_to_string(path).unwrap();
            }
        }
    }
    let missing: Vec<String> = public_names()
        .into_iter()
        .filter(|n| is_public(n) && !NOT_FOR_NOTES.contains(&n.as_str()) && !has_word(&code, n))
        .collect();
    assert!(missing.is_empty(), "нет в примерах навыка skills/baluk-note/examples: {missing:?}");
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

/// Словари оформления полные: у каждого языка — все ключи всех словарей.
#[test]
fn dictionaries_have_same_keys() {
    let page = compile(
        "#import \"/_baluk/i18n.typ\": words\n\
         #let all = words.values().map(d => d.keys()).flatten().dedup()\n\
         #for (lang, dict) in words { for key in all { if key not in dict [#lang: #key; ] } }\n",
    );
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let body = &page.rendered.as_ref().expect("нет отрисовки").body;
    assert!(!body.contains(':'), "в словарях не хватает слов (язык: ключ): {body}");
}

/// Языка нет в словарях — слова из английского, в том числе те, которых
/// нет в своих `words:`.
#[test]
fn unknown_language_falls_back_to_english() {
    let body = |template: &str| {
        let page = compile(&format!(
            "#import \"/_baluk/lib.typ\": *\n#show: note.with({template})\n#definition[x]\n#remark[y]\n"
        ));
        assert!(page.errors.is_empty(), "{:?}", page.errors);
        page.rendered.as_ref().expect("нет отрисовки").body.clone()
    };
    let plain = body("lang: \"uk\"");
    assert!(plain.contains("Definition") && plain.contains("Remark"), "{plain}");
    let own = body("lang: \"de\", words: (definition: \"Begriff\")");
    assert!(own.contains("Begriff") && own.contains("Remark"), "{own}");
    let russian = body("");
    assert!(russian.contains("Определение") && russian.contains("Замечание"), "{russian}");
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
