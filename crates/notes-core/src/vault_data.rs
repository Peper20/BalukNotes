//! Данные хранилища для заметок: виртуальные файлы `/_vault/<префикс>/…`.
//!
//! Заметка читает их как обычные файлы (`read("/_vault/graph/….json")`), а
//! содержимое на лету считает ядро. Реестр [`VaultData`] отображает префикс
//! (первый сегмент пути) на поставщика ([`DataProvider`]); граф
//! (`graph/<фильтр>.json`, [`crate::vault_graph`]) — первый из них. Новый
//! вид данных (теги, обратные ссылки заметки, список заметок папки) — новый
//! поставщик и одна строка регистрации, без правок компилятора
//! ([`crate::world`]).
//!
//! Версия заметки учитывает **только прочитанные ею файлы**: у каждого —
//! свой отпечаток ([`DataProvider::token`]). По умолчанию это хэш
//! содержимого: данные не изменились — заметка свежая, даже если хранилище
//! правили.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::version::{StableHasher, Token};

/// Поставщик файлов одного префикса.
pub trait DataProvider: Send + Sync {
    /// Содержимое файла; `path` — путь после префикса и `/`
    /// (`graph/x.json` → `x.json`). Ошибка — текст для читателя.
    fn read(&self, path: &str) -> Result<Vec<u8>, String>;

    /// Отпечаток файла для версии заметки: сменился — заметка, читавшая
    /// файл, устарела. По умолчанию — хэш содержимого.
    fn token(&self, path: &str) -> Token {
        content_token(&self.read(path))
    }
}

/// Отпечаток по содержимому (или ошибке).
pub fn content_token(content: &Result<Vec<u8>, String>) -> Token {
    let mut h = StableHasher::new();
    match content {
        Ok(bytes) => h.u64(0).bytes(bytes),
        Err(message) => h.u64(1).str(message),
    };
    h.finish()
}

/// Реестр поставщиков: префикс → поставщик. Клонируется дёшево.
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
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавить поставщика префикса (`graph` → файлы `/_vault/graph/…`).
    #[must_use]
    pub fn with(mut self, prefix: &str, provider: impl DataProvider + 'static) -> Self {
        Arc::make_mut(&mut self.providers).insert(prefix.to_owned(), Arc::new(provider));
        self
    }

    /// Поставщик и путь внутри него.
    fn route<'a>(&self, path: &'a str) -> Result<(&dyn DataProvider, &'a str), String> {
        let (prefix, rest) = path.split_once('/').unwrap_or((path, ""));
        if let Some(p) = self.providers.get(prefix) {
            return Ok((p.as_ref(), rest));
        }
        let known: Vec<String> = self.providers.keys().map(|k| format!("{k}/…")).collect();
        let known = if known.is_empty() { "никаких".to_owned() } else { known.join(", ") };
        Err(format!("нет данных хранилища «{path}» (есть только {known})"))
    }

    /// Файл `/_vault/<path>` (путь без `/_vault/`).
    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let (provider, rest) = self.route(path)?;
        provider.read(rest)
    }

    /// Отпечаток файла `/_vault/<path>`. Нет поставщика — отпечаток ошибки.
    pub fn token(&self, path: &str) -> Token {
        match self.route(path) {
            Ok((provider, rest)) => provider.token(rest),
            Err(message) => content_token(&Err(message)),
        }
    }
}

/// Поставщик из замыкания (отпечаток — по содержимому).
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

    /// Заглушка: отдаёт путь и счётчик; отпечаток — свой.
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
        assert_eq!(data.token("count/a"), 8, "свой отпечаток поставщика");

        let err = data.read("tags/all.json").unwrap_err();
        assert!(err.contains("count/…, echo/…"), "{err}");
        assert_eq!(data.token("tags/x"), data.token("tags/x"), "нет поставщика — отпечаток постоянный");
    }

    #[test]
    fn default_token_follows_content() {
        let data = VaultData::new().with("echo", FnProvider(|p: &str| Ok(p.as_bytes().to_vec())));
        assert_eq!(data.token("echo/a"), data.token("echo/a"));
        assert_ne!(data.token("echo/a"), data.token("echo/b"));
        assert_ne!(content_token(&Ok(vec![])), content_token(&Err(String::new())));
    }
}
