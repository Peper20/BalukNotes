//! Версии: из каких файлов собрана заметка и изменились ли они.
//!
//! Компиляция запоминает прочитанные файлы — [`Dep`] — и **отпечаток**
//! каждого в момент чтения ([`Versions::token`]: размер и время изменения,
//! дешёвый `stat`). Версия заметки — хэш пар «файл → отпечаток»
//! ([`combine`]). Годна ли сборка — сравнить с тем же хэшем по текущим
//! отпечаткам ([`Versions::current`]). Файл поменяли во время компиляции —
//! его отпечаток уже другой, версия не совпадёт, и заметка соберётся заново
//! (раньше версия считалась после компиляции, и правка терялась).
//!
//! Хэш — [`StableHasher`] (SipHash-1-3 с нулевым ключом, байты подаются
//! явно): одинаков в любой версии Rust и на любой машине — годится для кэша
//! на диске и позже для синхронизации.

use std::hash::Hasher as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::storage::{FileMeta, Storage, is_typ};

/// Стабильный хэш: алгоритм и порядок байтов закреплены.
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

    /// 16 шестнадцатеричных знаков.
    pub fn hex(&self) -> String {
        format!("{:016x}", self.finish())
    }
}

/// Файл, прочитанный компиляцией.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Dep {
    /// Файл хранилища (путь от корня, через `/`).
    Vault(String),
    /// Данные хранилища для заметок: `/_vault/<путь>` (граф). Отпечаток —
    /// всё хранилище (см. [`Versions::token`]).
    Data(String),
    /// Файл библиотеки оформления на диске (`--library` каталогом).
    /// Встроенная библиотека в зависимости не входит — она входит в метку
    /// кэша ([`crate::cache`]).
    Library(PathBuf),
}

/// Отпечаток файла на момент чтения.
pub type Token = u64;

/// Отпечатки файлов по хранилищу.
#[derive(Debug, Clone)]
pub struct Versions {
    storage: Arc<dyn Storage>,
}

impl Versions {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Текущий отпечаток файла. Пропавший файл — тоже отпечаток (другой).
    pub fn token(&self, dep: &Dep) -> Token {
        match dep {
            Dep::Vault(path) => meta_token(self.storage.stat(path).ok()),
            Dep::Data(_) => self.vault_token(),
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

    /// Версия по текущим отпечаткам — сравнивается с [`combine`] сборки.
    pub fn current(&self, deps: &[Dep]) -> String {
        let tokens: Vec<_> = deps.iter().map(|d| (d.clone(), self.token(d))).collect();
        combine(&tokens)
    }

    /// Отпечаток всего хранилища: пути, размеры и времена всех `.typ`
    /// (служебные каталоги `_…`, `.…` не входят).
    fn vault_token(&self) -> Token {
        let mut files: Vec<String> = self.storage.list().unwrap_or_default();
        files.retain(|p| is_typ(p));
        files.sort();
        let mut h = StableHasher::new();
        for path in &files {
            h.str(path).u64(meta_token(self.storage.stat(path).ok()));
        }
        h.finish()
    }
}

/// Отпечаток по сведениям о файле (`None` — файла нет).
fn meta_token(meta: Option<FileMeta>) -> Token {
    let mut h = StableHasher::new();
    match meta {
        None => h.str("нет"),
        Some(m) => {
            let nanos = m.modified.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_nanos());
            h.u64(m.len).bytes(&nanos.unwrap_or(0).to_le_bytes()).u64(u64::from(nanos.is_some()))
        }
    };
    h.finish()
}

/// Версия файлов: хэш пар «файл → отпечаток» (в порядке списка).
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
        // Закреплено: хэш меняться не должен (кэш на диске, синхронизация).
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
        assert_ne!(v2, versions.current(&deps), "пропавший файл тоже меняет версию");
    }

    #[test]
    fn file_changed_while_compiling_is_stale() {
        let mem = Arc::new(MemStorage::new());
        mem.write("a.typ", "1");
        let versions = Versions::new(mem.clone());
        let dep = Dep::Vault("a.typ".into());
        // Компиляция прочитала файл…
        let read = combine(&[(dep.clone(), versions.token(&dep))]);
        // …и пока она шла, файл поменяли.
        mem.write("a.typ", "2");
        assert_ne!(read, versions.current(&[dep]), "сборка устарела сразу");
    }

    #[test]
    fn vault_data_follows_every_note() {
        let mem = Arc::new(MemStorage::new());
        mem.write("A.typ", "");
        mem.write("_baluk/lib.typ", "");
        let versions = Versions::new(mem.clone());
        let deps = [Dep::Data("graph/x.json".into())];
        let v1 = versions.current(&deps);
        mem.write("_baluk/lib.typ", "служебное не в счёт");
        mem.write("картинка.png", "и не .typ");
        assert_eq!(v1, versions.current(&deps));
        mem.write("B.typ", "");
        assert_ne!(v1, versions.current(&deps));
    }
}
