//! Vault data for notes: virtual files `/_vault/<prefix>/...`.
//!
//! A note reads them as ordinary files (`read("/_vault/graph/....json")`),
//! the core computes the content on the fly. The registry [`VaultData`]
//! maps a prefix (the first path segment) to a provider ([`DataProvider`]);
//! the graph (`graph/<filter>.json`, [`crate::vault_graph`]) is the first of
//! them. A new kind of data (tags, backlinks of a note, the notes of a
//! folder) is a new provider and one registration line, without changes to
//! the compiler ([`crate::world`]).
//!
//! The version of a note includes **only the files it read**: each has its
//! own fingerprint ([`DataProvider::token`]). By default it is a hash of
//! the content: the data did not change - the note is fresh, even if the
//! vault was edited.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::version::{StableHasher, Token};

/// Provider of the files of one prefix.
pub trait DataProvider: Send + Sync {
    /// File content; `path` is the path after the prefix and `/`
    /// (`graph/x.json` -> `x.json`). The error is text for the reader.
    fn read(&self, path: &str) -> Result<Vec<u8>, String>;

    /// Fingerprint of the file for the note version: when it changes, a
    /// note that read the file is stale. By default, a hash of the content.
    fn token(&self, path: &str) -> Token {
        content_token(&self.read(path))
    }
}

/// Fingerprint of the content (or of the error).
pub fn content_token(content: &Result<Vec<u8>, String>) -> Token {
    let mut h = StableHasher::new();
    match content {
        Ok(bytes) => h.u64(0).bytes(bytes),
        Err(message) => h.u64(1).str(message),
    };
    h.finish()
}

/// Registry of providers: prefix -> provider. Cheap to clone.
#[derive(Clone, Default)]
pub struct VaultData {
    providers: Arc<BTreeMap<String, Arc<dyn DataProvider>>>,
}

impl fmt::Debug for VaultData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.providers.keys()).finish()
    }
}

impl VaultData {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the provider of a prefix (`graph` -> files `/_vault/graph/...`).
    #[must_use]
    pub fn with(mut self, prefix: &str, provider: impl DataProvider + 'static) -> Self {
        Arc::make_mut(&mut self.providers).insert(prefix.to_owned(), Arc::new(provider));
        self
    }

    /// The provider and the path inside it.
    fn route<'a>(&self, path: &'a str) -> Result<(&dyn DataProvider, &'a str), String> {
        let (prefix, rest) = path.split_once('/').unwrap_or((path, ""));
        if let Some(p) = self.providers.get(prefix) {
            return Ok((p.as_ref(), rest));
        }
        let known: Vec<String> = self.providers.keys().map(|k| format!("{k}/...")).collect();
        let known = if known.is_empty() { "none".to_owned() } else { known.join(", ") };
        Err(format!("no vault data \"{path}\" (there is only {known})"))
    }

    /// File `/_vault/<path>` (the path without `/_vault/`).
    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let (provider, rest) = self.route(path)?;
        provider.read(rest)
    }

    /// Fingerprint of the file `/_vault/<path>`. No provider: the fingerprint of the error.
    pub fn token(&self, path: &str) -> Token {
        match self.route(path) {
            Ok((provider, rest)) => provider.token(rest),
            Err(message) => content_token(&Err(message)),
        }
    }
}

/// Provider from a closure (fingerprint by content).
pub struct FnProvider<F>(pub F);

impl<F> fmt::Debug for FnProvider<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FnProvider")
    }
}

impl<F> DataProvider for FnProvider<F>
where
    F: Fn(&str) -> Result<Vec<u8>, String> + Send + Sync,
{
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        (self.0)(path)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// Stub: returns the path and the counter; its own fingerprint.
    struct Counter(Arc<AtomicU64>);

    impl DataProvider for Counter {
        fn read(&self, path: &str) -> Result<Vec<u8>, String> {
            Ok(format!("{path}:{}", self.0.load(Ordering::SeqCst)).into_bytes())
        }

        fn token(&self, _path: &str) -> Token {
            self.0.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn routes_by_prefix() {
        let n = Arc::new(AtomicU64::new(7));
        let data = VaultData::new()
            .with("count", Counter(n.clone()))
            .with("echo", FnProvider(|p: &str| Ok(p.as_bytes().to_vec())));
        assert_eq!(data.read("count/a/b.json").unwrap(), b"a/b.json:7");
        assert_eq!(data.read("echo/x").unwrap(), b"x");
        assert_eq!(data.token("count/a"), 7);
        n.store(8, Ordering::SeqCst);
        assert_eq!(data.token("count/a"), 8, "the provider's own fingerprint");

        let err = data.read("tags/all.json").unwrap_err();
        assert!(err.contains("count/..., echo/..."), "{err}");
        assert_eq!(data.token("tags/x"), data.token("tags/x"), "no provider: a constant fingerprint");
    }

    #[test]
    fn default_token_follows_content() {
        let data = VaultData::new().with("echo", FnProvider(|p: &str| Ok(p.as_bytes().to_vec())));
        assert_eq!(data.token("echo/a"), data.token("echo/a"));
        assert_ne!(data.token("echo/a"), data.token("echo/b"));
        assert_ne!(content_token(&Ok(vec![])), content_token(&Err(String::new())));
    }
}
