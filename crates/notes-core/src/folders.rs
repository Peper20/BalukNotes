//! Папки хранилища: название (а позже и настройки) — файл [`FOLDER_FILE`]
//! в самой папке. Переносишь папку — название едет с ней.
//!
//! ```toml
//! title = "Сети и протоколы"
//! ```
//!
//! Файла нет — папка называется своим именем. У книги название — в её
//! `main.typ` (`book.with(title: …)`), `_folder.toml` в книге не читается.
//! Неизвестные ключи — ошибка (опечатка в `title` не пройдёт молча);
//! ошибки показывает `notes check`, а папка остаётся со своим именем.

use serde::Deserialize;

/// Файл папки. Имя на `_` — служебное: в дерево и в заметки не попадает.
pub const FOLDER_FILE: &str = "_folder.toml";

/// Содержимое [`FOLDER_FILE`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderMeta {
    /// Название для показа (пробелы схлопнуты); `None` — имя папки.
    pub title: Option<String>,
}

/// Разбор [`FOLDER_FILE`]; ошибка — текст для читателя.
pub fn parse_folder(text: &str) -> Result<FolderMeta, String> {
    let mut meta: FolderMeta = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    if let Some(title) = &meta.title {
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() {
            return Err("пустое название: title = \"…\" или уберите строку".into());
        }
        meta.title = Some(title);
    }
    Ok(meta)
}

/// Папка, в которой лежат заметки или книги.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folder {
    /// Из [`FOLDER_FILE`]; `None` — файла нет или в нём ошибка.
    pub title: Option<String>,
    /// Ошибка в [`FOLDER_FILE`] (для `notes check`).
    pub error: Option<String>,
}

/// Все папки на пути к заметке, от корня: `a/b/c` → `a`, `a/b`.
pub fn ancestors(id: &str) -> impl Iterator<Item = &str> {
    id.match_indices('/').map(|(i, _)| &id[..i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_errors() {
        assert_eq!(
            parse_folder("title = \"Сети  и\\nпротоколы \"").unwrap().title.as_deref(),
            Some("Сети и протоколы")
        );
        assert_eq!(parse_folder("").unwrap(), FolderMeta::default(), "пустой файл — без названия");
        assert_eq!(parse_folder("# только комментарий\n").unwrap().title, None);
        assert!(parse_folder("title = \"  \"").unwrap_err().contains("пустое название"));
        assert!(parse_folder("titel = \"x\"").unwrap_err().contains("titel"), "опечатка в ключе — ошибка");
        assert!(parse_folder("title = 5").is_err());
        assert!(parse_folder("title = \"x").is_err(), "незакрытая строка");
        // Любые знаки, запрещённые в именах файлов, в названии — можно.
        let odd = r#"title = "@#$@&$*%@#!.:/\\""#;
        assert_eq!(parse_folder(odd).unwrap().title.as_deref(), Some(r"@#$@&$*%@#!.:/\"));
    }

    #[test]
    fn ancestors_from_root() {
        assert_eq!(ancestors("a/b/c").collect::<Vec<_>>(), ["a", "a/b"]);
        assert_eq!(ancestors("a").count(), 0);
    }
}
