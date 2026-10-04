//! Versions: which files a note is built from and whether they changed.
//!
//! Compiling remembers the files it read ([`Dep`]) and a **fingerprint** of
//! each at the time of reading ([`Versions::token`]: size and modification
//! time, a cheap `stat`). The note version is a hash of "file -> fingerprint"
//! pairs ([`combine`]). Whether a build is still good: compare it with the same
//! hash over the current fingerprints ([`Versions::current`]). A file changed
//! during compiling already has another fingerprint, so the version does not
//! match and the note gets rebuilt (computing the version after compiling
//! would lose the edit).
//!
//! The hash is [`StableHasher`] (SipHash-1-3 with a zero key, bytes fed
//! explicitly): the same in any Rust version and on any machine, so it suits
//! the disk cache and later sync.

use std::hash::Hasher as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::storage::{FileMeta, Storage};
use crate::vault_data::VaultData;

/// A stable hash: the algorithm and the byte order are fixed.
#[derive(Debug, Default, Clone)]
pub struct StableHasher(siphasher::sip::SipHasher13);

impl StableHasher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(&mut self, data: &[u8]) -> &mut Self {
        self.0.write(&(data.len() as u64).to_le_bytes());
        self.0.write(data);
        self
    }

    pub fn str(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes())
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.write(&v.to_le_bytes());
        self
    }

    pub fn finish(&self) -> u64 {
        self.0.finish()
    }

    /// 16 hex digits.
    pub fn hex(&self) -> String {
        format!("{:016x}", self.finish())
    }
}

/// A file read by compiling.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Dep {
    /// A vault file (the path from the root, with `/`).
    Vault(String),
    /// Vault data for notes: `/_vault/<path>` (the graph). The provider gives
    /// the fingerprint ([`crate::vault_data`]).
    Data(String),
    /// A file of the design library on disk (`--library` as a directory). The
    /// embedded library is not a dependency: it is part of the cache label
    /// ([`crate::cache`]).
    Library(PathBuf),
}

/// The fingerprint of a file at the time of reading.
pub type Token = u64;

/// File fingerprints of the vault and its data.
#[derive(Debug, Clone)]
pub struct Versions {
    storage: Arc<dyn Storage>,
    data: VaultData,
}

impl Versions {
    /// Without vault data (`/_vault/...` has no such files).
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage, data: VaultData::default() }
    }

    /// With vault data, `/_vault/...`.
    #[must_use]
    pub fn with_data(mut self, data: VaultData) -> Self {
        self.data = data;
        self
    }

    pub fn storage(&self) -> &Arc<dyn Storage> {
        &self.storage
    }

    pub fn data(&self) -> &VaultData {
        &self.data
    }

    /// The current fingerprint of a file. A missing file has a fingerprint too (another one).
    pub fn token(&self, dep: &Dep) -> Token {
        match dep {
            Dep::Vault(path) => meta_token(self.storage.stat(path).ok()),
            Dep::Data(path) => self.data.token(path),
            Dep::Library(path) => {
                let meta = std::fs::metadata(path).ok().map(|m| FileMeta {
                    is_dir: m.is_dir(),
                    len: m.len(),
                    modified: m.modified().ok(),
                });
                meta_token(meta)
            }
        }
    }

    /// The version by current fingerprints, compared with the build's [`combine`].
    pub fn current(&self, deps: &[Dep]) -> String {
        let tokens: Vec<_> = deps.iter().map(|d| (d.clone(), self.token(d))).collect();
        combine(&tokens)
    }
}

/// A fingerprint from file info (`None` means there is no file).
fn meta_token(meta: Option<FileMeta>) -> Token {
    let mut h = StableHasher::new();
    match meta {
        None => h.str("missing"),
        Some(m) => {
            let nanos = m.modified.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_nanos());
            h.u64(m.len).bytes(&nanos.unwrap_or(0).to_le_bytes()).u64(u64::from(nanos.is_some()))
        }
    };
    h.finish()
}

/// The version of files: a hash of "file -> fingerprint" pairs (in list order).
pub fn combine(deps: &[(Dep, Token)]) -> String {
    let mut h = StableHasher::new();
    for (dep, token) in deps {
        match dep {
            Dep::Vault(p) => h.u64(0).str(p),
            Dep::Data(p) => h.u64(1).str(p),
            Dep::Library(p) => h.u64(2).str(&p.to_string_lossy()),
        };
        h.u64(*token);
    }
    h.hex()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemStorage;

    #[test]
    fn stable_hash_is_fixed() {
        // Fixed: the hash must not change (disk cache, sync).
        assert_eq!(StableHasher::new().str("baluk").hex(), "86cf48582e8b8aa0");
        assert_eq!(StableHasher::new().hex(), "d1fba762150c532c");
        assert_ne!(StableHasher::new().str("ab").str("c").hex(), StableHasher::new().str("a").str("bc").hex());
    }

    #[test]
    fn version_follows_files() {
        let mem = Arc::new(MemStorage::new());
        mem.write("a.typ", "1");
        let versions = Versions::new(mem.clone());
        let deps = [Dep::Vault("a.typ".into())];
        let v1 = versions.current(&deps);
        assert_eq!(v1, versions.current(&deps));
        mem.write("a.typ", "12");
        let v2 = versions.current(&deps);
        assert_ne!(v1, v2);
        mem.remove("a.typ");
        assert_ne!(v2, versions.current(&deps), "a missing file changes the version too");
    }

    #[test]
    fn file_changed_while_compiling_is_stale() {
        let mem = Arc::new(MemStorage::new());
        mem.write("a.typ", "1");
        let versions = Versions::new(mem.clone());
        let dep = Dep::Vault("a.typ".into());
        // Compiling read the file...
        let read = combine(&[(dep.clone(), versions.token(&dep))]);
        // ...and while it ran, the file changed.
        mem.write("a.typ", "2");
        assert_ne!(read, versions.current(&[dep]), "the build is stale at once");
    }

    #[test]
    fn data_token_comes_from_provider() {
        use crate::vault_data::FnProvider;
        use parking_lot::Mutex;
        let answer = Arc::new(Mutex::new("1"));
        let data = {
            let answer = answer.clone();
            VaultData::new().with("x", FnProvider(move |_: &str| Ok(answer.lock().as_bytes().to_vec())))
        };
        let versions = Versions::new(Arc::new(MemStorage::new())).with_data(data);
        let deps = [Dep::Data("x/a".into())];
        let v1 = versions.current(&deps);
        assert_eq!(v1, versions.current(&deps));
        *answer.lock() = "2";
        assert_ne!(v1, versions.current(&deps), "the provider's answer changed, so did the version");
    }
}
