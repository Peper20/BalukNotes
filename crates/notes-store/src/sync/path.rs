//! Which paths are synced. One function decides for everything: the server
//! refuses other paths, the device tree does not list them, the engine skips
//! them in what the server sends.
//!
//! A path is relative to the vault root, `/`-separated, in UTF-8, at most
//! [`MAX_PATH_BYTES`] bytes, with no empty, `.` or `..` segments, no
//! backslash and no control characters. Not synced:
//!
//! - anything inside `.git/` or `.trash/` (at any depth);
//! - files ending with `.tmp` (the temporary file of an atomic write);
//! - the OS junk `.DS_Store` and `Thumbs.db`;
//! - everything in `.baluk/` except `.baluk/settings.json`.
//!
//! Symlinks are not paths: the walkers skip them.

use super::error::{Error, Result};

/// The longest path in bytes.
pub const MAX_PATH_BYTES: usize = 1024;
/// The longest path segment in bytes.
pub const MAX_SEGMENT_BYTES: usize = 255;

/// The only synced file of the `.baluk` folder (vault settings).
const BALUK_SETTINGS: &str = ".baluk/settings.json";

/// Folders that are never synced, with everything in them.
pub(crate) fn is_skipped_dir(name: &str) -> bool {
    matches!(name, ".git" | ".trash")
}

/// Why the path is not synced, or `Ok` if it is.
fn problem(path: &str) -> std::result::Result<(), &'static str> {
    if path.is_empty() {
        return Err("empty");
    }
    if path.len() > MAX_PATH_BYTES {
        return Err("too long");
    }
    if path.starts_with('/') {
        return Err("absolute");
    }
    if path.chars().any(|c| c == '\\' || c.is_control()) {
        return Err("backslash or control character");
    }
    let mut last = "";
    for segment in path.split('/') {
        match segment {
            "" => return Err("empty segment"),
            "." | ".." => return Err("dot segment"),
            _ if segment.len() > MAX_SEGMENT_BYTES => return Err("segment too long"),
            _ if is_skipped_dir(segment) => return Err("reserved folder"),
            _ => {}
        }
        last = segment;
    }
    #[expect(clippy::case_sensitive_file_extension_comparisons, reason = "the exact suffix of write_atomic")]
    let is_tmp = last.ends_with(".tmp");
    if is_tmp {
        return Err("temporary file");
    }
    if matches!(last, ".DS_Store" | "Thumbs.db") {
        return Err("system junk file");
    }
    if path.split('/').next() == Some(".baluk") && path != BALUK_SETTINGS {
        return Err("only .baluk/settings.json is synced");
    }
    Ok(())
}

/// Is the path valid and synced.
pub fn is_synced(path: &str) -> bool {
    problem(path).is_ok()
}

/// As [`is_synced`], with the reason as an error.
pub fn check(path: &str) -> Result<()> {
    problem(path).map_err(|reason| Error::InvalidPath { path: path.to_owned(), reason })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_paths() {
        for path in [
            "note.typ",
            "a/b/c.typ",
            "_folder.toml",
            "img/photo.png",
            ".baluk/settings.json",
            "dir/.hidden",
            "a.tmp.typ",
            "dir/.baluk/x",
        ] {
            assert!(is_synced(path), "{path}");
        }
    }

    #[test]
    fn bad_syntax() {
        let long_segment = "a".repeat(MAX_SEGMENT_BYTES + 1);
        let long_path = format!("{}/x", "a/".repeat(MAX_PATH_BYTES / 2));
        for path in [
            "",
            "/abs",
            "a//b",
            "a/",
            "./a",
            "a/./b",
            "../a",
            "a/../b",
            "a\\b",
            "a\nb",
            "a\0b",
            long_segment.as_str(),
            long_path.as_str(),
        ] {
            assert!(!is_synced(path), "{path:?}");
        }
        assert!(matches!(check("a/../b"), Err(Error::InvalidPath { .. })));
        assert!(check("a/b").is_ok());
    }

    #[test]
    fn not_synced_names() {
        for path in [
            ".git/config",
            "sub/.git/HEAD",
            ".trash/old.typ",
            "x/.trash/y",
            "note.typ.tmp",
            "a/b.tmp",
            ".DS_Store",
            "a/Thumbs.db",
            ".baluk/cache.json",
            ".baluk/sub/settings.json",
            ".baluk",
        ] {
            assert!(!is_synced(path), "{path}");
        }
    }
}
