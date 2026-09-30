//! Заготовка новой заметки или книги (`notes new`): шаблон библиотеки
//! (`note` или `book`) и шапка-комментарий о ходе работы — по ней следующая
//! сессия продолжает с того же места (правила — `docs/writing.md`,
//! «Процесс»).
//!
//! Существующее не перезаписывается; внутри книги заметок нет — там главы.
//! Заготовки собираются без ошибок и предупреждений (тесты ниже и шаг
//! `baluk-note` в `tools/check.sh`).
//!
//! Название живёт в самом файле (`title: […]`), поэтому в нём можно всё. Имя
//! файла — из названия ([`file_name`]): без знаков, запрещённых в именах
//! файлов Windows, и без служебных `_`/`.` в начале; занятое — с номером
//! ([`NewNote::id_in`]).

use std::collections::HashSet;
use std::fmt::Write;

use crate::vault::{BOOK_MAIN, NoteId, NoteKind, Vault};
use crate::{Error, Result};

/// Что заводить.
#[derive(Debug, Clone)]
pub struct NewNote {
    pub kind: NoteKind,
    /// Название; `None` — последний сегмент пути (только с явным путём).
    pub title: Option<String>,
    pub tags: Vec<String>,
    /// Язык заметки (`lang:`); `None` и `"ru"` — по умолчанию библиотеки.
    pub lang: Option<String>,
}

/// Шапка — состояние работы для следующей сессии.
const HEADER: &str = "\
// Работа над заметкой (обновлять после каждого шага; готово — удалить шапку):
//   цель и читатель: —
//   исходники: —
//   план: не согласован
//   сделано: —
//   дальше: выяснить цель и читателя, собрать исходники
";

impl NewNote {
    /// Текст главного файла заготовки.
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
        let mut out = format!("{HEADER}#import \"/_baluk/lib.typ\": *\n#show: {template}.with(\n{args})\n");
        if self.kind == NoteKind::Book {
            out += "\n// Этот файл — корень книги: язык и теги выше общие для всех глав.\n\
                    // Главы — файлы NN-тема.typ рядом (#import, затем `= Название` или\n\
                    // #show: chapter.with(title: […], tags: (…)) — свои теги главы),\n\
                    // по строке на главу:\n// #include \"01-тема.typ\"\n";
        }
        out
    }

    /// Путь новой заметки в папке `folder` (пусто — корень): имя файла из
    /// названия ([`file_name`]); занято (без учёта регистра: в Windows и
    /// macOS `SSH` и `ssh` — один файл) — `Имя 2`, `Имя 3`…
    pub fn id_in(&self, vault: &Vault, folder: &str) -> Result<NoteId> {
        let title = self.title.as_deref().unwrap_or_default();
        let refuse = |reason: String| Err(Error::Create { id: title.to_owned(), reason });
        if title.trim().is_empty() {
            return refuse("пустое название".into());
        }
        let prefix = if folder.is_empty() { String::new() } else { format!("{}/", NoteId::new(folder)?) };
        // Имена в папке: файлы (без .typ) и подпапки — строчными.
        let files = vault.storage().list().map_err(|e| vault.io_error(folder, e))?;
        let taken: HashSet<String> = files
            .iter()
            .filter_map(|f| f.strip_prefix(&prefix))
            .map(|rest| rest.split_once('/').map_or_else(|| rest.strip_suffix(".typ").unwrap_or(rest), |(dir, _)| dir))
            .map(str::to_lowercase)
            .collect();
        let exists = |name: &str| {
            taken.contains(&name.to_lowercase()) || vault.storage().stat(&format!("{prefix}{name}")).is_ok()
        };
        let base = file_name(title);
        for n in 1..=MAX_NUMBER {
            let name = if n == 1 { base.clone() } else { format!("{base} {n}") };
            if !exists(&name) && !exists(&format!("{name}.typ")) {
                return NoteId::new(format!("{prefix}{name}"));
            }
        }
        refuse(format!("в папке уже {MAX_NUMBER} заметок «{base}»"))
    }

    /// Создать заготовку в хранилище. Результат — путь главного файла от
    /// корня хранилища.
    pub fn create(&self, vault: &Vault, id: &NoteId) -> Result<String> {
        let refuse = |reason: String| Err(Error::Create { id: id.to_string(), reason });
        if self.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
            return refuse("пустое название".into());
        }
        if let Some(lang) = &self.lang
            && (!(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()))
        {
            return refuse(format!("язык — код ISO 639 строчными латинскими буквами (ru, en, de), а не «{lang}»"));
        }
        let storage = vault.storage();
        let exists = |path: &str| storage.stat(path).is_ok();
        // Внутри книги файлы — её главы, а не заметки.
        let segments: Vec<&str> = id.as_str().split('/').collect();
        for end in 1..segments.len() {
            let folder = segments[..end].join("/");
            if exists(&format!("{folder}/{BOOK_MAIN}")) {
                return refuse(format!(
                    "«{folder}» — книга: внутри неё не заметки, а главы (файл NN-тема.typ и #include в её {BOOK_MAIN})"
                ));
            }
        }
        if exists(&format!("{id}.typ")) {
            return refuse("заметка уже есть".into());
        }
        if exists(id.as_str()) {
            return refuse(format!("уже есть папка «{id}» (книга или раздел с этим именем)"));
        }
        let main = match self.kind {
            NoteKind::Note => format!("{id}.typ"),
            NoteKind::Book => format!("{id}/{BOOK_MAIN}"),
        };
        storage.create(&main, self.source(id).as_bytes()).map_err(|e| Error::io(storage.display(&main), e))?;
        Ok(main)
    }
}

/// Сколько номеров перебирать для занятого имени.
const MAX_NUMBER: usize = 1000;

/// Самое длинное имя файла из названия, в буквах.
pub const MAX_FILE_NAME: usize = 80;

/// И в байтах: предел файловых систем — 255 байт (русская буква — два,
/// значок — четыре); место — номеру ` 1000` и `.typ`.
const MAX_FILE_BYTES: usize = 200;

/// Имя, если от названия ничего не осталось (`???`).
pub const UNTITLED: &str = "Без названия";

/// Знаки, запрещённые в именах файлов Windows (и `/` — разделитель везде).
const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Имена устройств Windows: `CON`, `con.txt` — не файлы.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Имя файла (без `.typ`) или папки книги из названия:
///
/// - запрещённые в Windows знаки `/ \ : * ? " < > |` и управляющие — пробел,
///   пробелы схлопнуты;
/// - в начале нет `_` и `.` (служебные имена), в конце — `.` и пробелов
///   (Windows их отрезает) и `.typ`;
/// - не длиннее [`MAX_FILE_NAME`] букв и 200 байт (по целым буквам);
/// - имя устройства Windows (`CON`, `lpt1.x`) и `main` (главный файл книги
///   сделал бы папку книгой) — с `_` после основы;
/// - ничего не осталось — [`UNTITLED`].
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
    // `Main.typ` в Windows — тот же `main.typ`.
    if RESERVED.iter().chain(&["main"]).any(|r| stem.eq_ignore_ascii_case(r)) {
        name.insert(stem.len(), '_');
    }
    name
}

/// Текст в разметке Typst `[…]`: знаки разметки — через `\`, в том числе
/// сокращения (`...` → «…», `1.` в начале — список) и кавычки (Typst
/// сделал бы их «умными»): название в файле — ровно то, что ввели.
fn markup(text: &str) -> String {
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

/// Строка Typst в кавычках.
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
        assert!(note.starts_with("// Работа над заметкой"));
        let book = NewNote { lang: Some("en".into()), tags: vec!["a".into(), "b\"".into()], ..new(NoteKind::Book) }
            .source(&id("Курсы/Матан"));
        assert!(book.contains("  lang: \"en\",\n  tags: (\"a\", \"b\\\"\"),\n"), "{book}");
        assert!(book.contains("#include \"01-тема.typ\""));
        let ru = NewNote { lang: Some("ru".into()), ..new(NoteKind::Note) }.source(&id("x"));
        assert!(!ru.contains("lang:"), "русский — по умолчанию");
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
        assert!(err(NoteKind::Note, "Сеть/SSH").contains("заметка уже есть"));
        assert!(err(NoteKind::Book, "Сеть").contains("уже есть папка"));
        assert!(err(NoteKind::Note, "Книги/Пределы/Глава").contains("«Книги/Пределы» — книга"));
        let lang = NewNote { lang: Some("Русский".into()), ..new(NoteKind::Note) };
        assert!(lang.create(&vault, &id("x")).unwrap_err().to_string().contains("ISO 639"));
        assert_eq!(text(&mem, "Сеть/SSH.typ"), "old", "существующее не тронуто");

        assert_eq!(new(NoteKind::Note).create(&vault, &id("Сеть/UFW")).unwrap(), "Сеть/UFW.typ");
        assert_eq!(new(NoteKind::Book).create(&vault, &id("Курсы/Матан")).unwrap(), "Курсы/Матан/main.typ");
        assert!(text(&mem, "Курсы/Матан/main.typ").contains("#show: book.with("));

        let blank = NewNote { title: Some("  ".into()), ..new(NoteKind::Note) };
        assert!(blank.create(&vault, &id("y")).unwrap_err().to_string().contains("пустое название"));
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
            // Служебные имена и хвосты, которые Windows отрезает.
            ("_черновик", "черновик"),
            (".скрытое", "скрытое"),
            ("__. _.x", "x"),
            ("Итоги...", "Итоги"),
            ("Итоги . . .", "Итоги"),
            ("заметка.typ", "заметка"),
            ("заметка.TYP.typ", "заметка"),
            ("v1.2 релиз", "v1.2 релиз"),
            // Имена устройств Windows и главный файл книги.
            ("CON", "CON_"),
            ("con.txt", "con_.txt"),
            ("Lpt9", "Lpt9_"),
            ("CONSOLE", "CONSOLE"),
            ("COM10", "COM10"),
            ("main", "main_"),
            ("Main", "Main_"),
            ("main идея", "main идея"),
            // Ничего не осталось.
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
            assert_eq!(got, want, "название {title:?}");
            NoteId::new(got.clone()).unwrap_or_else(|e| panic!("{title:?} → {got:?}: {e}"));
        }
    }

    #[test]
    fn file_name_is_bounded() {
        let long = "дл".repeat(100);
        assert_eq!(file_name(&long).chars().count(), MAX_FILE_NAME);
        // Обрезка по букве, а не по байту, и хвост после обрезки снова чистится.
        let dotted = format!("{}. . . хвост", "я".repeat(MAX_FILE_NAME - 3));
        assert_eq!(file_name(&dotted), "я".repeat(MAX_FILE_NAME - 3));
        let wide = file_name(&"🙂".repeat(200));
        assert!(wide.len() <= 200 && wide.chars().all(|c| c == '🙂'), "в байтах — в пределах ФС, по целым буквам");
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

        assert_eq!(path("DNS", "Сеть").unwrap(), "Сеть/DNS", "имя из подпапки не мешает");
        assert_eq!(path("SSH: основы", "Сеть").unwrap(), "Сеть/SSH основы 2", "занято — с номером");
        assert_eq!(path("ssh ОСНОВЫ", "Сеть").unwrap(), "Сеть/ssh ОСНОВЫ 2", "регистр не важен");
        assert_eq!(path("протоколы", "Сеть").unwrap(), "Сеть/протоколы 2", "есть папка с тем же именем");
        assert_eq!(path("книга", "Сеть").unwrap(), "Сеть/книга 2", "и книга");
        assert_eq!(path("???", "Сеть").unwrap(), "Сеть/Без названия 2");
        assert_eq!(path("Вне папок", "").unwrap(), "Вне папок");
        assert_eq!(path("a/b", "").unwrap(), "a b", "/ в названии — не папка");
        assert_eq!(path("x", "Новая/Глубже").unwrap(), "Новая/Глубже/x", "папки нет — будет");

        for bad in ["", "   ", "\n\t"] {
            assert!(path(bad, "Сеть").unwrap_err().to_string().contains("пустое название"), "{bad:?}");
        }
        assert!(
            NewNote { title: None, ..new(NoteKind::Note) }.id_in(&vault, "").is_err(),
            "без названия — нечего брать"
        );
        assert!(path("x", "_служебная").is_err(), "папка — по правилам пути");
        assert!(path("x", "Сеть/").is_err());

        // Название в файле — как написано, со всеми знаками; индекс читает его обратно.
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
            assert_eq!(outline.title.as_deref(), Some(title), "{title:?} → {main}");
        }
        // Книга: название — в main.typ, имя папки — из названия.
        let book = NewNote { title: Some("Матан: кратные".into()), ..new(NoteKind::Book) };
        let id = book.id_in(&vault, "Курсы").unwrap();
        assert_eq!(book.create(&vault, &id).unwrap(), "Курсы/Матан кратные/main.typ");
        // Внутри книги — нельзя, как и с явным путём.
        let inside = titled("Глава").id_in(&vault, "Сеть/Книга").unwrap();
        assert!(titled("Глава").create(&vault, &inside).unwrap_err().to_string().contains("книга"));
    }
}
