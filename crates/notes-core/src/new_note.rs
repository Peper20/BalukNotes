//! Заготовка новой заметки или книги (`notes new`): шаблон библиотеки
//! (`note` или `book`) и шапка-комментарий о ходе работы — по ней следующая
//! сессия продолжает с того же места (правила — `docs/writing.md`,
//! «Процесс»).
//!
//! Существующее не перезаписывается; внутри книги заметок нет — там главы.
//! Заготовки собираются без ошибок и предупреждений (тесты ниже и шаг
//! `new-note` в `tools/check.sh`).

use std::fmt::Write;

use crate::vault::{BOOK_MAIN, NoteId, NoteKind, Vault};
use crate::{Error, Result};

/// Что заводить.
#[derive(Debug, Clone)]
pub struct NewNote {
    pub kind: NoteKind,
    /// Название; `None` — последний сегмент пути.
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
            out += "\n// Главы — файлы NN-тема.typ рядом, по строке на главу:\n// #include \"01-тема.typ\"\n";
        }
        out
    }

    /// Создать заготовку в хранилище. Результат — путь главного файла от
    /// корня хранилища.
    pub fn create(&self, vault: &Vault, id: &NoteId) -> Result<String> {
        let refuse = |reason: String| Err(Error::Create { id: id.to_string(), reason });
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

/// Текст в разметке Typst `[…]`: знаки разметки — через `\`.
fn markup(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '\n' | '\r' => out.push(' '),
            '[' | ']' | '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '=' | '~' | '/' | '-' | '+' => {
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
    }
}
