//! What the device synced last time: the base of the next round.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{Error, Result};
use crate::fsutil::write_atomic;

/// A file as it was when synced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Synced {
    /// SHA-256 hex of the content both sides had.
    pub hash: String,
    /// The local size and modification time (nanoseconds since the epoch) at
    /// that moment: a file with the same pair is not read again.
    pub size: u64,
    pub mtime: i64,
}

/// The base of a device's vault: what both sides agreed on at the end of the
/// last round (per file, as each file is done).
///
/// A state opened with [`State::open`] remembers its file, and [`State::save`]
/// writes it there atomically; the engine saves at the end of a round and when
/// a round fails midway. A [`State::default`] lives in memory only.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    /// The server's `seq` up to which every change was processed.
    pub remote_seq: u64,
    pub files: BTreeMap<String, Synced>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl State {
    /// Loads the state from `path`; no file - an empty state (the first
    /// sync). The path is remembered for [`State::save`].
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mut state = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes)
                .map_err(|e| Error::Corrupt { path: path.display().to_string(), reason: e.to_string() })?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };
        state.path = Some(path);
        Ok(state)
    }

    /// Saves atomically; does nothing for a state without a file.
    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let json = serde_json::to_vec(self).map_err(|e| Error::Other(e.to_string()))?;
        write_atomic(path, &json)?;
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync/vault.json");
        let mut state = State::open(&path).unwrap();
        assert_eq!((state.remote_seq, state.files.len()), (0, 0));
        state.remote_seq = 7;
        state.files.insert("a.typ".into(), Synced { hash: "h".into(), size: 1, mtime: -5 });
        state.save().unwrap();
        let loaded = State::open(&path).unwrap();
        assert_eq!(loaded.remote_seq, 7);
        assert_eq!(loaded.files, state.files);
        assert_eq!(loaded.path(), Some(path.as_path()));
        // In memory: nothing to write.
        State::default().save().unwrap();
    }

    #[test]
    fn broken_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(State::open(&path), Err(Error::Corrupt { .. })));
    }
}
