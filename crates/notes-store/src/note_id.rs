//! Note paths (architecture §2): the rules of `NoteId`, shared by the core and
//! the parts that handle note paths without Typst.

use std::fmt;
use std::path::Path;

use serde::Serialize;

/// A note path that does not pass the checks of [`NoteId::new`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid note path \"{id}\": {reason}")]
pub struct InvalidId {
    pub id: String,
    pub reason: &'static str,
}

/// A note path from the vault root, without `.typ`, separated by `/`: `Network/SSH`.
///
/// It is checked on creation, so a path inside the vault can always be safely
/// derived from it (no `..`, absolute paths or internal directories).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct NoteId(String);

impl NoteId {
    pub fn new(id: impl Into<String>) -> Result<Self, InvalidId> {
        let id = id.into();
        let invalid = |reason| Err(InvalidId { id: id.clone(), reason });
        if id.is_empty() {
            return invalid("empty path");
        }
        if id.starts_with('/') || id.ends_with('/') {
            return invalid("no / at the start or the end");
        }
        if id.contains('\\') {
            return invalid("the separator is /");
        }
        for segment in id.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." {
                return invalid("empty segments, . and .. are not allowed");
            }
            if segment.starts_with('_') || segment.starts_with('.') {
                return invalid("names starting with _ and . are internal");
            }
        }
        if Path::new(&id).extension().is_some_and(|e| e == "typ") {
            return invalid("no .typ in the path");
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last segment, the display name: `Network/SSH` -> `SSH`.
    pub fn name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// The note folder (`Network` for `Network/SSH`), empty for the root.
    pub fn parent(&self) -> &str {
        self.0.rsplit_once('/').map_or("", |(p, _)| p)
    }
}

impl fmt::Display for NoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_id_validation() {
        assert!(NoteId::new("Сеть/SSH").is_ok());
        assert!(NoteId::new("SSH").is_ok());
        for bad in ["", "/SSH", "Сеть/", "a//b", "../x", "a/./b", "_baluk/lib", ".git/x", "a\\b", "SSH.typ"] {
            assert!(NoteId::new(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn note_id_parts() {
        let id = NoteId::new("Сеть/SSL/acme").unwrap();
        assert_eq!(id.name(), "acme");
        assert_eq!(id.parent(), "Сеть/SSL");
        assert_eq!(NoteId::new("SSH").unwrap().parent(), "");
    }
}
