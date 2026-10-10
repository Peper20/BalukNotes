//! Vault names (architecture §9): the name is also the name of the vault
//! folder, so a path derived from a checked name is always safe.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The longest vault name, in characters.
pub const MAX_NAME: usize = 64;

/// An internal name (the `_baluk` library, `.git`): never a note or a vault.
#[must_use]
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('_') || name.starts_with('.')
}

/// A vault name that does not pass the checks of [`VaultName::new`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid vault name \"{name}\": {reason}")]
pub struct InvalidName {
    pub name: String,
    pub reason: &'static str,
}

/// A vault name, which is also the name of its folder. It is checked on
/// creation, so a path in the vaults directory can always be safely derived from it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(try_from = "String", into = "String")]
pub struct VaultName(String);

impl VaultName {
    /// Checks the name and wraps it.
    ///
    /// # Errors
    /// [`InvalidName`] with the reason when the name is empty, padded with
    /// spaces, too long, internal, or has characters a folder name cannot have.
    pub fn new(name: impl Into<String>) -> Result<Self, InvalidName> {
        let name = name.into();
        let invalid = |reason| Err(InvalidName { name: name.clone(), reason });
        if name.trim().is_empty() {
            return invalid("empty name");
        }
        if name.trim() != name {
            return invalid("spaces at the start or the end");
        }
        if name.chars().count() > MAX_NAME {
            return invalid("longer than 64 characters");
        }
        if is_hidden(&name) {
            return invalid("names starting with _ and . are internal");
        }
        // A folder name on any OS (Windows forbids these characters and a trailing dot).
        if name.chars().any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c)) {
            return invalid(r#"no characters / \ : * ? " < > |"#);
        }
        if name.ends_with('.') {
            return invalid("a dot at the end");
        }
        Ok(Self(name))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VaultName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for VaultName {
    type Error = InvalidName;

    fn try_from(name: String) -> Result<Self, InvalidName> {
        Self::new(name)
    }
}

impl From<VaultName> for String {
    fn from(name: VaultName) -> Self {
        name.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> VaultName {
        VaultName::new(s).unwrap()
    }

    fn reason(s: &str) -> &'static str {
        VaultName::new(s).unwrap_err().reason
    }

    #[test]
    fn names() {
        for ok in ["Заметки", "Учёба 2026", "C++", "a.b", "x-y_z"] {
            assert_eq!(name(ok).as_str(), ok);
            assert_eq!(name(ok).to_string(), ok);
        }
        for bad in ["", "  ", " a", "a ", ".git", "_baluk", "a/b", r"a\b", "a:b", "a?", "a.", "a\nb"] {
            assert!(VaultName::new(bad).is_err(), "{bad:?}");
        }
        assert!(VaultName::new("я".repeat(MAX_NAME)).is_ok());
        assert!(VaultName::new("я".repeat(MAX_NAME + 1)).is_err());
        // From JSON with the same check.
        assert!(serde_json::from_str::<VaultName>(r#""a/b""#).is_err());
        assert_eq!(serde_json::from_str::<VaultName>(r#""Учёба""#).unwrap(), name("Учёба"));
        assert_eq!(serde_json::to_string(&name("Учёба")).unwrap(), r#""Учёба""#);
    }

    #[test]
    fn reasons() {
        assert_eq!(reason(""), "empty name");
        assert_eq!(reason(" a"), "spaces at the start or the end");
        assert_eq!(reason(&"я".repeat(MAX_NAME + 1)), "longer than 64 characters");
        assert_eq!(reason("_baluk"), "names starting with _ and . are internal");
        assert_eq!(reason("a/b"), r#"no characters / \ : * ? " < > |"#);
        assert_eq!(reason("a."), "a dot at the end");
        let err = VaultName::new("a/b").unwrap_err();
        assert_eq!(err.to_string(), r#"invalid vault name "a/b": no characters / \ : * ? " < > |"#);
        assert!(is_hidden(".git") && is_hidden("_x") && !is_hidden("a_"));
    }
}
