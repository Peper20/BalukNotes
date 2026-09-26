//! Кэш отрисовки на диске: после перезапуска приложения заметка, файлы
//! которой не менялись, не компилируется заново (у книги это секунды).
//!
//! Хранится сырая отрисовка (до обработки рисунков — она зависит от
//! настроек), список файлов заметки и их версия. Запись годна, если:
//! - её сделала **та же сборка приложения** (время и размер исполняемого
//!   файла): новая версия кода отрисовки не должна показывать старый HTML;
//! - версия файлов заметки совпадает с текущей (те же `stat`, что и в памяти).
//!
//! Кэш — не источник правды: любая ошибка чтения или записи — просто
//! промах (с предупреждением в журнал), удалить каталог можно в любой момент.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::diag::Diagnostic;
use crate::fsutil::write_atomic;
use crate::render::Rendered;
use crate::vault::NoteId;

#[derive(Debug)]
pub struct DiskCache {
    dir: PathBuf,
    /// Корень хранилища: у разных хранилищ (`--vault`) бывают заметки с
    /// одинаковыми путями, их кэши не должны смешиваться.
    vault: PathBuf,
    /// Метка сборки приложения.
    build: String,
}

/// Что лежит в файле кэша.
#[derive(Debug, Serialize, Deserialize)]
pub struct Stored {
    build: String,
    id: String,
    /// Версия файлов заметки на момент сборки.
    pub files: String,
    pub deps: Vec<PathBuf>,
    pub raw: Rendered,
    pub warnings: Vec<Diagnostic>,
}

impl DiskCache {
    pub fn new(dir: impl Into<PathBuf>, vault: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into(), vault: vault.into(), build: build_stamp() }
    }

    fn path(&self, id: &NoteId) -> PathBuf {
        // Имя файла — хэш хранилища и пути: в пути заметки бывают любые символы.
        let mut h = DefaultHasher::new();
        self.vault.hash(&mut h);
        id.as_str().hash(&mut h);
        self.dir.join(format!("{:016x}.json", h.finish()))
    }

    /// Запись для заметки, если она годна. `current` — текущая версия
    /// файлов по списку из записи.
    pub fn load(&self, id: &NoteId, current: impl FnOnce(&[PathBuf]) -> String) -> Option<Stored> {
        let path = self.path(id);
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::warn!("кэш {}: {e}", path.display());
                return None;
            }
        };
        let stored: Stored = match serde_json::from_slice(&data) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("кэш {}: {e} — пересоберу", path.display());
                return None;
            }
        };
        (stored.build == self.build && stored.id == id.as_str() && current(&stored.deps) == stored.files)
            .then_some(stored)
    }

    /// Годна ли запись для заметки — без загрузки самой отрисовки (для
    /// прогрева: собранное в прошлый запуск не собирать снова).
    pub fn is_fresh(&self, id: &NoteId, current: impl FnOnce(&[PathBuf]) -> String) -> bool {
        /// Заголовок записи; `raw` и `warnings` serde пропускает, не разбирая в структуры.
        #[derive(Deserialize)]
        struct Head {
            build: String,
            id: String,
            files: String,
            deps: Vec<PathBuf>,
        }
        let Ok(data) = fs::read(self.path(id)) else { return false };
        serde_json::from_slice::<Head>(&data)
            .is_ok_and(|h| h.build == self.build && h.id == id.as_str() && current(&h.deps) == h.files)
    }

    pub fn store(&self, id: &NoteId, files: &str, deps: &[PathBuf], raw: &Rendered, warnings: &[Diagnostic]) {
        #[derive(Serialize)]
        struct StoredRef<'a> {
            build: &'a str,
            id: &'a str,
            files: &'a str,
            deps: &'a [PathBuf],
            raw: &'a Rendered,
            warnings: &'a [Diagnostic],
        }
        let stored = StoredRef { build: &self.build, id: id.as_str(), files, deps, raw, warnings };
        let result =
            serde_json::to_vec(&stored).map_err(Into::into).and_then(|data| write_atomic(&self.path(id), &data));
        if let Err(e) = result {
            tracing::warn!("кэш для {id} не записан: {e}");
        }
    }
}

/// Версия приложения + время и размер исполняемого файла: пересборка —
/// другой код отрисовки, значит, другой кэш.
fn build_stamp() -> String {
    let exe = std::env::current_exe().ok().and_then(|p| fs::metadata(p).ok());
    let (len, modified) = exe.map_or((0, None), |m| (m.len(), m.modified().ok()));
    let secs = modified.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    format!("{}-{len}-{secs}", env!("CARGO_PKG_VERSION"))
}

/// Каталог кэша внутри каталога данных.
pub fn default_dir(data: &Path) -> PathBuf {
    data.join("cache").join("pages")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> Rendered {
        Rendered {
            title: Some("T".into()),
            styles: String::new(),
            body: "<p>x</p>".into(),
            headings: vec![],
            links: vec![],
            tags: vec![],
        }
    }

    #[test]
    fn round_trip_and_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path(), "/хранилище");
        let id = NoteId::new("Сеть/SSH").unwrap();
        assert!(cache.load(&id, |_| "v1".into()).is_none());

        cache.store(&id, "v1", &[PathBuf::from("/a.typ")], &rendered(), &[]);
        let got = cache.load(&id, |deps| {
            assert_eq!(deps, [PathBuf::from("/a.typ")]);
            "v1".into()
        });
        assert_eq!(got.unwrap().raw.body, "<p>x</p>");
        assert!(cache.load(&id, |_| "v2".into()).is_none(), "файлы изменились");
        assert!(cache.is_fresh(&id, |_| "v1".into()));
        assert!(!cache.is_fresh(&id, |_| "v2".into()));

        let other_build =
            DiskCache { build: "другая".into(), ..DiskCache::new(dir.path(), "/хранилище") };
        assert!(other_build.load(&id, |_| "v1".into()).is_none(), "другая сборка приложения");
        assert!(!other_build.is_fresh(&id, |_| "v1".into()));
        let other_vault = DiskCache::new(dir.path(), "/другое");
        assert!(other_vault.load(&id, |_| "v1".into()).is_none(), "другое хранилище");

        fs::write(cache.path(&id), "испорчено").unwrap();
        assert!(cache.load(&id, |_| "v1".into()).is_none());
        assert!(!cache.is_fresh(&id, |_| "v1".into()));
    }
}
