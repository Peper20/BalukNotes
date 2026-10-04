//! The page cache: memory and disk behind one interface.
//!
//! - **Records** ([`Record`]: version and files, errors, build time) are kept
//!   in memory for every note anything is known about (they are small); on a
//!   miss they come from disk. They answer "is the note built" and give its
//!   version (by `stat` of the files, without compiling).
//! - **Pages** are kept in memory in an LRU with a byte limit: one processed
//!   copy (for the current figure settings). The raw rendering lies on disk and
//!   is read only when the settings changed or the page was evicted. Without a
//!   disk cache the raw rendering is kept in memory (there is nowhere else to
//!   get it), within the same limit.
//! - Warming writes only to disk ([`PageCache::store`] without a page): only
//!   open notes stay in memory.

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

/// The default limit of pages in memory (a computer; a device setting changes
/// it: [`PageCache::set_limits`]).
pub const MEMORY_BUDGET: usize = 64 << 20;

/// The raw rendering tag in memory when there is no disk cache.
const IN_MEMORY: &str = "memory";

/// The raw rendering for [`PageCache::store`].
#[derive(Debug)]
pub enum Raw {
    /// A new rendering.
    New(Arc<Rendered>),
    /// The earlier good one (a build with an error), already in the cache.
    Previous(Arc<Rendered>),
    /// No rendering.
    None,
}

/// The cache of built pages of one vault (see the module).
#[derive(Debug)]
pub struct PageCache {
    versions: Versions,
    disk: Option<DiskCache>,
    /// The limit of pages in memory, in bytes.
    budget: AtomicUsize,
    disk_limits: Mutex<DiskLimits>,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    records: HashMap<NoteId, Record>,
    held: HashMap<NoteId, Held>,
    /// The LRU clock.
    clock: u64,
    bytes: usize,
}

/// What is kept in memory for a note.
#[derive(Debug)]
struct Held {
    /// The raw rendering: only without a disk cache.
    raw: Option<Arc<Rendered>>,
    /// The processed page and the settings it was processed for.
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

/// The rough weight of a rendering in memory.
fn size_of_rendered(r: &Rendered) -> usize {
    r.body.len() + r.styles.len()
}

/// The page version: the files + the figure processing settings.
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

    /// Disk cache limits (device settings).
    pub fn disk_limits(&self) -> DiskLimits {
        *self.disk_limits.lock()
    }

    /// Limits from the device settings: memory at once (the excess is evicted
    /// on the next write), disk on the next cleanup.
    pub fn set_limits(&self, memory: usize, disk: DiskLimits) {
        self.budget.store(memory, Ordering::Relaxed);
        *self.disk_limits.lock() = disk;
    }

    pub fn has_disk(&self) -> bool {
        self.disk.is_some()
    }

    /// The note record (from memory or disk).
    fn record(&self, id: &NoteId) -> Option<Record> {
        if let Some(r) = self.state.lock().records.get(id) {
            return Some(r.clone());
        }
        let record = self.disk.as_ref()?.record(id)?;
        self.state.lock().records.insert(id.clone(), record.clone());
        Some(record)
    }

    /// Whether the record is good for the current files.
    fn is_current(&self, record: &Record) -> bool {
        self.versions.current(&record.deps) == record.files
    }

    /// Whether the note is built for the current files (in memory or on disk).
    pub fn is_fresh(&self, id: &NoteId) -> bool {
        self.record(id).is_some_and(|r| self.is_current(&r))
    }

    /// The current page version by the known file list (without building).
    pub fn version(&self, id: &NoteId, opts: FigureOptions) -> Option<String> {
        let record = self.record(id)?;
        Some(page_version(&self.versions.current(&record.deps), opts))
    }

    /// Links of the last good build (in memory or on disk).
    pub fn links(&self, id: &NoteId) -> Option<Vec<LinkRef>> {
        self.record(id).map(|r| r.links)
    }

    /// How long the last build took, in ms.
    pub fn build_ms(&self, id: &NoteId) -> Option<u64> {
        self.record(id).map(|r| r.build_ms)
    }

    /// The page, if the note files did not change. `memory_only`: only a finished
    /// page in memory (no disk reads, no figure processing).
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
        tracing::debug!(%id, memory = self.state.lock().held.contains_key(id), "page from cache");
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

    /// A finished page in memory for these settings.
    fn held_page(&self, id: &NoteId, opts: FigureOptions) -> Option<Arc<NotePage>> {
        let mut state = self.state.lock();
        state.clock += 1;
        let clock = state.clock;
        let held = state.held.get_mut(id)?;
        held.used = clock;
        held.page.as_ref().filter(|(_, o)| *o == opts).map(|(p, _)| p.clone())
    }

    /// The raw rendering for a record: from memory or disk.
    fn raw(&self, id: &NoteId, record: &Record) -> Option<Arc<Rendered>> {
        if let Some(raw) = self.state.lock().held.get(id).and_then(|h| h.raw.clone()) {
            return Some(raw);
        }
        let raw = self.disk.as_ref()?.raw(id, record)?;
        tracing::debug!(%id, "rendering from disk");
        Some(Arc::new(raw))
    }

    /// The last good rendering (current or not), to show under an error.
    pub fn last_good(&self, id: &NoteId) -> Option<Arc<Rendered>> {
        let record = self.record(id)?;
        record.raw.as_ref()?;
        self.raw(id, &record)
    }

    /// Remembers a build: the record to memory and disk, the rendering to disk
    /// (without a disk, to memory), the page `page` to memory (warming does not
    /// pass one).
    pub fn store(&self, id: &NoteId, mut record: Record, raw: Raw, page: Option<(Arc<NotePage>, FigureOptions)>) {
        let previous_tag = self.state.lock().records.get(id).and_then(|r| r.raw.clone());
        let (fresh, kept) = match raw {
            // The earlier rendering already lies under its tag; no tag - write it again.
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
        // The earlier page is stale together with the record.
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

    /// Puts into memory (a `None` raw keeps the earlier one) and evicts pages
    /// not needed for long beyond the limit.
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
            let oldest =
                state.held.iter().filter(|(k, _)| *k != id).min_by_key(|(_, h)| h.used).map(|(k, _)| k.clone());
            let Some((victim, h)) = oldest.and_then(|k| state.held.remove_entry(&k)) else {
                break;
            };
            state.bytes -= h.bytes;
            tracing::debug!(id = %victim, bytes = h.bytes, "evicted from memory");
        }
    }

    /// How many pages and bytes are in memory.
    pub fn memory(&self) -> (usize, usize) {
        let state = self.state.lock();
        (state.held.len(), state.bytes)
    }

    /// Disk cache cleanup (see [`DiskCache::prune`]).
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
            sanitizer: None,
        }
    }

    /// Figure processing in tests: marks the body with the settings.
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
        assert_eq!(cache.memory(), (0, 0), "warming does not keep the page in memory");
        assert!(cache.is_fresh(&a.id));
        assert!(cache.page(&a, P2, &finish, true).is_none(), "it is not in memory");

        let got = cache.page(&a, P2, &finish, false).unwrap();
        assert_eq!(got.rendered.as_ref().unwrap().body, "a|p2");
        assert_eq!(cache.memory().0, 1);
        assert!(Arc::ptr_eq(&got, &cache.page(&a, P2, &finish, true).unwrap()), "from memory from then on");
        // Other settings: the raw one from disk, one processed copy in memory.
        assert_eq!(cache.page(&a, FULL, &finish, false).unwrap().rendered.as_ref().unwrap().body, "a|full");
        assert!(cache.page(&a, P2, &finish, true).is_none());

        // After a restart: from disk.
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
        assert_eq!(cache.last_good(&a.id).unwrap().body, "a", "the earlier rendering, to show under an error");
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
        assert!(restarted.is_fresh(&a.id), "an error is cached too: do not build again");
        let got = restarted.page(&a, P2, &finish, false).unwrap();
        assert_eq!(got.errors.len(), 1);
        assert_eq!(got.rendered.as_ref().unwrap().body, "a|p2", "under an error: the earlier rendering");
    }

    #[test]
    fn memory_is_bounded_lru() {
        let f = fixture();
        let big = "x".repeat(100);
        // The limit fits two pages.
        let cache = f.cache(true, 2 * (big.len() + 3));
        let (a, b) = (entry("A"), entry("B"));
        f.mem.write("C.typ", "c");
        let c = entry("C");
        for e in [&a, &b] {
            let file = format!("{}.typ", e.id);
            cache.store(&e.id, f.record(&file), Raw::New(Arc::new(rendered(&big))), Some(page(e, &big, P2)));
        }
        cache.page(&a, P2, &finish, true).unwrap(); // A is recent
        cache.store(&c.id, f.record("C.typ"), Raw::New(Arc::new(rendered(&big))), Some(page(&c, &big, P2)));
        assert_eq!(cache.memory().0, 2);
        assert!(cache.page(&b, P2, &finish, true).is_none(), "B is evicted");
        assert!(cache.page(&a, P2, &finish, true).is_some());
        assert!(cache.page(&b, P2, &finish, false).is_some(), "but it is on disk");
    }

    #[test]
    fn without_disk_raw_stays_in_memory() {
        let f = fixture();
        let cache = f.cache(false, MEMORY_BUDGET);
        let a = entry("A");
        cache.store(&a.id, f.record("A.typ"), Raw::New(Arc::new(rendered("a"))), None);
        assert_eq!(cache.memory().0, 1, "without a disk the warmed page stays in memory");
        assert_eq!(cache.page(&a, FULL, &finish, false).unwrap().rendered.as_ref().unwrap().body, "a|full");
        assert!(f.cache(false, MEMORY_BUDGET).record(&a.id).is_none());
    }
}
