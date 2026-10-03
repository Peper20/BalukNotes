//! Публичный интерфейс библиотеки `baluk`: имена, которые видит заметка
//! после `#import "/_baluk/lib.typ": *`. Случайное переименование или
//! пропавший реэкспорт при перекройке модулей библиотеки ловит снимок
//! `tests/snapshots/baluk-api.txt`:
//!
//!   cargo test -p notes-core --test it library::                       # сравнить
//!   UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library::    # обновить
//!
//! Имя добавлено или убрано намеренно — обновить снимок и README библиотеки
//! (второй тест проверяет, что каждое имя там упомянуто). Так же обновляется
//! размер текстов навыка `tests/snapshots/skill-size.txt`.

use std::fmt::Write as _;
use std::path::Path;

use crate::common::{compile, repo};

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
         Если так задумано: UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library:: \
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
/// есть — в начале и в конце, в тегах `<request>` (модель отделяет задачу от
/// инструкций), а `$` с цифрой (формула `$0$`) — нет: её испортит подстановка.
#[test]
fn skill_gets_arguments_and_has_no_positional_placeholders() {
    let skill = std::fs::read_to_string(repo().join("skills/baluk-note/SKILL.md")).unwrap();
    assert_eq!(
        skill.matches("<request>\n$ARGUMENTS\n</request>").count(),
        2,
        "SKILL.md: просьба пользователя — `<request>`, `$ARGUMENTS`, `</request>` отдельными строками, в начале и в конце"
    );
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

/// Справочник навыка (`skills/baluk-note/reference.md`) — полный: у каждой
/// публичной функции строка-сигнатура в блоке кода, её именованные
/// параметры и значения по умолчанию — как в исходнике библиотеки.
/// Лишний параметр допустим, только если функция принимает `..rest`.
#[test]
fn skill_reference_matches_library() {
    const NOT_FOR_NOTES: [&str; 3] = ["customize", "themes", "cetz"];
    let reference = std::fs::read_to_string(repo().join("skills/baluk-note/reference.md")).unwrap();
    let blocks: Vec<&str> = reference.split("```").skip(1).step_by(2).collect();
    let sources = library_sources();
    let mut problems = Vec::new();
    for name in public_names().into_iter().filter(|n| is_public(n)) {
        if !has_word(&reference, &name) {
            problems.push(format!("{name}: нет в справочнике"));
            continue;
        }
        let Some(source) = find_signature(&sources, &name) else { continue };
        if NOT_FOR_NOTES.contains(&name.as_str()) {
            continue;
        }
        let Some(documented) = blocks.iter().flat_map(|b| b.lines()).find_map(|line| {
            let line = line.trim_start();
            line.starts_with(&format!("{name}(")).then(|| signature(line, name.len()))
        }) else {
            problems.push(format!("{name}: нет строки «{name}(…)» в блоке кода"));
            continue;
        };
        let (real, real_rest) = named_params(source);
        let (doc, _) = named_params(documented);
        for (param, default) in &real {
            match doc.iter().find(|(p, _)| p == param) {
                None => problems.push(format!("{name}: не описан параметр {param}")),
                Some((_, d)) if d != default && !default.starts_with('_') => {
                    problems.push(format!("{name}: {param} по умолчанию {default}, в справочнике {d}"));
                }
                Some(_) => {}
            }
        }
        if !real_rest {
            for (param, _) in doc.iter().filter(|(p, _)| !real.iter().any(|(r, _)| r == p)) {
                problems.push(format!("{name}: параметра {param} нет в библиотеке"));
            }
        }
    }
    assert!(problems.is_empty(), "skills/baluk-note/reference.md расходится с библиотекой:\n{}", problems.join("\n"));
}

/// Исходники библиотеки `baluk/` (все `.typ`).
fn library_sources() -> Vec<String> {
    let mut out = Vec::new();
    let mut dirs = vec![repo().join("baluk")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "typ") {
                out.push(std::fs::read_to_string(path).unwrap());
            }
        }
    }
    out
}

/// Параметры функции `#let name(…)` из исходников; `None` — не функция.
fn find_signature<'a>(sources: &'a [String], name: &str) -> Option<&'a str> {
    let head = format!("#let {name}(");
    let mut found = sources.iter().flat_map(|s| s.match_indices(&head).map(move |(i, _)| (s, i)));
    let (text, i) = found.next()?;
    assert!(found.next().is_none(), "{name}: определена в библиотеке дважды");
    Some(signature(&text[i..], head.len() - 1))
}

/// Текст между скобкой `(` на позиции `open` и парной ей `)`.
fn signature(text: &str, open: usize) -> &str {
    let mut depth = 0;
    let mut quoted = false;
    for (i, c) in text[open..].char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' | '[' | '{' if !quoted => depth += 1,
            ')' | ']' | '}' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return &text[open + 1..open + i];
                }
            }
            _ => {}
        }
    }
    panic!("не закрыта скобка: {text}")
}

/// Именованные параметры `имя: значение` (значение без лишних пробелов) и
/// есть ли `..rest`.
fn named_params(params: &str) -> (Vec<(String, String)>, bool) {
    let mut parts = Vec::new();
    let (mut depth, mut quoted, mut start) = (0, false, 0);
    for (i, c) in params.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' | '[' | '{' if !quoted => depth += 1,
            ')' | ']' | '}' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                parts.push(&params[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&params[start..]);
    let rest = parts.iter().any(|p| p.trim().starts_with(".."));
    let named = parts
        .iter()
        .filter_map(|p| {
            let (name, value) = p.split_once(':')?;
            let name = name.trim();
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c == '-')
                .then(|| (name.to_owned(), value.split_whitespace().collect::<Vec<_>>().join(" ")))
        })
        .collect();
    (named, rest)
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

/// `chapter`: только в книге; название обязательно, теги — массив строк.
#[test]
fn chapter_is_checked() {
    let error = |template: &str, chapter: &str| {
        let page = compile(&format!(
            "#import \"/_baluk/lib.typ\": *\n#show: {template}.with(title: [Книга])\n#show: chapter.with({chapter})\nТекст\n"
        ));
        page.errors.first().map(|e| e.message.clone()).unwrap_or_default()
    };
    assert_eq!(error("book", "title: [Глава], tags: (\"тег\",), label: \"гл\""), "");
    assert!(error("note", "title: [Глава]").contains("только в главе книги"), "{}", error("note", "title: [Глава]"));
    assert!(error("book", "tags: (\"тег\",)").contains("нужно название"));
    assert!(error("book", "title: [Глава], tags: \"тег\"").contains("массив строк"));
    assert!(error("book", "title: [Глава], label: <гл>").contains("label — строка"));
}

/// Тексты навыка `/baluk-note` (`SKILL.md`, `reference.md`, образцы) — для
/// модели: английский и простой Markdown без типографики (`skills/README.md`).
/// Кириллица и `« » — …` можно только в `код` в строке (дословные сообщения
/// `notes`, русские слова оформления) и в `argument-hint` (его видит человек).
#[test]
fn skill_texts_are_plain_english() {
    let forbidden = |c: char| "«»„“”‘’—–…→←".contains(c) || ('\u{0400}'..='\u{04FF}').contains(&c);
    let mut problems = Vec::new();
    for (name, text) in skill_files() {
        let markdown = Path::new(&name).extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
        let mut fenced = false;
        for (i, line) in text.lines().enumerate() {
            if markdown && line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if markdown && line.starts_with("argument-hint:") {
                continue;
            }
            // Вне блоков кода Markdown — без `…` в строке.
            let checked: String =
                if markdown && !fenced { line.split('`').step_by(2).collect() } else { line.to_owned() };
            if checked.chars().any(forbidden) {
                problems.push(format!("{name}:{}: {line}", i + 1));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "кириллица или типографика в текстах навыка (цитату вывода notes — в `код`):\n{}",
        problems.join("\n")
    );
}

/// Размер навыка — бюджет: текст растёт, только когда это задумано. Меряем
/// знаки (у английского текста токенов около четверти от них); вырос больше
/// чем на 10 % от записанного — тест падает. Задумано — обновить:
/// `UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library::`.
#[test]
fn skill_size_within_budget() {
    const GROWTH: f64 = 1.10;
    let path = repo().join("tests/snapshots/skill-size.txt");
    let files = skill_files();
    let total: usize = files.iter().map(|(_, t)| t.chars().count()).sum();
    let mut table = String::new();
    for (name, text) in &files {
        let _ = writeln!(table, "{} {name}", text.chars().count());
    }
    let _ = writeln!(table, "{total} всего");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &table).unwrap();
        return;
    }
    let recorded = std::fs::read_to_string(&path).unwrap_or_default();
    let budget: usize =
        recorded.lines().find_map(|l| l.strip_suffix(" всего")).and_then(|n| n.parse().ok()).expect(
            "нет tests/snapshots/skill-size.txt — UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library::",
        );
    #[allow(clippy::cast_precision_loss, reason = "размер текста — тысячи знаков, точность f64 с запасом")]
    let over = total as f64 > budget as f64 * GROWTH;
    assert!(
        !over,
        "навык вырос больше чем на 10 %: было {budget} знаков, стало {total}. Сократить или, если рост задуман, \
         UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library::\n{table}"
    );
}

/// Файлы навыка `/baluk-note`, которые читает модель: путь от `skills/baluk-note/` и текст.
fn skill_files() -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                out.push((name, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let root = repo().join("skills/baluk-note");
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out
}
