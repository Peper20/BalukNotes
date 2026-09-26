//! Кэш сборок на диске: после перезапуска заметка, файлы которой не
//! менялись, не компилируется заново (у книги это секунды).
//!
//! На заметку — два файла в каталоге хранилища (`<кэш>/<хэш хранилища>/`):
//! - `<хэш пути>.json` — **запись** ([`Record`]): версия и список файлов,
//!   ошибки и предупреждения, время сборки (порядок прогрева), метка
//!   отрисовки; маленький — его читают проверка «собрано ли» и прогрев;
//! - `<хэш пути>.raw.json` — сырая отрисовка (до обработки рисунков: она
//!   зависит от настроек). Метка в записи и в файле отрисовки совпадает —
//!   сбой между записью двух файлов даёт промах, а не чужую страницу.
//!
//! Ошибки сборки тоже записываются (с прежней удачной отрисовкой, если она
//! была): заметка с ошибкой не собирается заново в каждом запуске.
//!
//! Запись годна, если её сделал **тот же код отрисовки** — метка
//! [`stamp`]: версия формата, хэш исходников отрисовки (`build.rs`),
//! встроенная библиотека и набор шрифтов, — и версия файлов заметки
//! совпадает с текущей. Пересборка сервера или клиента кэш не сбрасывает.
//!
//! Кэш — не источник правды: любая ошибка чтения или записи — просто
//! промах (с предупреждением в журнал), удалить каталог можно в любой
//! момент. Чистка — [`DiskCache::prune`] (удалённые заметки, старые чужие
//! записи, предел размера).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::diag::Diagnostic;
use crate::fsutil::write_atomic;
use crate::render::{LinkRef, Rendered};
use crate::vault::NoteId;
use crate::version::{Dep, StableHasher};

/// Версия формата записей: менять при любом изменении [`Record`] или
/// [`Rendered`].
pub const FORMAT: u32 = 3;

/// Предел размера кэша на диске (все хранилища вместе).
pub const DISK_LIMIT: u64 = 512 << 20;

/// Чужие записи (другое хранилище, другая сборка приложения), не
/// обновлявшиеся столько, удаляются при чистке.
pub const FOREIGN_TTL: Duration = Duration::from_secs(14 * 24 * 3600);

/// Что известно о сборке заметки.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Версия файлов заметки на момент чтения ([`crate::version::combine`]).
    pub files: String,
    pub deps: Vec<Dep>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// Сколько длилась сборка, мс.
    pub build_ms: u64,
    /// Метка сырой отрисовки: `None` — отрисовки нет (ошибка, и удачной
    /// сборки не было).
    pub raw: Option<String>,
    /// Ссылки из последней удачной отрисовки (`Rendered.links`) — и
    /// вычисляемые: ими индекс ссылок дополняет разбор исходников.
    #[serde(default)]
    pub links: Vec<LinkRef>,
}

/// Файл записи.
#[derive(Serialize, Deserialize)]
struct RecordFile {
    stamp: String,
    id: String,
    #[serde(flatten)]
    record: Record,
}

/// Файл отрисовки.
#[derive(Serialize, Deserialize)]
struct RawFile {
    tag: String,
    raw: Rendered,
}

#[derive(Debug)]
pub struct DiskCache {
    /// Корень кэша (все хранилища).
    root: PathBuf,
    /// Каталог этого хранилища.
    dir: PathBuf,
    /// Метка кода отрисовки.
    stamp: String,
}

/// Итог чистки.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pruned {
    /// Сколько файлов удалено.
    pub removed: usize,
    /// Сколько байт осталось.
    pub bytes: u64,
}

impl DiskCache {
    /// `vault` — где хранилище ([`crate::storage::Storage::location`]):
    /// у разных хранилищ бывают заметки с одинаковыми путями.
    pub fn new(root: impl Into<PathBuf>, vault: &str, stamp: String) -> Self {
        let root = root.into();
        let dir = root.join(StableHasher::new().str(vault).hex());
        Self { root, dir, stamp }
    }

    fn base(&self, id: &NoteId) -> PathBuf {
        // Имя файла — хэш пути: в пути заметки бывают любые символы.
        self.dir.join(StableHasher::new().str(id.as_str()).hex())
    }

    fn record_path(&self, id: &NoteId) -> PathBuf {
        self.base(id).with_extension("json")
    }

    fn raw_path(&self, id: &NoteId) -> PathBuf {
        self.base(id).with_extension("raw.json")
    }

    /// Запись заметки, если её сделал тот же код отрисовки. Годна ли она
    /// для текущих файлов — решает вызывающий (по `files` и `deps`).
    pub fn record(&self, id: &NoteId) -> Option<Record> {
        let file: RecordFile = read_json(&self.record_path(id))?;
        (file.stamp == self.stamp && file.id == id.as_str()).then_some(file.record)
    }

    /// Сырая отрисовка к записи (метки должны совпасть).
    pub fn raw(&self, id: &NoteId, record: &Record) -> Option<Rendered> {
        let tag = record.raw.as_ref()?;
        let file: RawFile = read_json(&self.raw_path(id))?;
        (&file.tag == tag).then_some(file.raw)
    }

    /// Записать сборку. `raw` — новая отрисовка (её метка — `record.raw`);
    /// `None` — отрисовка не менялась (на диске лежит та, что в метке).
    pub fn store(&self, id: &NoteId, record: &Record, raw: Option<&Rendered>) {
        #[derive(Serialize)]
        struct RawRef<'a> {
            tag: &'a str,
            raw: &'a Rendered,
        }
        #[derive(Serialize)]
        struct RecordRef<'a> {
            stamp: &'a str,
            id: &'a str,
            #[serde(flatten)]
            record: &'a Record,
        }
        let result = (|| {
            // Сначала отрисовка, потом запись: сбой между ними — промах.
            if let (Some(raw), Some(tag)) = (raw, &record.raw) {
                write_atomic(&self.raw_path(id), &serde_json::to_vec(&RawRef { tag, raw })?)?;
            }
            let file = RecordRef { stamp: &self.stamp, id: id.as_str(), record };
            write_atomic(&self.record_path(id), &serde_json::to_vec(&file)?)
        })();
        if let Err(e) = result {
            tracing::warn!("кэш для {id} не записан: {e}");
        }
    }

    /// Чистка: записи удалённых заметок (`alive` — есть ли заметка),
    /// сироты и мусор, чужие записи старше [`FOREIGN_TTL`], файлы старого
    /// формата; затем, если кэш больше `limit`, — самые давние записи.
    pub fn prune(&self, alive: &dyn Fn(&str) -> bool, limit: u64) -> Pruned {
        let now = SystemTime::now();
        let old = |mtime: SystemTime| now.duration_since(mtime).is_ok_and(|age| age > FOREIGN_TTL);
        let mut pruned = Pruned::default();
        let remove = |path: &Path, pruned: &mut Pruned| {
            let result = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
            match result {
                Ok(()) => pruned.removed += 1,
                Err(e) => tracing::warn!("кэш {}: не удалён: {e}", path.display()),
            }
        };
        // Группы, которые можно удалить целиком: (время, размер, файлы).
        let mut groups: Vec<(SystemTime, u64, Vec<PathBuf>)> = Vec::new();
        for item in list(&self.root) {
            if item.path == self.dir {
                continue;
            }
            if item.is_dir {
                let files = list(&item.path);
                let newest = files.iter().map(|f| f.mtime).max().unwrap_or(item.mtime);
                if old(newest) {
                    remove(&item.path, &mut pruned);
                } else {
                    groups.push((newest, files.iter().map(|f| f.len).sum(), vec![item.path]));
                }
            } else {
                remove(&item.path, &mut pruned); // старый формат: файлы прямо в корне
            }
        }
        // Своё хранилище: запись + отрисовка по имени.
        let mut own: HashMap<String, (Option<Item>, Option<Item>)> = HashMap::new();
        for item in list(&self.dir) {
            let name = item.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(base) = name.strip_suffix(".raw.json") {
                own.entry(base.to_owned()).or_default().1 = Some(item);
            } else if let Some(base) = name.strip_suffix(".json") {
                own.entry(base.to_owned()).or_default().0 = Some(item);
            } else {
                remove(&item.path, &mut pruned); // `.tmp` после сбоя и прочее
            }
        }
        for (record, raw) in own.into_values() {
            let head = record.as_ref().and_then(|r| read_json::<RecordHead>(&r.path));
            let keep = match (&record, &head) {
                (Some(r), Some(h)) => {
                    let ours = h.stamp == self.stamp;
                    alive(&h.id) && (ours || !old(r.mtime)) && (h.raw.is_some() == raw.is_some() || !ours)
                }
                _ => false, // сирота или испорченная запись
            };
            let paths: Vec<PathBuf> = record.iter().chain(raw.iter()).map(|i| i.path.clone()).collect();
            if keep {
                let bytes = record.iter().chain(raw.iter()).map(|i| i.len).sum();
                groups.push((record.as_ref().map_or(now, |r| r.mtime), bytes, paths));
            } else {
                for p in &paths {
                    remove(p, &mut pruned);
                }
            }
        }
        // Предел размера: сначала самые давние.
        groups.sort_by_key(|g| g.0);
        let mut total: u64 = groups.iter().map(|g| g.1).sum();
        for (_, bytes, paths) in &groups {
            if total <= limit {
                break;
            }
            for p in paths {
                remove(p, &mut pruned);
            }
            total -= bytes;
        }
        pruned.bytes = total;
        pruned
    }
}

/// Заголовок записи для чистки.
#[derive(Deserialize)]
struct RecordHead {
    stamp: String,
    id: String,
    raw: Option<String>,
}

#[derive(Debug)]
struct Item {
    path: PathBuf,
    is_dir: bool,
    len: u64,
    mtime: SystemTime,
}

fn list(dir: &Path) -> Vec<Item> {
    let Ok(read) = fs::read_dir(dir) else { return vec![] };
    read.flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some(Item { path: e.path(), is_dir: meta.is_dir(), len: meta.len(), mtime })
        })
        .collect()
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!("кэш {}: {e}", path.display());
            return None;
        }
    };
    match serde_json::from_slice(&data) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("кэш {}: {e} — пересоберу", path.display());
            None
        }
    }
}

/// Метка кода отрисовки: версия формата, версия приложения, хэш исходников
/// отрисовки, шрифтов и Typst (`build.rs`), `extra` — встроенная библиотека
/// и набор шрифтов (их считает вызывающий).
pub fn stamp(extra: &[u64]) -> String {
    let mut h = StableHasher::new();
    h.u64(u64::from(FORMAT)).str(env!("CARGO_PKG_VERSION")).str(env!("NOTES_RENDER_HASH"));
    for x in extra {
        h.u64(*x);
    }
    format!("{FORMAT}-{}", h.hex())
}

/// Новая метка отрисовки для записи.
pub fn new_tag(files: &str) -> String {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    StableHasher::new().str(files).bytes(&nanos.to_le_bytes()).hex()
}

/// Каталог кэша внутри каталога данных (`data/cache`; отрисовка — в
/// `pages/`, шрифты — в `fonts/`).
pub fn default_dir(data: &Path) -> PathBuf {
    data.join("cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(body: &str) -> Rendered {
        Rendered {
            title: Some("T".into()),
            styles: String::new(),
            body: body.into(),
            headings: vec![],
            links: vec![],
            tags: vec![],
        }
    }

    fn record(files: &str, raw: Option<&str>) -> Record {
        Record {
            files: files.into(),
            deps: vec![Dep::Vault("a.typ".into())],
            errors: vec![],
            warnings: vec![],
            build_ms: 7,
            raw: raw.map(Into::into),
            links: vec![],
        }
    }

    #[test]
    fn round_trip_and_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path(), "/хранилище", "м1".into());
        let id = NoteId::new("Сеть/SSH").unwrap();
        assert!(cache.record(&id).is_none());

        let rec = record("v1", Some("t1"));
        cache.store(&id, &rec, Some(&rendered("<p>x</p>")));
        assert_eq!(cache.record(&id).as_ref(), Some(&rec));
        assert_eq!(cache.raw(&id, &rec).unwrap().body, "<p>x</p>");

        // Ошибка: запись новая, отрисовка — прежняя (метка та же).
        let failed = Record { errors: vec![Diagnostic::error("сломано")], ..record("v2", Some("t1")) };
        cache.store(&id, &failed, None);
        assert_eq!(cache.record(&id).unwrap().errors.len(), 1);
        assert_eq!(cache.raw(&id, &failed).unwrap().body, "<p>x</p>");
        assert!(cache.raw(&id, &record("v2", Some("чужая"))).is_none(), "метки не совпали — промах");

        let other_build = DiskCache::new(dir.path(), "/хранилище", "м2".into());
        assert!(other_build.record(&id).is_none(), "другой код отрисовки");
        let other_vault = DiskCache::new(dir.path(), "/другое", "м1".into());
        assert!(other_vault.record(&id).is_none(), "другое хранилище");

        fs::write(cache.record_path(&id), "испорчено").unwrap();
        assert!(cache.record(&id).is_none());
    }

    #[test]
    fn prune_removes_dead_orphans_and_old_format() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path(), "/хранилище", "м1".into());
        let id = |s| NoteId::new(s).unwrap();
        for name in ["Живая", "Удалённая"] {
            cache.store(&id(name), &record("v", Some("t")), Some(&rendered("x")));
        }
        fs::write(dir.path().join("0123456789abcdef.json"), "{}").unwrap(); // старый формат
        let orphan = cache.raw_path(&id("Сирота"));
        fs::write(&orphan, "{}").unwrap();
        let fresh_foreign = DiskCache::new(dir.path(), "/другое", "м1".into());
        fresh_foreign.store(&id("Чужая"), &record("v", None), None);

        let pruned = cache.prune(&|id| id == "Живая", DISK_LIMIT);
        assert_eq!(pruned.removed, 4, "удалённая (2 файла), сирота, старый формат");
        assert!(cache.record(&id("Живая")).is_some());
        assert!(cache.record(&id("Удалённая")).is_none());
        assert!(!orphan.exists());
        assert!(fresh_foreign.record(&id("Чужая")).is_some(), "свежую чужую запись не трогаем");

        // Предел размера: остаётся не больше предела, давнее — первым.
        let pruned = cache.prune(&|_| true, 0);
        assert_eq!(pruned.bytes, 0);
        assert!(cache.record(&id("Живая")).is_none());
    }

    #[test]
    fn stamp_depends_on_library_and_fonts() {
        assert_eq!(stamp(&[1, 2]), stamp(&[1, 2]));
        assert_ne!(stamp(&[1, 2]), stamp(&[1, 3]));
        assert!(stamp(&[]).starts_with(&format!("{FORMAT}-")));
    }
}
