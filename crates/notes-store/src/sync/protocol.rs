//! The protocol types shared by the server and the device.

use std::fmt::{self, Write as _};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::error::Error;

/// One file of a vault as the server knows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    /// SHA-256 hex of the content; `None` - the file is deleted (a tombstone).
    pub hash: Option<String>,
    /// The size in bytes; 0 for a tombstone.
    pub size: u64,
    /// The vault's change number at which this version appeared.
    pub seq: u64,
}

/// The answer to "what changed after N".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changes {
    /// The vault's current `seq`: ask `changes(seq)` next time.
    pub seq: u64,
    /// The files with `entry.seq > after`, ordered by `seq`. One entry per
    /// path: the latest version.
    pub entries: Vec<Entry>,
}

/// What the writer believes is on the server for the path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Base {
    /// Overwrite or delete exactly this version (SHA-256 hex).
    Hash(String),
    /// The file must not exist (never existed, or deleted).
    Absent,
    /// No check: the writer wins.
    Any,
}

impl fmt::Display for Base {
    /// The text form for an HTTP header: `any`, `absent` or the hex hash.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hash(hash) => f.write_str(hash),
            Self::Absent => f.write_str("absent"),
            Self::Any => f.write_str("any"),
        }
    }
}

impl FromStr for Base {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        match text {
            "any" => Ok(Self::Any),
            "absent" => Ok(Self::Absent),
            hash if is_hash(hash) => Ok(Self::Hash(hash.to_owned())),
            _ => Err(Error::InvalidBase(text.to_owned())),
        }
    }
}

/// A write was refused: the server has another version than the writer's
/// [`Base`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    /// What the server has now: a live entry, a tombstone, or `None` if it
    /// never knew the path.
    pub current: Option<Entry>,
}

/// The result of a write or a delete that reached the server: the new entry
/// or a [`Conflict`].
pub type Outcome = Result<Entry, Conflict>;

/// Is the text a SHA-256 hex hash as [`hash_hex`] writes it.
pub fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// SHA-256 hex (lowercase) of the content.
pub fn hash_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(64);
    for byte in digest {
        // Writing to a String does not fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes() {
        assert_eq!(hash_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert!(is_hash(&hash_hex(b"x")));
        assert!(!is_hash("abc"));
        assert!(!is_hash(&"A".repeat(64)));
    }

    #[test]
    fn base_text_form() {
        let hash = hash_hex(b"note");
        for base in [Base::Any, Base::Absent, Base::Hash(hash.clone())] {
            assert_eq!(base.to_string().parse::<Base>().unwrap(), base);
        }
        assert_eq!(Base::Any.to_string(), "any");
        assert_eq!(Base::Absent.to_string(), "absent");
        assert_eq!(Base::Hash(hash.clone()).to_string(), hash);
        for bad in ["", "ANY", "abc", "0".repeat(63).as_str()] {
            assert!(matches!(bad.parse::<Base>(), Err(Error::InvalidBase(_))), "{bad}");
        }
    }

    #[test]
    fn json_shapes() {
        let entry = Entry { path: "a.typ".into(), hash: None, size: 0, seq: 3 };
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(serde_json::from_str::<Entry>(&json).unwrap(), entry);
        let conflict = Conflict { current: Some(entry) };
        let json = serde_json::to_string(&conflict).unwrap();
        assert_eq!(serde_json::from_str::<Conflict>(&json).unwrap(), conflict);
    }
}
