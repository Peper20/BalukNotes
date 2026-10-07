//! The public interface of the `baluk` library: the names a note sees after
//! `#import "/_baluk/lib.typ": *`. An accidental rename or a lost re-export
//! when the library modules are reshuffled is caught by the snapshot
//! `tests/snapshots/baluk-api.txt`:
//!
//!   cargo test -p notes-core --test it library::                       # compare
//!   UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library::    # update
//!
//! A name added or removed on purpose: update the snapshot and the library
//! README (the second test checks that every name is mentioned there). The
//! size of the skill texts, `tests/snapshots/skill-size.txt`, is updated the same way.

use std::fmt::Write as _;
use std::path::Path;

use crate::common::{compile, repo};

/// The names of the `lib.typ` module in alphabetical order, as a note sees them.
fn public_names() -> Vec<String> {
    let page = compile("#import \"/_baluk/lib.typ\" as baluk\n#dictionary(baluk).keys().sorted().join(\" \")\n");
    assert!(page.errors.is_empty(), "lib.typ did not build: {:?}", page.errors);
    let rendered = page.rendered.as_ref().expect("no rendering");
    let body = &rendered.body;
    let text = body.trim().trim_start_matches("<p>").trim_end_matches("</p>");
    assert!(!text.contains('<'), "unexpected markup: {body}");
    text.split_whitespace().map(str::to_owned).collect()
}

/// No `_` at the start: internal names (`_color`, `_num`) are not the interface.
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
        "the public baluk names changed - gone: {gone:?}, new: {new:?}.\n\
         If that is intended: UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library:: \
         and fix baluk/README.md"
    );
}

/// Every public name is mentioned in the library README, in code (`...` or a
/// ``` block): a new function without a description does not pass.
#[test]
fn every_public_name_is_documented() {
    let readme = std::fs::read_to_string(repo().join("baluk/README.md")).unwrap();
    let code = code_spans(&readme);
    let missing: Vec<String> =
        public_names().into_iter().filter(|n| is_public(n) && !code.iter().any(|c| has_word(c, n))).collect();
    assert!(missing.is_empty(), "not described in baluk/README.md: {missing:?}");
}

/// The writing rules (`docs/writing.md`) and the `/baluk-note` skill
/// (`skills/baluk-note/SKILL.md`) are not stale: every `#name` call in them is a
/// public library name (except Typst keywords).
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
        assert!(unknown.is_empty(), "{file}: not in the library {unknown:?}; fix the rules");
    }
}

/// Shells (Claude Code, opencode) substitute call arguments into the skill
/// text: `$ARGUMENTS` is the user's request (without it the model does not know
/// the task), `$0`, `$1`... are single words. So `$ARGUMENTS` is in the skill,
/// at the start and at the end, in `<request>` tags (the model tells the task
/// from the instructions), and `$` with a digit (a formula `$0$`) is not: the
/// substitution would break it.
#[test]
fn skill_gets_arguments_and_has_no_positional_placeholders() {
    let skill = std::fs::read_to_string(repo().join("skills/baluk-note/SKILL.md")).unwrap();
    assert_eq!(
        skill.matches("<request>\n$ARGUMENTS\n</request>").count(),
        2,
        "SKILL.md: the user's request - `<request>`, `$ARGUMENTS`, `</request>` on separate lines, at the start and at the end"
    );
    let bad: Vec<&str> =
        skill.lines().filter(|l| l.split('$').skip(1).any(|r| r.starts_with(|c: char| c.is_ascii_digit()))).collect();
    assert!(bad.is_empty(), "SKILL.md: \"$digit\" gets replaced by a call argument: {bad:?}");
}

/// The examples of the `/baluk-note` skill (`skills/baluk-note/examples/`) show
/// every public name: a weak model writes by example, not by description.
/// That they build without errors is the `baluk-note` step in `tools/check.sh`.
#[test]
fn skill_examples_use_every_public_name() {
    // A custom theme is a library edit, not a note: the examples do not have one.
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
    assert!(missing.is_empty(), "not in the skill examples skills/baluk-note/examples: {missing:?}");
}

/// The skill reference (`skills/baluk-note/reference.md`) is complete: every
/// public function has a signature line in a code block, and its named
/// parameters and defaults are as in the library source. An extra parameter is
/// allowed only if the function takes `..rest`.
#[test]
fn skill_reference_matches_library() {
    const NOT_FOR_NOTES: [&str; 3] = ["customize", "themes", "cetz"];
    let reference = std::fs::read_to_string(repo().join("skills/baluk-note/reference.md")).unwrap();
    let blocks: Vec<&str> = reference.split("```").skip(1).step_by(2).collect();
    let sources = library_sources();
    let mut problems = Vec::new();
    for name in public_names().into_iter().filter(|n| is_public(n)) {
        if !has_word(&reference, &name) {
            problems.push(format!("{name}: not in the reference"));
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
            problems.push(format!("{name}: no \"{name}(...)\" line in a code block"));
            continue;
        };
        let (real, real_rest) = named_params(source);
        let (doc, _) = named_params(documented);
        for (param, default) in &real {
            match doc.iter().find(|(p, _)| p == param) {
                None => problems.push(format!("{name}: parameter {param} not described")),
                Some((_, d)) if d != default && !default.starts_with('_') => {
                    problems.push(format!("{name}: {param} defaults to {default}, the reference says {d}"));
                }
                Some(_) => {}
            }
        }
        if !real_rest {
            for (param, _) in doc.iter().filter(|(p, _)| !real.iter().any(|(r, _)| r == p)) {
                problems.push(format!("{name}: the library has no parameter {param}"));
            }
        }
    }
    assert!(problems.is_empty(), "skills/baluk-note/reference.md differs from the library:\n{}", problems.join("\n"));
}

/// The library sources `baluk/` (every `.typ`).
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

/// The parameters of a function `#let name(...)` from the sources; `None` if it is not a function.
fn find_signature<'a>(sources: &'a [String], name: &str) -> Option<&'a str> {
    let head = format!("#let {name}(");
    let mut found = sources.iter().flat_map(|s| s.match_indices(&head).map(move |(i, _)| (s, i)));
    let (text, i) = found.next()?;
    assert!(found.next().is_none(), "{name}: defined twice in the library");
    Some(signature(&text[i..], head.len() - 1))
}

/// The text between the bracket `(` at `open` and its matching `)`.
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
    panic!("unclosed bracket: {text}")
}

/// Named parameters `name: value` (the value without extra spaces) and whether
/// there is `..rest`.
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

/// Markdown code: ``` ... ``` blocks and `...` inline.
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

/// `name` as a whole word in the text: the neighbours are not letters, digits, `-` or `_`.
fn has_word(text: &str, name: &str) -> bool {
    let is_name_char = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    text.match_indices(name).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + name.len()..].chars().next();
        !before.is_some_and(is_name_char) && !after.is_some_and(is_name_char)
    })
}

/// The design dictionaries are complete: every language has every key of every dictionary.
#[test]
fn dictionaries_have_same_keys() {
    let page = compile(
        "#import \"/_baluk/i18n.typ\": words\n\
         #let all = words.values().map(d => d.keys()).flatten().dedup()\n\
         #for (lang, dict) in words { for key in all { if key not in dict [#lang: #key; ] } }\n",
    );
    assert!(page.errors.is_empty(), "{:?}", page.errors);
    let body = &page.rendered.as_ref().expect("no rendering").body;
    assert!(!body.contains(':'), "the dictionaries lack words (language: key): {body}");
}

/// A language not in the dictionaries gets English words, including those
/// missing from its own `words:`.
#[test]
fn unknown_language_falls_back_to_english() {
    let body = |template: &str| {
        let page = compile(&format!(
            "#import \"/_baluk/lib.typ\": *\n#show: note.with({template})\n#definition[x]\n#remark[y]\n"
        ));
        assert!(page.errors.is_empty(), "{:?}", page.errors);
        page.rendered.as_ref().expect("no rendering").body.clone()
    };
    let plain = body("lang: \"uk\"");
    assert!(plain.contains("Definition") && plain.contains("Remark"), "{plain}");
    let own = body("lang: \"de\", words: (definition: \"Begriff\")");
    assert!(own.contains("Begriff") && own.contains("Remark"), "{own}");
    let russian = body("");
    assert!(russian.contains("Определение") && russian.contains("Замечание"), "{russian}");
}

/// `words:` of the template: an unknown key and a non-string are errors with a hint.
#[test]
fn own_words_are_checked() {
    let error = |words: &str| {
        let page =
            compile(&format!("#import \"/_baluk/lib.typ\": *\n#show: note.with(lang: \"de\", words: {words})\nText\n"));
        page.errors.first().map(|e| e.message.clone()).unwrap_or_default()
    };
    assert!(error("(figur: \"Abb.\")").contains("no word \"figur\""), "{}", error("(figur: \"Abb.\")"));
    assert!(error("(figure: [Abb.])").contains("is a string"));
    assert!(error("\"Abb.\"").contains("words is a dictionary"));
    assert_eq!(error("(figure: \"Abb.\")"), "");
}

/// `frames(..., pdf:)`: frame numbers from 1 to the number of frames.
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
        assert!(error(bad).contains("frame numbers from 1 to 5"), "{bad}: {}", error(bad));
    }
}

/// `chapter`: only in a book; the title is required, tags are an array of strings.
#[test]
fn chapter_is_checked() {
    let error = |template: &str, chapter: &str| {
        let page = compile(&format!(
            "#import \"/_baluk/lib.typ\": *\n#show: {template}.with(title: [Книга])\n#show: chapter.with({chapter})\nТекст\n"
        ));
        page.errors.first().map(|e| e.message.clone()).unwrap_or_default()
    };
    assert_eq!(error("book", "title: [Глава], tags: (\"тег\",), label: \"гл\""), "");
    assert!(error("note", "title: [Глава]").contains("only for a book chapter"), "{}", error("note", "title: [Глава]"));
    assert!(error("book", "tags: (\"тег\",)").contains("needs a chapter title"));
    assert!(error("book", "title: [Глава], tags: \"тег\"").contains("array of strings"));
    assert!(error("book", "title: [Глава], label: <гл>").contains("label is a string"));
}

/// The texts of the `/baluk-note` skill (`SKILL.md`, `reference.md`, examples)
/// are for a model: English and plain Markdown without typography
/// (`skills/README.md`). Cyrillic and `« » — …` are allowed only in inline
/// `code` (verbatim `notes` messages, Russian design words) and in
/// `argument-hint` (a person sees it).
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
            // Outside Markdown code blocks: no inline `...`.
            let checked: String =
                if markdown && !fenced { line.split('`').step_by(2).collect() } else { line.to_owned() };
            if checked.chars().any(forbidden) {
                problems.push(format!("{name}:{}: {line}", i + 1));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "Cyrillic or typography in the skill texts (quote notes output in `code`):\n{}",
        problems.join("\n")
    );
}

/// The skill size is a budget: the text grows only on purpose. We count
/// characters (English text has about a quarter as many tokens); more than
/// 10 % over the recorded size fails the test. Intended growth: update with
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
    let _ = writeln!(table, "{total} total");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &table).unwrap();
        return;
    }
    let recorded = std::fs::read_to_string(&path).unwrap_or_default();
    let budget: usize =
        recorded.lines().find_map(|l| l.strip_suffix(" total")).and_then(|n| n.parse().ok()).expect(
            "no tests/snapshots/skill-size.txt: UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library::",
        );
    #[expect(clippy::cast_precision_loss, reason = "the text is thousands of tokens, well within f64 precision")]
    let over = total as f64 > budget as f64 * GROWTH;
    assert!(
        !over,
        "the skill grew by more than 10 %: it was {budget} characters, now {total}. Cut it or, if the growth is intended, \
         UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it library::\n{table}"
    );
}

/// The files of the `/baluk-note` skill the model reads: the path from `skills/baluk-note/`, and the text.
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
