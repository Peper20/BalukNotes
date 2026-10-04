//! Pages on request: the build ([`Pipeline`]) over the cache ([`PageCache`]).
//!
//! A note is built when it is requested (or ahead: warming, [`crate::warm`]).
//! Builds of **different** notes run in parallel (each has its own compiler
//! file cache, [`crate::world`]): a note opens at once while a book builds. A
//! note builds once at a time (the note lock in `building`): a second request
//! for the same note during its build waits for the first and takes the result
//! from the cache instead of building again. A waiting user request tells
//! warming not to start a new build ([`Pages::wait_for_users`]): the reader
//! gets the CPU first.
//!
//! A compile error is shown over the last good rendering. The page version is
//! the file version + the figure processing settings: changing the settings
//! does not recompile the note, and the client requests the page again itself.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;

use crate::Result;
use crate::cache::Record;
use crate::diag::Diagnostic;
use crate::figures::FigureOptions;
use crate::page_cache::{PageCache, Raw, page_version};
use crate::pipeline::{Build, Pipeline, Priority};
use crate::render::Rendered;
use crate::vault::{Entry, NoteId, NoteKind, Vault};

/// A note ready to be shown.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NotePage {
    pub id: NoteId,
    pub kind: NoteKind,
    /// The version of the note files and rendering settings.
    pub version: String,
    /// The last good rendering. On a compile error, the previous good one (if
    /// any): the reader sees the note with the error over it.
    pub rendered: Option<Arc<Rendered>>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// A book by chapters: which chapter is in `rendered` and where the others
    /// are. Only in the answer to a chapter request ([`crate::book::chapter_page`]);
    /// `None` for a whole page.
    pub book: Option<crate::book::BookView>,
}

/// Pages of one vault: builds on request over the page cache (see the module).
#[derive(Debug)]
pub struct Pages {
    vault: Vault,
    pipeline: Arc<dyn Pipeline>,
    cache: Arc<PageCache>,
    /// Note locks: held for the time of the note's build.
    building: Mutex<HashMap<NoteId, Arc<Mutex<()>>>>,
    /// How many page requests wait now: warming yields to them.
    waiting: AtomicUsize,
    /// The state of releasing memory after a series of builds.
    releaser: Arc<Mutex<ReleaserState>>,
}

/// E5: a release adds about 3 ms to the next rebuild; 5 s is a compromise that
/// noticeably lowers idle RSS and still fires in normal work.
const IDLE_RELEASE_DELAY: Duration = Duration::from_secs(5);
const RELEASE_POLL_DELAY: Duration = Duration::from_millis(200);

#[derive(Debug, Default)]
struct ReleaserState {
    /// Whether there was a build after the last release.
    pending_release: bool,
    /// The last build written to the cache.
    last_build: Option<Instant>,
}

impl Pages {
    pub fn new(vault: Vault, pipeline: Arc<dyn Pipeline>, cache: PageCache) -> Self {
        let pages = Self {
            vault,
            pipeline,
            cache: Arc::new(cache),
            building: Mutex::default(),
            waiting: AtomicUsize::new(0),
            releaser: Arc::new(Mutex::default()),
        };
        // One debouncer per Pages: releasing memory when idle.
        pages.spawn_releaser();
        pages
    }

    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    pub fn cache(&self) -> &Arc<PageCache> {
        &self.cache
    }

    /// The note page: from the cache if its files did not change.
    pub fn page(&self, id: &NoteId, opts: FigureOptions) -> Result<Arc<NotePage>> {
        /// A request waits for a page: warming does not start a new build.
        struct Waiting<'a>(&'a AtomicUsize);
        impl Drop for Waiting<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        self.waiting.fetch_add(1, Ordering::SeqCst);
        let _waiting = Waiting(&self.waiting);

        let entry = self.vault.entry(id)?;
        let finish = |raw: &Rendered, opts| self.pipeline.finish(raw, opts);
        if let Some(page) = self.cache.page(&entry, opts, &finish, true) {
            return Ok(page);
        }
        let lock = self.note_lock(id);
        let _building = lock.lock();
        if let Some(page) = self.cache.page(&entry, opts, &finish, false) {
            return Ok(page);
        }
        let built = self.pipeline.build(&entry, Priority::User);
        Ok(self.store_page(&entry, built, opts))
    }

    /// The current note version. For a known note only `stat` of its files,
    /// without compiling; a new one gets built.
    pub fn version(&self, id: &NoteId, opts: FigureOptions) -> Result<String> {
        if let Some(version) = self.cache.version(id, opts) {
            return Ok(version);
        }
        Ok(self.page(id, opts)?.version.clone())
    }

    /// Whether the note has a good build, in memory or on disk.
    pub fn is_built(&self, id: &NoteId) -> bool {
        self.cache.is_fresh(id)
    }

    /// How long the last build of the note took.
    pub fn last_build(&self, id: &NoteId) -> Option<Duration> {
        self.cache.build_ms(id).map(Duration::from_millis)
    }

    /// Waits until user requests get their pages.
    pub fn wait_for_users(&self) {
        while self.waiting.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Builds a note ahead (warming): to the disk cache, without figure
    /// processing and without a page in memory. An already built note is
    /// skipped. `true` means it was built now.
    pub fn prebuild(&self, id: &NoteId) -> Result<bool> {
        let entry = self.vault.entry(id)?;
        let lock = self.note_lock(id);
        let _building = lock.lock();
        if self.cache.is_fresh(id) {
            return Ok(false);
        }
        let built = self.pipeline.build(&entry, Priority::Background);
        self.store(&entry, built);
        Ok(true)
    }

    /// Frees the memory of builds (after a warming round).
    pub fn release_memory(&self) {
        self.pipeline.release_memory();
    }

    /// The build lock of a note. Locks of notes not being built are dropped so
    /// the map does not grow.
    fn note_lock(&self, id: &NoteId) -> Arc<Mutex<()>> {
        let mut locks = self.building.lock();
        locks.retain(|_, l| Arc::strong_count(l) > 1);
        locks.entry(id.clone()).or_default().clone()
    }

    /// Stores a build without a page (warming). On an error the earlier good
    /// rendering stays under it.
    fn store(&self, entry: &Entry, built: Build) {
        let (record, raw, _) = self.record(entry, built);
        self.cache.store(&entry.id, record, raw, None);
        self.note_built();
    }

    /// Stores a build with its page in `opts` and returns the page.
    fn store_page(&self, entry: &Entry, built: Build, opts: FigureOptions) -> Arc<NotePage> {
        let (record, raw, shown) = self.record(entry, built);
        let page = Arc::new(NotePage {
            id: entry.id.clone(),
            kind: entry.kind,
            version: page_version(&record.files, opts),
            rendered: shown.map(|r| Arc::new(self.pipeline.finish(&r, opts))),
            errors: record.errors.clone(),
            warnings: record.warnings.clone(),
            book: None,
        });
        self.cache.store(&entry.id, record, raw, Some((page.clone(), opts)));
        self.note_built();
        page
    }

    /// The cache record of a build, its raw rendering for the cache and the
    /// rendering to show (the previous good one if the build failed).
    fn record(&self, entry: &Entry, built: Build) -> (Record, Raw, Option<Arc<Rendered>>) {
        let (raw, shown) = match built.raw {
            Some(r) => {
                let r = Arc::new(r);
                (Raw::New(r.clone()), Some(r))
            }
            None => match self.cache.last_good(&entry.id) {
                Some(r) => (Raw::Previous(r.clone()), Some(r)),
                None => (Raw::None, None),
            },
        };
        // Links of the shown rendering, for the link index (computed paths).
        let links = shown.as_ref().map(|r| r.links.clone()).unwrap_or_default();
        let record = Record {
            files: built.files,
            deps: built.deps,
            errors: built.errors,
            warnings: built.warnings,
            build_ms: u64::try_from(built.took.as_millis()).unwrap_or(u64::MAX),
            raw: None,
            links,
        };
        (record, raw, shown)
    }

    /// Delayed memory release: marks a build (the debouncer will see the idle time).
    fn note_built(&self) {
        let mut state = self.releaser.lock();
        state.pending_release = true;
        state.last_build = Some(Instant::now());
    }

    /// One worker thread per `Pages`: watches for idle time and calls the release.
    fn spawn_releaser(&self) {
        let pipeline = Arc::downgrade(&self.pipeline);
        let releaser = Arc::downgrade(&self.releaser);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(RELEASE_POLL_DELAY);
                let (Some(pipeline), Some(releaser)) = (pipeline.upgrade(), releaser.upgrade()) else {
                    break;
                };
                let should_release = {
                    let mut state = releaser.lock();
                    let idle = state.last_build.as_ref().is_some_and(|built| built.elapsed() >= IDLE_RELEASE_DELAY);
                    if state.pending_release && idle {
                        state.pending_release = false;
                        true
                    } else {
                        false
                    }
                };
                if should_release {
                    pipeline.release_memory();
                }
            }
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::cache::DiskCache;
    use crate::page_cache::MEMORY_BUDGET;
    use crate::storage::MemStorage;
    use crate::version::{Dep, Versions};

    /// A build without Typst: the rendering is the text of the main file, and
    /// `ошибка` ("error") in the text means a compile error.
    #[derive(Debug)]
    pub(crate) struct FakePipeline {
        pub vault: Vault,
        pub versions: Versions,
        pub builds: AtomicUsize,
        /// A build delay (to test concurrent requests).
        pub delay: Duration,
    }

    impl Pipeline for FakePipeline {
        fn build(&self, entry: &Entry, _priority: Priority) -> Build {
            self.builds.fetch_add(1, Ordering::SeqCst);
            let main = entry.main.to_string_lossy().into_owned();
            let deps = vec![Dep::Vault(main.clone())];
            let files = self.versions.current(&deps);
            let text = self.vault.read_text(&main).unwrap_or_default();
            std::thread::sleep(self.delay);
            let (raw, errors) = if text.contains("ошибка") {
                (None, vec![Diagnostic::error("ошибка")])
            } else {
                let raw = Rendered {
                    title: None,
                    styles: String::new(),
                    body: text,
                    headings: vec![],
                    links: vec![],
                    tags: vec![],
                    sanitizer: None,
                };
                (Some(raw), vec![])
            };
            Build { raw, errors, warnings: vec![], deps, files, took: Duration::from_millis(text_len(&main)) }
        }

        fn finish(&self, raw: &Rendered, opts: FigureOptions) -> Rendered {
            Rendered { body: format!("{}|{}", raw.body, opts.key()), ..raw.clone() }
        }
    }

    /// The "build time" in tests is the path length, so the order is predictable.
    fn text_len(main: &str) -> u64 {
        main.chars().count() as u64
    }

    pub(crate) struct Setup {
        pub mem: Arc<MemStorage>,
        pub dir: tempfile::TempDir,
    }

    impl Setup {
        pub(crate) fn new(files: &[(&str, &str)]) -> Self {
            let mem = Arc::new(MemStorage::new());
            for (path, text) in files {
                mem.write(path, *text);
            }
            Self { mem, dir: tempfile::tempdir().unwrap() }
        }

        /// Pages over a vault; `disk` adds a disk cache (shared by all "runs"
        /// with this setup).
        pub(crate) fn pages(&self, disk: bool, delay: Duration) -> (Pages, Arc<FakePipeline>) {
            let vault = Vault::new(self.mem.clone());
            let versions = Versions::new(self.mem.clone());
            let pipeline = Arc::new(FakePipeline {
                vault: vault.clone(),
                versions: versions.clone(),
                builds: AtomicUsize::new(0),
                delay,
            });
            let disk = disk.then(|| DiskCache::new(self.dir.path(), "mem", "м".into()));
            let cache = PageCache::new(versions, disk, MEMORY_BUDGET);
            (Pages::new(vault, pipeline.clone(), cache), pipeline)
        }
    }

    fn id(s: &str) -> NoteId {
        NoteId::new(s).unwrap()
    }

    const OPTS: FigureOptions = FigureOptions { precision: Some(2) };

    fn body(page: &NotePage) -> &str {
        &page.rendered.as_ref().unwrap().body
    }

    #[test]
    fn builds_once_and_rebuilds_after_edit() {
        let s = Setup::new(&[("A.typ", "раз")]);
        let (pages, pipeline) = s.pages(true, Duration::ZERO);
        let first = pages.page(&id("A"), OPTS).unwrap();
        assert_eq!(body(&first), "раз|p2");
        assert!(Arc::ptr_eq(&first, &pages.page(&id("A"), OPTS).unwrap()));
        assert_eq!(pages.version(&id("A"), OPTS).unwrap(), first.version);
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 1);

        s.mem.write("A.typ", "два");
        assert_ne!(pages.version(&id("A"), OPTS).unwrap(), first.version, "the version without a build");
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 1);
        assert_eq!(body(&pages.page(&id("A"), OPTS).unwrap()), "два|p2");
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn error_is_shown_over_previous_render() {
        let s = Setup::new(&[("A.typ", "раз")]);
        let (pages, _) = s.pages(true, Duration::ZERO);
        pages.page(&id("A"), OPTS).unwrap();
        s.mem.write("A.typ", "ошибка");
        let page = pages.page(&id("A"), OPTS).unwrap();
        assert_eq!(page.errors.len(), 1);
        assert_eq!(body(&page), "раз|p2");

        // After a restart: the same picture, without a build.
        let (restarted, pipeline) = s.pages(true, Duration::ZERO);
        let page = restarted.page(&id("A"), OPTS).unwrap();
        assert_eq!((page.errors.len(), body(&page)), (1, "раз|p2"));
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 0, "an error is cached too");
    }

    #[test]
    fn new_error_without_previous_has_no_render() {
        let s = Setup::new(&[("A.typ", "ошибка")]);
        let (pages, _) = s.pages(false, Duration::ZERO);
        let page = pages.page(&id("A"), OPTS).unwrap();
        assert!(page.rendered.is_none() && page.errors.len() == 1);
    }

    #[test]
    fn options_change_does_not_rebuild() {
        let s = Setup::new(&[("A.typ", "раз")]);
        let (pages, pipeline) = s.pages(true, Duration::ZERO);
        let p2 = pages.page(&id("A"), OPTS).unwrap();
        let full = pages.page(&id("A"), FigureOptions { precision: None }).unwrap();
        assert_eq!(body(&full), "раз|full");
        assert_ne!(p2.version, full.version, "the client will request the page again");
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn prebuild_goes_to_disk_and_skips_built() {
        let s = Setup::new(&[("A.typ", "раз")]);
        let (pages, pipeline) = s.pages(true, Duration::ZERO);
        assert!(pages.prebuild(&id("A")).unwrap());
        assert!(!pages.prebuild(&id("A")).unwrap(), "a built note is not built again");
        assert_eq!(pages.cache().memory().0, 0, "warming goes to disk only");
        assert!(pages.is_built(&id("A")));
        assert_eq!(body(&pages.page(&id("A"), OPTS).unwrap()), "раз|p2");
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 1);
        assert_eq!(pages.last_build(&id("A")), Some(Duration::from_millis(5)));
    }

    #[test]
    fn concurrent_requests_share_one_build() {
        let s = Setup::new(&[("A.typ", "раз")]);
        let (pages, pipeline) = s.pages(false, Duration::from_millis(100));
        let (a, b) = std::thread::scope(|scope| {
            let a = scope.spawn(|| pages.page(&id("A"), OPTS).unwrap());
            let b = scope.spawn(|| pages.page(&id("A"), OPTS).unwrap());
            (a.join().unwrap(), b.join().unwrap())
        });
        assert!(Arc::ptr_eq(&a, &b), "the second request waited for the first build");
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn different_notes_build_in_parallel() {
        let s = Setup::new(&[("A.typ", "раз"), ("B.typ", "два")]);
        let (pages, pipeline) = s.pages(false, Duration::from_millis(300));
        let started = std::time::Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(|| pages.page(&id("A"), OPTS).unwrap());
            scope.spawn(|| pages.page(&id("B"), OPTS).unwrap());
        });
        assert!(started.elapsed() < Duration::from_millis(550), "not one by one: {:?}", started.elapsed());
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 2);
        assert!(pages.building.lock().len() <= 2);
    }

    #[test]
    fn missing_note_is_not_found() {
        let s = Setup::new(&[]);
        let (pages, _) = s.pages(false, Duration::ZERO);
        assert!(matches!(pages.page(&id("Нет"), OPTS), Err(crate::Error::NotFound(_))));
    }

    #[test]
    fn book_is_an_entry() {
        let s = Setup::new(&[("Книга/main.typ", "книга"), ("Книга/01.typ", "")]);
        let (pages, _) = s.pages(false, Duration::ZERO);
        assert_eq!(pages.page(&id("Книга"), OPTS).unwrap().kind, NoteKind::Book);
    }

    #[test]
    fn drop_pages_drops_pipeline() {
        let s = Setup::new(&[]);
        let (pages, pipeline) = s.pages(false, Duration::ZERO);
        let weak = Arc::downgrade(&pipeline);
        drop(pipeline);
        drop(pages);
        std::thread::sleep(Duration::from_millis(250));
        assert!(weak.upgrade().is_none(), "the release thread must not hold the pipeline");
    }
}
