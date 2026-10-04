//! Vault folders: the title (and later settings) lives in the file
//! [`FOLDER_FILE`] in the folder itself. Move the folder and the title moves with it.
//!
//! ```toml
//! title = "Networks and protocols"
//! ```
//!
//! Without the file the folder is called by its name. A book has its title in
//! its `main.typ` (`book.with(title: ...)`); `_folder.toml` in a book is not
//! read. Unknown keys are an error (a typo in `title` does not pass silently);
//! `notes check` shows errors, and the folder keeps its own name.

use serde::Deserialize;

/// The folder file. A name starting with `_` is internal: it is not in the tree or among notes.
pub const FOLDER_FILE: &str = "_folder.toml";

/// The contents of [`FOLDER_FILE`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderMeta {
    /// The display title (whitespace collapsed); `None` means the folder name.
    pub title: Option<String>,
}

/// Parses [`FOLDER_FILE`]; the error is a text for the reader.
pub fn parse_folder(text: &str) -> Result<FolderMeta, String> {
    let mut meta: FolderMeta = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    if let Some(title) = &meta.title {
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() {
            return Err("empty title: title = \"...\" or remove the line".into());
        }
        meta.title = Some(title);
    }
    Ok(meta)
}

/// A folder that holds notes or books.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folder {
    /// From [`FOLDER_FILE`]; `None` if there is no file or it has an error.
    pub title: Option<String>,
    /// An error in [`FOLDER_FILE`] (for `notes check`).
    pub error: Option<String>,
}

/// Every folder on the way to a note, from the root: `a/b/c` -> `a`, `a/b`.
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
        assert_eq!(parse_folder("").unwrap(), FolderMeta::default(), "an empty file has no title");
        assert_eq!(parse_folder("# a comment only\n").unwrap().title, None);
        assert!(parse_folder("title = \"  \"").unwrap_err().contains("empty title"));
        assert!(parse_folder("titel = \"x\"").unwrap_err().contains("titel"), "a typo in a key is an error");
        assert!(parse_folder("title = 5").is_err());
        assert!(parse_folder("title = \"x").is_err(), "an unclosed string");
        // Any characters forbidden in file names are allowed in a title.
        let odd = r#"title = "@#$@&$*%@#!.:/\\""#;
        assert_eq!(parse_folder(odd).unwrap().title.as_deref(), Some(r"@#$@&$*%@#!.:/\"));
    }

    #[test]
    fn ancestors_from_root() {
        assert_eq!(ancestors("a/b/c").collect::<Vec<_>>(), ["a", "a/b"]);
        assert_eq!(ancestors("a").count(), 0);
    }
}
