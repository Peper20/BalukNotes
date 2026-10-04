//! A template for a new note or book (`notes new`): the library template
//! (`note` or `book`). The progress header (goal, plan, what next) is written
//! by the agent with the `/baluk-note` skill, not by the template (the user's
//! decision).
//!
//! Nothing existing is overwritten; there are no notes inside a book, only
//! chapters. Templates build without errors or warnings (tests below and the
//! `baluk-note` step in `tools/check.sh`).
//!
//! The title lives in the file itself (`title: [...]`), so anything goes in it.
//! The file name comes from the title ([`file_name`]): no characters forbidden
//! in Windows file names and no internal `_`/`.` at the start; a taken name
//! gets a number ([`NewNote::id_in`]).

use std::collections::HashSet;
use std::fmt::Write;

use crate::vault::{BOOK_MAIN, NoteId, NoteKind, Vault};
use crate::{Error, Result};

/// What to create.
#[derive(Debug, Clone)]
pub struct NewNote {
    pub kind: NoteKind,
    /// The title; `None` means the last path segment (only with an explicit path).
    pub title: Option<String>,
    pub tags: Vec<String>,
    /// The note language (`lang:`); `None` and `"ru"` mean the library default.
    pub lang: Option<String>,
}

impl NewNote {
    /// The text of the template's main file.
    pub fn source(&self, id: &NoteId) -> String {
        let template = match self.kind {
            NoteKind::Note => "note",
            NoteKind::Book => "book",
        };
        let title = self.title.as_deref().map_or(id.name(), str::trim);
        let mut args = format!("  title: [{}],\n", markup(title));
        if let Some(lang) = self.lang.as_deref().filter(|l| *l != "ru") {
            let _ = writeln!(args, "  lang: {},", string(lang));
        }
        if !self.tags.is_empty() {
            let list: Vec<String> = self.tags.iter().map(|t| string(t)).collect();
            let comma = if list.len() == 1 { "," } else { "" };
            let _ = writeln!(args, "  tags: ({}{comma}),", list.join(", "));
        }
        let mut out = format!("#import \"/_baluk/lib.typ\": *\n#show: {template}.with(\n{args})\n");
        if self.kind == NoteKind::Book {
            out += "\n// This file is the root of the book: the language and tags above apply to every chapter.\n\
                    // Chapters are NN-topic.typ files next to it (#import, then `= Title` or\n\
                    // #show: chapter.with(title: [...], tags: (...)) for the chapter's own tags),\n\
                    // one line per chapter:\n// #include \"01-topic.typ\"\n";
        }
        out
    }

    /// The path of a new note in the folder `folder` (empty for the root): the
    /// file name comes from the title ([`file_name`]); if taken (ignoring case:
    /// on Windows and macOS `SSH` and `ssh` are one file), `Name 2`, `Name 3`...
    pub fn id_in(&self, vault: &Vault, folder: &str) -> Result<NoteId> {
        let title = self.title.as_deref().unwrap_or_default();
        let refuse = |reason: String| Err(Error::Create { id: title.to_owned(), reason });
        if title.trim().is_empty() {
            return refuse("empty title".into());
        }
        match free_id(vault, folder, &file_name(title), None)? {
            Ok(id) => Ok(id),
            Err(reason) => refuse(reason),
        }
    }

    /// Creates the template in the vault. Returns the path of the main file from
    /// the vault root.
    pub fn create(&self, vault: &Vault, id: &NoteId) -> Result<String> {
        let refuse = |reason: String| Err(Error::Create { id: id.to_string(), reason });
        if self.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
            return refuse("empty title".into());
        }
        if let Some(lang) = &self.lang
            && (!(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()))
        {
            return refuse(format!(
                "the language is an ISO 639 code in lowercase Latin letters (ru, en, de), not \"{lang}\""
            ));
        }
        let storage = vault.storage();
        let exists = |path: &str| storage.stat(path).is_ok();
        // Files inside a book are its chapters, not notes.
        let segments: Vec<&str> = id.as_str().split('/').collect();
        for end in 1..segments.len() {
            let folder = segments[..end].join("/");
            if exists(&format!("{folder}/{BOOK_MAIN}")) {
                return refuse(format!(
                    "\"{folder}\" is a book: it holds chapters, not notes (an NN-topic.typ file and #include in its {BOOK_MAIN})"
                ));
            }
        }
        if exists(&format!("{id}.typ")) {
            return refuse("the note already exists".into());
        }
        if exists(id.as_str()) {
            return refuse(format!("a folder \"{id}\" already exists (a book or a section with this name)"));
        }
        let main = match self.kind {
            NoteKind::Note => format!("{id}.typ"),
            NoteKind::Book => format!("{id}/{BOOK_MAIN}"),
        };
        storage.create(&main, self.source(id).as_bytes()).map_err(|e| Error::io(storage.display(&main), e))?;
        Ok(main)
    }
}

/// A free path in the folder `folder` (empty for the root) for the name `base`:
/// if taken (ignoring case: on Windows and macOS `SSH` and `ssh` are one file),
/// `base 2`, `base 3`... The name `except` (the one being renamed) does not count
/// as taken. The inner `Err(reason)` means there is no free number.
pub(crate) fn free_id(
    vault: &Vault,
    folder: &str,
    base: &str,
    except: Option<&str>,
) -> Result<std::result::Result<NoteId, String>> {
    let prefix = if folder.is_empty() { String::new() } else { format!("{}/", NoteId::new(folder)?) };
    // Names in the folder: files (without .typ) and subfolders (empty ones too), lowercase.
    let files = vault.storage().list().map_err(|e| vault.io_error(folder, e))?;
    let dirs = vault.storage().dirs().map_err(|e| vault.io_error(folder, e))?;
    let except = except.map(str::to_lowercase);
    let taken: HashSet<String> = files
        .iter()
        .chain(&dirs)
        .filter_map(|f| f.strip_prefix(&prefix))
        .map(|rest| rest.split_once('/').map_or_else(|| rest.strip_suffix(".typ").unwrap_or(rest), |(dir, _)| dir))
        .map(str::to_lowercase)
        .filter(|name| Some(name) != except.as_ref())
        .collect();
    let exists = |name: &str| {
        let stem = name.strip_suffix(".typ").unwrap_or(name).to_lowercase();
        let own = except.as_deref() == Some(stem.as_str());
        taken.contains(&name.to_lowercase()) || (!own && vault.storage().stat(&format!("{prefix}{name}")).is_ok())
    };
    for n in 1..=MAX_NUMBER {
        let name = if n == 1 { base.to_owned() } else { format!("{base} {n}") };
        if !exists(&name) && !exists(&format!("{name}.typ")) {
            return Ok(Ok(NoteId::new(format!("{prefix}{name}"))?));
        }
    }
    Ok(Err(format!("the folder already has {MAX_NUMBER} notes \"{base}\"")))
}

/// How many numbers to try for a taken name.
const MAX_NUMBER: usize = 1000;

/// The longest file name made from a title, in characters.
pub const MAX_FILE_NAME: usize = 80;

/// And in bytes: the file system limit is 255 bytes (a Cyrillic letter takes
/// two, an emoji four); room is left for the number ` 1000` and `.typ`.
const MAX_FILE_BYTES: usize = 200;

/// The name when nothing is left of the title (`???`).
pub const UNTITLED: &str = "Без названия";

/// Characters forbidden in Windows file names (and `/`, the separator everywhere).
const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Windows device names: `CON`, `con.txt` are not files.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The file name (without `.typ`) or book folder name made from a title:
///
/// - characters forbidden on Windows `/ \ : * ? " < > |` and control
///   characters become spaces, whitespace is collapsed;
/// - no `_` or `.` at the start (internal names); no `.` or spaces at the end
///   (Windows cuts them) and no `.typ`;
/// - at most [`MAX_FILE_NAME`] characters and 200 bytes (by whole characters);
/// - a Windows device name (`CON`, `lpt1.x`) and `main` (the main file of a book
///   would turn the folder into a book) get `_` after the stem;
/// - if nothing is left, [`UNTITLED`].
pub fn file_name(title: &str) -> String {
    let spaced: String =
        title.chars().map(|c| if c.is_control() || FORBIDDEN.contains(&c) { ' ' } else { c }).collect();
    let mut name = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut cut = false;
    loop {
        let before = name.len();
        name = name.trim_start_matches(['_', '.', ' ']).trim_end_matches(['.', ' ']).to_owned();
        if name.len() > 4
            && name.is_char_boundary(name.len() - 4)
            && name[name.len() - 4..].eq_ignore_ascii_case(".typ")
        {
            name.truncate(name.len() - 4);
        }
        if !cut && (name.chars().count() > MAX_FILE_NAME || name.len() > MAX_FILE_BYTES) {
            let mut bytes = 0;
            name = name
                .chars()
                .take(MAX_FILE_NAME)
                .take_while(|c| {
                    bytes += c.len_utf8();
                    bytes <= MAX_FILE_BYTES
                })
                .collect();
            cut = true;
        }
        if name.len() == before {
            break;
        }
    }
    if name.is_empty() {
        return UNTITLED.to_owned();
    }
    let stem = name.split('.').next().unwrap_or(&name);
    // `Main.typ` is the same as `main.typ` on Windows.
    if RESERVED.iter().chain(&["main"]).any(|r| stem.eq_ignore_ascii_case(r)) {
        name.insert(stem.len(), '_');
    }
    name
}

/// Text in Typst markup `[...]`: markup characters are escaped with `\`,
/// shorthands too (`...` -> an ellipsis, `1.` at the start - a list) and quotes
/// (Typst would make them "smart"): the title in the file is exactly what was typed.
pub(crate) fn markup(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '\n' | '\r' => out.push(' '),
            '[' | ']' | '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '=' | '~' | '/' | '-' | '+' | '.'
            | '\'' | '"' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// A quoted Typst string.
fn string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::storage::{MemStorage, Storage};

    fn new(kind: NoteKind) -> NewNote {
        NewNote { kind, title: None, tags: vec![], lang: None }
    }

    fn id(s: &str) -> NoteId {
        NoteId::new(s).unwrap()
    }

    fn text(mem: &MemStorage, path: &str) -> String {
        String::from_utf8(mem.read(path).unwrap()).unwrap()
    }

    #[test]
    fn source_of_note_and_book() {
        let note = NewNote { tags: vec!["сеть".into()], ..new(NoteKind::Note) }.source(&id("Сеть/SSH"));
        assert!(note.contains("#show: note.with(\n  title: [SSH],\n  tags: (\"сеть\",),\n)\n"), "{note}");
        assert!(note.starts_with("#import"), "no header: the skill writes it");
        let book = NewNote { lang: Some("en".into()), tags: vec!["a".into(), "b\"".into()], ..new(NoteKind::Book) }
            .source(&id("Курсы/Матан"));
        assert!(book.contains("  lang: \"en\",\n  tags: (\"a\", \"b\\\"\"),\n"), "{book}");
        assert!(book.contains("#include \"01-topic.typ\""));
        let ru = NewNote { lang: Some("ru".into()), ..new(NoteKind::Note) }.source(&id("x"));
        assert!(!ru.contains("lang:"), "Russian is the default");
    }

    #[test]
    fn title_markup_is_escaped() {
        let title = "C++ [1] #x $y$ a/b // c\n- d";
        let src = NewNote { title: Some(title.into()), ..new(NoteKind::Note) }.source(&id("x"));
        assert!(src.contains(r"title: [C\+\+ \[1\] \#x \$y\$ a\/b \/\/ c \- d],"), "{src}");
    }

    #[test]
    fn create_refuses_conflicts() {
        let mem = Arc::new(MemStorage::new());
        mem.write("Сеть/SSH.typ", "old");
        mem.write("Книги/Пределы/main.typ", "");
        let vault = Vault::new(mem.clone());
        let err = |kind, path: &str| new(kind).create(&vault, &id(path)).unwrap_err().to_string();
        assert!(err(NoteKind::Note, "Сеть/SSH").contains("the note already exists"));
        assert!(err(NoteKind::Book, "Сеть").contains("a folder \"Сеть\" already exists"));
        assert!(err(NoteKind::Note, "Книги/Пределы/Глава").contains("\"Книги/Пределы\" is a book"));
        let lang = NewNote { lang: Some("Русский".into()), ..new(NoteKind::Note) };
        assert!(lang.create(&vault, &id("x")).unwrap_err().to_string().contains("ISO 639"));
        assert_eq!(text(&mem, "Сеть/SSH.typ"), "old", "nothing existing is touched");

        assert_eq!(new(NoteKind::Note).create(&vault, &id("Сеть/UFW")).unwrap(), "Сеть/UFW.typ");
        assert_eq!(new(NoteKind::Book).create(&vault, &id("Курсы/Матан")).unwrap(), "Курсы/Матан/main.typ");
        assert!(text(&mem, "Курсы/Матан/main.typ").contains("#show: book.with("));

        let blank = NewNote { title: Some("  ".into()), ..new(NoteKind::Note) };
        assert!(blank.create(&vault, &id("y")).unwrap_err().to_string().contains("empty title"));
    }

    #[test]
    fn file_name_from_title() {
        let cases = [
            ("SSH: основы работы", "SSH основы работы"),
            ("Ввод/вывод", "Ввод вывод"),
            (r#"a\b:c*d?e"f<g>h|i"#, "a b c d e f g h i"),
            ("  много   пробелов\tи\nстрок  ", "много пробелов и строк"),
            ("Что такое C++?", "Что такое C++"),
            ("#$@&%!", "#$@&%!"),
            // Internal names and endings that Windows cuts.
            ("_черновик", "черновик"),
            (".скрытое", "скрытое"),
            ("__. _.x", "x"),
            ("Итоги...", "Итоги"),
            ("Итоги . . .", "Итоги"),
            ("заметка.typ", "заметка"),
            ("заметка.TYP.typ", "заметка"),
            ("v1.2 релиз", "v1.2 релиз"),
            // Windows device names and the main file of a book.
            ("CON", "CON_"),
            ("con.txt", "con_.txt"),
            ("Lpt9", "Lpt9_"),
            ("CONSOLE", "CONSOLE"),
            ("COM10", "COM10"),
            ("main", "main_"),
            ("Main", "Main_"),
            ("main идея", "main идея"),
            // Nothing is left.
            ("", UNTITLED),
            ("   ", UNTITLED),
            (r#"/\:*?"<>|"#, UNTITLED),
            ("...", UNTITLED),
            ("_", UNTITLED),
            (".typ", "typ"),
            ("\u{0}\u{7}\u{1b}", UNTITLED),
            ("@#$@&$*%@#!.:/\\", "@#$@&$ %@#!"),
        ];
        for (title, want) in cases {
            let got = file_name(title);
            assert_eq!(got, want, "title {title:?}");
            NoteId::new(got.clone()).unwrap_or_else(|e| panic!("{title:?} -> {got:?}: {e}"));
        }
    }

    #[test]
    fn file_name_is_bounded() {
        let long = "дл".repeat(100);
        assert_eq!(file_name(&long).chars().count(), MAX_FILE_NAME);
        // Cut by character, not by byte, and the end is cleaned again after the cut.
        let dotted = format!("{}. . . хвост", "я".repeat(MAX_FILE_NAME - 3));
        assert_eq!(file_name(&dotted), "я".repeat(MAX_FILE_NAME - 3));
        let wide = file_name(&"🙂".repeat(200));
        assert!(
            wide.len() <= 200 && wide.chars().all(|c| c == '🙂'),
            "within the file system limit in bytes, by whole characters"
        );
    }

    #[test]
    fn id_from_title_is_free_and_title_survives() {
        let mem = Arc::new(MemStorage::new());
        mem.write("Сеть/SSH основы.typ", "");
        mem.write("Сеть/Без названия.typ", "");
        mem.write("Сеть/Книга/main.typ", "");
        mem.write("Сеть/Протоколы/DNS.typ", "");
        let vault = Vault::new(mem.clone());
        let titled = |t: &str| NewNote { title: Some(t.into()), ..new(NoteKind::Note) };
        let path = |t: &str, folder: &str| titled(t).id_in(&vault, folder).map(|id| id.to_string());

        assert_eq!(path("DNS", "Сеть").unwrap(), "Сеть/DNS", "a name in a subfolder does not interfere");
        assert_eq!(path("SSH: основы", "Сеть").unwrap(), "Сеть/SSH основы 2", "taken: with a number");
        assert_eq!(path("ssh ОСНОВЫ", "Сеть").unwrap(), "Сеть/ssh ОСНОВЫ 2", "case does not matter");
        assert_eq!(path("протоколы", "Сеть").unwrap(), "Сеть/протоколы 2", "a folder with the same name exists");
        assert_eq!(path("книга", "Сеть").unwrap(), "Сеть/книга 2", "so does a book");
        assert_eq!(path("???", "Сеть").unwrap(), "Сеть/Без названия 2");
        assert_eq!(path("Вне папок", "").unwrap(), "Вне папок");
        assert_eq!(path("a/b", "").unwrap(), "a b", "/ in a title is not a folder");
        assert_eq!(path("x", "Новая/Глубже").unwrap(), "Новая/Глубже/x", "a missing folder will be created");

        for bad in ["", "   ", "\n\t"] {
            assert!(path(bad, "Сеть").unwrap_err().to_string().contains("empty title"), "{bad:?}");
        }
        assert!(
            NewNote { title: None, ..new(NoteKind::Note) }.id_in(&vault, "").is_err(),
            "without a title there is nothing to take"
        );
        assert!(path("x", "_служебная").is_err(), "the folder follows the path rules");
        assert!(path("x", "Сеть/").is_err());

        // The title in the file is as typed, with every character; the index reads it back.
        let odd_titles = [
            "@#$@&$*%@#!.:/\\",
            "C++ [1] #x $y$",
            "Итоги... <черновик>",
            "a_b*c*",
            "= не заголовок",
            "- не список",
            "1. не список",
            "It's \"так\" -- и ~ --- -?",
            "https://example.org // не комментарий",
        ];
        for title in odd_titles {
            let note = titled(title);
            let id = note.id_in(&vault, "Сеть").unwrap();
            let main = note.create(&vault, &id).unwrap();
            let outline = crate::outline::parse_outline(&text(&mem, &main));
            assert_eq!(outline.title.as_deref(), Some(title), "{title:?} -> {main}");
        }
        // A book: the title is in main.typ, the folder name comes from the title.
        let book = NewNote { title: Some("Матан: кратные".into()), ..new(NoteKind::Book) };
        let id = book.id_in(&vault, "Курсы").unwrap();
        assert_eq!(book.create(&vault, &id).unwrap(), "Курсы/Матан кратные/main.typ");
        // Inside a book: not allowed, as with an explicit path.
        let inside = titled("Глава").id_in(&vault, "Сеть/Книга").unwrap();
        assert!(titled("Глава").create(&vault, &inside).unwrap_err().to_string().contains("is a book"));
    }
}
