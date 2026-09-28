//! Кэш страниц: память и диск за одним интерфейсом.
//!
//! - **Записи** ([`Record`]: версия и файлы, ошибки, время сборки) — в памяти
//!   для каждой заметки, о которой что-то известно (маленькие); при промахе
//!   берутся с диска. По ним — «собрана ли заметка» и её версия (`stat`
//!   файлов, без компиляции).
//! - **Страницы** — в памяти, в LRU с пределом по байтам: одна
//!   обработанная копия (под текущие настройки рисунков). Сырая отрисовка
//!   лежит на диске и читается, только когда поменялись настройки или
//!   страница вытеснена. Без кэша на диске сырая держится в памяти (иначе
//!   её негде взять), в том же пределе.
//! - Прогрев пишет только на диск ([`PageCache::store`] без страницы): в
//!   памяти остаются лишь открытые заметки.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;

use crate::cache::{DiskCache, DiskLimits, Pruned, Record};
use crate::figures::FigureOptions;
use crate::pages::NotePage;
use crate::render::{LinkRef, Rendered};
use crate::vault::{Entry, NoteId};
use crate::version::Versions;

/// Предел страниц в памяти по умолчанию (компьютер; меняется настройкой
/// устройства — [`PageCache::set_limits`]).
pub const MEMORY_BUDGET: usize = 64 << 20;

/// Метка сырой отрисовки в памяти, когда кэша на диске нет.
const IN_MEMORY: &str = "память";

/// Сырая отрисовка для [`PageCache::store`].
#[derive(Debug)]
pub enum Raw {
    /// Новая отрисовка.
    New(Arc<Rendered>),
    /// Прежняя удачная (сборка с ошибкой) — та, что уже в кэше.
    Previous(Arc<Rendered>),
    /// Отрисовки нет.
    None,
}

#[derive(Debug)]
pub struct PageCache {
    versions: Versions,
    disk: Option<DiskCache>,
    /// Предел страниц в памяти, байт.
    budget: AtomicUsize,
    disk_limits: Mutex<DiskLimits>,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    records: HashMap<NoteId, Record>,
    held: HashMap<NoteId, Held>,
    /// Часы LRU.
    clock: u64,
    bytes: usize,
}

/// Что держится в памяти для заметки.
#[derive(Debug)]
struct Held {
    /// Сырая отрисовка — только без кэша на диске.
    raw: Option<Arc<Rendered>>,
    /// Обработанная страница и настройки, под которые она обработана.
    page: Option<(Arc<NotePage>, FigureOptions)>,
    used: u64,
    bytes: usize,
}

impl Held {
    fn measure(&mut self) {
        let raw = self.raw.as_deref().map_or(0, size_of_rendered);
        let page = self.page.as_ref().and_then(|(p, _)| p.rendered.as_deref()).map_or(0, size_of_rendered);
        self.bytes = raw + page;
    }
}

/// Примерный вес отрисовки в памяти.
fn size_of_rendered(r: &Rendered) -> usize {
    r.body.len() + r.styles.len()
}

/// Версия страницы: файлы + настройки обработки рисунков.
pub fn page_version(files: &str, opts: FigureOptions) -> String {
    format!("{files}-{}", opts.key())
}

impl PageCache {
    pub fn new(versions: Versions, disk: Option<DiskCache>, budget: usize) -> Self {
        Self {
            versions,
            disk,
            budget: AtomicUsize::new(budget),
            disk_limits: Mutex::new(DiskLimits::default()),
            state: Mutex::default(),
        }
    }

    /// Пределы кэша на диске (настройки устройства).
    pub fn disk_limits(&self) -> DiskLimits {
        *self.disk_limits.lock()
    }

    /// Пределы из настроек устройства: память — сразу (лишнее вытеснится
    /// при следующей записи), диск — при следующей чистке.
    pub fn set_limits(&self, memory: usize, disk: DiskLimits) {
        self.budget.store(memory, Ordering::Relaxed);
        *self.disk_limits.lock() = disk;
    }

    pub fn has_disk(&self) -> bool {
        self.disk.is_some()
    }

    /// Запись заметки (из памяти или с диска).
    fn record(&self, id: &NoteId) -> Option<Record> {
        if let Some(r) = self.state.lock().records.get(id) {
            return Some(r.clone());
        }
        let record = self.disk.as_ref()?.record(id)?;
        self.state.lock().records.insert(id.clone(), record.clone());
        Some(record)
    }

    /// Годна ли запись для текущих файлов.
    fn is_current(&self, record: &Record) -> bool {
        self.versions.current(&record.deps) == record.files
    }

    /// Собрана ли заметка для текущих файлов (в памяти или на диске).
    pub fn is_fresh(&self, id: &NoteId) -> bool {
        self.record(id).is_some_and(|r| self.is_current(&r))
    }

    /// Текущая версия страницы по известному списку файлов (без сборки).
    pub fn version(&self, id: &NoteId, opts: FigureOptions) -> Option<String> {
        let record = self.record(id)?;
        Some(page_version(&self.versions.current(&record.deps), opts))
    }

    /// Ссылки последней удачной сборки (в памяти или на диске).
    pub fn links(&self, id: &NoteId) -> Option<Vec<LinkRef>> {
        self.record(id).map(|r| r.links)
    }

    /// Сколько длилась прошлая сборка, мс.
    pub fn build_ms(&self, id: &NoteId) -> Option<u64> {
        self.record(id).map(|r| r.build_ms)
    }

    /// Страница, если файлы заметки не менялись. `memory_only` — только
    /// готовая страница в памяти (без чтения диска и обработки рисунков).
    pub fn page(
        &self,
        entry: &Entry,
        opts: FigureOptions,
        finish: &dyn Fn(&Rendered, FigureOptions) -> Rendered,
        memory_only: bool,
    ) -> Option<Arc<NotePage>> {
        let id = &entry.id;
        let record = if memory_only { self.state.lock().records.get(id)?.clone() } else { self.record(id)? };
        if !self.is_current(&record) {
            return None;
        }
        if let Some(page) = self.held_page(id, opts) {
            return Some(page);
        }
        if memory_only {
            return None;
        }
        let raw = match &record.raw {
            None => None,
            Some(_) => Some(self.raw(id, &record)?),
        };
        tracing::debug!(%id, memory = self.state.lock().held.contains_key(id), "страница из кэша");
        let page = Arc::new(NotePage {
            id: id.clone(),
            kind: entry.kind,
            version: page_version(&record.files, opts),
            rendered: raw.map(|r| Arc::new(finish(&r, opts))),
            errors: record.errors.clone(),
            warnings: record.warnings.clone(),
            book: None,
        });
        self.hold(id, None, Some((page.clone(), opts)));
        Some(page)
    }

    /// Готовая страница в памяти под эти настройки.
    fn held_page(&self, id: &NoteId, opts: FigureOptions) -> Option<Arc<NotePage>> {
        let mut state = self.state.lock();
        state.clock += 1;
        let clock = state.clock;
        let held = state.held.get_mut(id)?;
        held.used = clock;
        held.page.as_ref().filter(|(_, o)| *o == opts).map(|(p, _)| p.clone())
    }

    /// Сырая отрисовка по записи: из памяти или с диска.
    fn raw(&self, id: &NoteId, record: &Record) -> Option<Arc<Rendered>> {
        if let Some(raw) = self.state.lock().held.get(id).and_then(|h| h.raw.clone()) {
            return Some(raw);
        }
        let raw = self.disk.as_ref()?.raw(id, record)?;
        tracing::debug!(%id, "отрисовка с диска");
        Some(Arc::new(raw))
    }

    /// Последняя удачная отрисовка (годная или нет) — показать под ошибкой.
    pub fn last_good(&self, id: &NoteId) -> Option<Arc<Rendered>> {
        let record = self.record(id)?;
        record.raw.as_ref()?;
        self.raw(id, &record)
    }

    /// Запомнить сборку: запись — в память и на диск, отрисовку — на диск
    /// (без диска — в память), страницу `page` — в память (прогрев её не
    /// передаёт).
    pub fn store(&self, id: &NoteId, mut record: Record, raw: Raw, page: Option<(Arc<NotePage>, FigureOptions)>) {
        let previous_tag = self.state.lock().records.get(id).and_then(|r| r.raw.clone());
        let (fresh, kept) = match raw {
            // Прежняя отрисовка уже лежит под своей меткой; нет метки — пишем заново.
            Raw::Previous(r) if previous_tag.is_some() && self.disk.is_some() => (None, Some(r)),
            Raw::New(r) | Raw::Previous(r) => (Some(r), None),
            Raw::None => (None, None),
        };
        record.raw = match (&fresh, &kept) {
            (Some(_), _) if self.disk.is_some() => Some(crate::cache::new_tag(&record.files)),
            (Some(_), _) => Some(IN_MEMORY.into()),
            (None, Some(_)) => previous_tag,
            (None, None) => None,
        };
        if let Some(disk) = &self.disk {
            disk.store(id, &record, fresh.as_deref());
        }
        self.state.lock().records.insert(id.clone(), record);
        let raw_in_memory = if self.disk.is_none() { fresh.or(kept) } else { None };
        // Прежняя страница устарела вместе с записью.
        self.forget(id);
        if raw_in_memory.is_some() || page.is_some() {
            self.hold(id, raw_in_memory, page);
        }
    }

    fn forget(&self, id: &NoteId) {
        let mut state = self.state.lock();
        if let Some(h) = state.held.remove(id) {
            state.bytes -= h.bytes;
        }
    }

    /// Положить в память (сырая — `None`: оставить прежнюю) и вытеснить
    /// давно не нужные страницы сверх предела.
    fn hold(&self, id: &NoteId, raw: Option<Arc<Rendered>>, page: Option<(Arc<NotePage>, FigureOptions)>) {
        let mut state = self.state.lock();
        state.clock += 1;
        let clock = state.clock;
        let old = state.held.remove(id);
        let old_bytes = old.as_ref().map_or(0, |h| h.bytes);
        let mut held = Held { raw: raw.or_else(|| old.and_then(|h| h.raw)), page, used: clock, bytes: 0 };
        held.measure();
        state.bytes = state.bytes - old_bytes + held.bytes;
        state.held.insert(id.clone(), held);
        let budget = self.budget.load(Ordering::Relaxed);
        while state.bytes > budget && state.held.len() > 1 {
            let Some(victim) =
                state.held.iter().filter(|(k, _)| *k != id).min_by_key(|(_, h)| h.used).map(|(k, _)| k.clone())
            else {
                break;
            };
            let h = state.held.remove(&victim).expect("есть");
            state.bytes -= h.bytes;
            tracing::debug!(id = %victim, bytes = h.bytes, "вытеснена из памяти");
        }
    }

    /// Сколько страниц и байт в памяти.
    pub fn memory(&self) -> (usize, usize) {
        let state = self.state.lock();
        (state.held.len(), state.bytes)
    }

    /// Чистка кэша на диске (см. [`DiskCache::prune`]).
    pub fn prune(&self, alive: &dyn Fn(&str) -> bool) -> Option<Pruned> {
        let limits = *self.disk_limits.lock();
        Some(self.disk.as_ref()?.prune(alive, limits))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::storage::MemStorage;
    use crate::vault::NoteKind;
    use crate::version::Dep;

    fn rendered(body: &str) -> Rendered {
        Rendered {
            title: None,
            styles: String::new(),
            body: body.into(),
            headings: vec![],
            links: vec![],
            tags: vec![],
        }
    }

    /// Обработка рисунков в тестах: помечает тело настройками.
    fn finish(r: &Rendered, opts: FigureOptions) -> Rendered {
        Rendered { body: format!("{}|{}", r.body, opts.key()), ..rendered("") }
    }

    struct Fixture {
        mem: Arc<MemStorage>,
        versions: Versions,
        _dir: tempfile::TempDir,
        dir: PathBuf,
    }

    fn fixture() -> Fixture {
        let mem = Arc::new(MemStorage::new());
        mem.write("A.typ", "a");
        mem.write("B.typ", "b");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        Fixture { versions: Versions::new(mem.clone()), mem, _dir: dir, dir: path }
    }

    impl Fixture {
        fn cache(&self, disk: bool, budget: usize) -> PageCache {
            let disk = disk.then(|| DiskCache::new(&self.dir, "mem", "м".into()));
            PageCache::new(self.versions.clone(), disk, budget)
        }

        fn record(&self, file: &str) -> Record {
            let deps = vec![Dep::Vault(file.into())];
            Record {
                files: self.versions.current(&deps),
                deps,
                errors: vec![],
                warnings: vec![],
                build_ms: 5,
                raw: None,
                links: vec![],
            }
        }
    }

    fn entry(id: &str) -> Entry {
        Entry { id: NoteId::new(id).unwrap(), kind: NoteKind::Note, main: PathBuf::from(format!("{id}.typ")) }
    }

    fn page(e: &Entry, body: &str, opts: FigureOptions) -> (Arc<NotePage>, FigureOptions) {
        let page = NotePage {
            id: e.id.clone(),
            kind: e.kind,
            version: String::new(),
            rendered: Some(Arc::new(finish(&rendered(body), opts))),
            errors: vec![],
            warnings: vec![],
            book: None,
        };
        (Arc::new(page), opts)
    }

    const P2: FigureOptions = FigureOptions { precision: Some(2) };
    const FULL: FigureOptions = FigureOptions { precision: None };

    #[test]
    fn warm_goes_to_disk_only_and_opens_from_it() {
        let f = fixture();
        let cache = f.cache(true, MEMORY_BUDGET);
        let a = entry("A");
        cache.store(&a.id, f.record("A.typ"), Raw::New(Arc::new(rendered("a"))), None);
        assert_eq!(cache.memory(), (0, 0), "прогрев не держит страницу в памяти");
        assert!(cache.is_fresh(&a.id));
        assert!(cache.page(&a, P2, &finish, true).is_none(), "в памяти её нет");

        let got = cache.page(&a, P2, &finish, false).unwrap();
        assert_eq!(got.rendered.as_ref().unwrap().body, "a|p2");
        assert_eq!(cache.memory().0, 1);
        assert!(Arc::ptr_eq(&got, &cache.page(&a, P2, &finish, true).unwrap()), "дальше — из памяти");
        // Другие настройки — сырая с диска, одна обработанная копия в памяти.
        assert_eq!(cache.page(&a, FULL, &finish, false).unwrap().rendered.as_ref().unwrap().body, "a|full");
        assert!(cache.page(&a, P2, &finish, true).is_none());

        // После перезапуска — с диска.
        let restarted = f.cache(true, MEMORY_BUDGET);
        assert!(restarted.is_fresh(&a.id));
        assert_eq!(restarted.build_ms(&a.id), Some(5));
        assert_eq!(restarted.page(&a, P2, &finish, false).unwrap().rendered.as_ref().unwrap().body, "a|p2");
    }

    #[test]
    fn edited_file_makes_page_stale() {
        let f = fixture();
        let cache = f.cache(true, MEMORY_BUDGET);
        let a = entry("A");
        cache.store(&a.id, f.record("A.typ"), Raw::New(Arc::new(rendered("a"))), Some(page(&a, "a", P2)));
        let before = cache.version(&a.id, P2).unwrap();
        f.mem.write("A.typ", "aa");
        assert!(!cache.is_fresh(&a.id));
        assert_ne!(cache.version(&a.id, P2).unwrap(), before);
        assert!(cache.page(&a, P2, &finish, false).is_none());
        assert_eq!(cache.last_good(&a.id).unwrap().body, "a", "прежняя отрисовка — показать под ошибкой");
    }

    #[test]
    fn error_keeps_previous_render_across_restart() {
        let f = fixture();
        let cache = f.cache(true, MEMORY_BUDGET);
        let a = entry("A");
        cache.store(&a.id, f.record("A.typ"), Raw::New(Arc::new(rendered("a"))), None);
        f.mem.write("A.typ", "сломано");
        let previous = cache.last_good(&a.id).unwrap();
        let failed = Record { errors: vec![crate::diag::Diagnostic::error("ошибка")], ..f.record("A.typ") };
        cache.store(&a.id, failed, Raw::Previous(previous), None);

        let restarted = f.cache(true, MEMORY_BUDGET);
        assert!(restarted.is_fresh(&a.id), "ошибка тоже в кэше: не собирать заново");
        let got = restarted.page(&a, P2, &finish, false).unwrap();
        assert_eq!(got.errors.len(), 1);
        assert_eq!(got.rendered.as_ref().unwrap().body, "a|p2", "под ошибкой — прежняя отрисовка");
    }

    #[test]
    fn memory_is_bounded_lru() {
        let f = fixture();
        let big = "x".repeat(100);
        // Предел — на две страницы.
        let cache = f.cache(true, 2 * (big.len() + 3));
        let (a, b) = (entry("A"), entry("B"));
        f.mem.write("C.typ", "c");
        let c = entry("C");
        for e in [&a, &b] {
            let file = format!("{}.typ", e.id);
            cache.store(&e.id, f.record(&file), Raw::New(Arc::new(rendered(&big))), Some(page(e, &big, P2)));
        }
        cache.page(&a, P2, &finish, true).unwrap(); // A — недавняя
        cache.store(&c.id, f.record("C.typ"), Raw::New(Arc::new(rendered(&big))), Some(page(&c, &big, P2)));
        assert_eq!(cache.memory().0, 2);
        assert!(cache.page(&b, P2, &finish, true).is_none(), "B вытеснена");
        assert!(cache.page(&a, P2, &finish, true).is_some());
        assert!(cache.page(&b, P2, &finish, false).is_some(), "но есть на диске");
    }

    #[test]
    fn without_disk_raw_stays_in_memory() {
        let f = fixture();
        let cache = f.cache(false, MEMORY_BUDGET);
        let a = entry("A");
        cache.store(&a.id, f.record("A.typ"), Raw::New(Arc::new(rendered("a"))), None);
        assert_eq!(cache.memory().0, 1, "без диска прогретое держится в памяти");
        assert_eq!(cache.page(&a, FULL, &finish, false).unwrap().rendered.as_ref().unwrap().body, "a|full");
        assert!(f.cache(false, MEMORY_BUDGET).record(&a.id).is_none());
    }
}
