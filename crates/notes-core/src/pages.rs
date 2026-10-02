//! Страницы по запросу: сборка ([`Pipeline`]) поверх кэша ([`PageCache`]).
//!
//! Заметка собирается, когда её запросили (или заранее — прогрев,
//! [`crate::warm`]). Сборки **разных** заметок идут параллельно (у каждой
//! свой кэш файлов компилятора, [`crate::world`]): открыть заметку, пока
//! собирается книга, можно сразу. Сборка одной заметки — одна (замок
//! заметки в `building`): второй запрос той же заметки, пришедший во время
//! её сборки, ждёт первую и берёт результат из кэша, а не собирает заново.
//! Ждущий запрос пользователя — сигнал прогреву не начинать новую сборку
//! ([`Pages::wait_for_users`]): процессор — сначала читателю.
//!
//! Ошибка компиляции показывается поверх последней удачной отрисовки.
//! Версия страницы — версия файлов + настройки обработки рисунков: смена
//! настроек не перекомпилирует заметку, а клиент перезапросит страницу сам.

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

/// Заметка, готовая к показу.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NotePage {
    pub id: NoteId,
    pub kind: NoteKind,
    /// Версия файлов заметки и настроек отрисовки.
    pub version: String,
    /// Последняя удачная отрисовка. При ошибке компиляции — предыдущая
    /// удачная (если была): читатель видит заметку и ошибку поверх неё.
    pub rendered: Option<Arc<Rendered>>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// Книга по главам: какая глава в `rendered` и где остальные. Только у
    /// ответа на запрос главы ([`crate::book::chapter_page`]); у страницы
    /// целиком — `None`.
    pub book: Option<crate::book::BookView>,
}

#[derive(Debug)]
pub struct Pages {
    vault: Vault,
    pipeline: Arc<dyn Pipeline>,
    cache: Arc<PageCache>,
    /// Замки заметок: держится на время сборки этой заметки.
    building: Mutex<HashMap<NoteId, Arc<Mutex<()>>>>,
    /// Сколько запросов страниц сейчас ждут: прогрев им уступает.
    waiting: AtomicUsize,
    /// Состояние релиза памяти после серии сборок.
    releaser: Arc<Mutex<ReleaserState>>,
}

/// E5: релиз даёт около +3 мс к следующей пересборке; 5 с - компромисс,
/// который заметно снижает idle RSS и срабатывает в обычной работе.
const IDLE_RELEASE_DELAY: Duration = Duration::from_secs(5);
const RELEASE_POLL_DELAY: Duration = Duration::from_millis(200);

#[derive(Debug, Default)]
struct ReleaserState {
    /// Была ли сборка после последнего релиза.
    pending_release: bool,
    /// Последняя запись сборки в кэш.
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
        // Один дебаунсер на Pages: релиз памяти по простоям.
        pages.spawn_releaser();
        pages
    }

    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    pub fn cache(&self) -> &Arc<PageCache> {
        &self.cache
    }

    /// Страница заметки: из кэша, если её файлы не менялись.
    pub fn page(&self, id: &NoteId, opts: FigureOptions) -> Result<Arc<NotePage>> {
        /// Запрос ждёт страницу — прогрев не начнёт новую сборку.
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
        Ok(self.store(&entry, built, Some(opts)).expect("страница с настройками"))
    }

    /// Текущая версия заметки. Для известной — только `stat` её файлов,
    /// без компиляции; для новой — собирает.
    pub fn version(&self, id: &NoteId, opts: FigureOptions) -> Result<String> {
        if let Some(version) = self.cache.version(id, opts) {
            return Ok(version);
        }
        Ok(self.page(id, opts)?.version.clone())
    }

    /// Есть ли у заметки годная сборка — в памяти или на диске.
    pub fn is_built(&self, id: &NoteId) -> bool {
        self.cache.is_fresh(id)
    }

    /// Сколько длилась прошлая сборка заметки.
    pub fn last_build(&self, id: &NoteId) -> Option<Duration> {
        self.cache.build_ms(id).map(Duration::from_millis)
    }

    /// Подождать, пока запросы пользователя получат свои страницы.
    pub fn wait_for_users(&self) {
        while self.waiting.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Собрать заметку заранее (прогрев): в кэш на диске, без обработки
    /// рисунков и без страницы в памяти. Уже собранная — пропускается.
    /// `true` — собрана сейчас.
    pub fn prebuild(&self, id: &NoteId) -> Result<bool> {
        let entry = self.vault.entry(id)?;
        let lock = self.note_lock(id);
        let _building = lock.lock();
        if self.cache.is_fresh(id) {
            return Ok(false);
        }
        let built = self.pipeline.build(&entry, Priority::Background);
        self.store(&entry, built, None);
        Ok(true)
    }

    /// Освободить память сборок (после прохода прогрева).
    pub fn release_memory(&self) {
        self.pipeline.release_memory();
    }

    /// Замок сборки заметки. Замки несобираемых заметок убираются, чтобы
    /// карта не росла.
    fn note_lock(&self, id: &NoteId) -> Arc<Mutex<()>> {
        let mut locks = self.building.lock();
        locks.retain(|_, l| Arc::strong_count(l) > 1);
        locks.entry(id.clone()).or_default().clone()
    }

    /// Запомнить сборку. Ошибка — под ней прежняя удачная отрисовка.
    /// С настройками `opts` — страница (она же остаётся в памяти).
    fn store(&self, entry: &Entry, built: Build, opts: Option<FigureOptions>) -> Option<Arc<NotePage>> {
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
        // Ссылки показанной отрисовки — для индекса ссылок (вычисляемые пути).
        let links = shown.as_ref().map(|r| r.links.clone()).unwrap_or_default();
        let page = opts.map(|opts| {
            let page = NotePage {
                id: entry.id.clone(),
                kind: entry.kind,
                version: page_version(&built.files, opts),
                rendered: shown.map(|r| Arc::new(self.pipeline.finish(&r, opts))),
                errors: built.errors.clone(),
                warnings: built.warnings.clone(),
                book: None,
            };
            (Arc::new(page), opts)
        });
        let record = Record {
            files: built.files,
            deps: built.deps,
            errors: built.errors,
            warnings: built.warnings,
            build_ms: u64::try_from(built.took.as_millis()).unwrap_or(u64::MAX),
            raw: None,
            links,
        };
        self.cache.store(&entry.id, record, raw, page.clone());
        self.note_built();
        page.map(|(p, _)| p)
    }

    /// Отложенный выпуск памяти: отметить сборку (дебаунсер увидит простой).
    fn note_built(&self) {
        let mut state = self.releaser.lock();
        state.pending_release = true;
        state.last_build = Some(Instant::now());
    }

    /// Один рабочий поток на `Pages`, проверяет простой и вызывает релиз.
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

    /// Сборка без Typst: отрисовка — текст главного файла, `ошибка` в
    /// тексте — ошибка компиляции.
    #[derive(Debug)]
    pub(crate) struct FakePipeline {
        pub vault: Vault,
        pub versions: Versions,
        pub builds: AtomicUsize,
        /// Задержка сборки (проверка одновременных запросов).
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
                };
                (Some(raw), vec![])
            };
            Build { raw, errors, warnings: vec![], deps, files, took: Duration::from_millis(text_len(&main)) }
        }

        fn finish(&self, raw: &Rendered, opts: FigureOptions) -> Rendered {
            Rendered { body: format!("{}|{}", raw.body, opts.key()), ..raw.clone() }
        }
    }

    /// «Время сборки» в тестах — длина пути, чтобы порядок был предсказуем.
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

        /// Страницы поверх хранилища; `disk` — с кэшем на диске (общим для
        /// всех «запусков» этой настройки).
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
        assert_ne!(pages.version(&id("A"), OPTS).unwrap(), first.version, "версия — без сборки");
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

        // После перезапуска — та же картина, без сборки.
        let (restarted, pipeline) = s.pages(true, Duration::ZERO);
        let page = restarted.page(&id("A"), OPTS).unwrap();
        assert_eq!((page.errors.len(), body(&page)), (1, "раз|p2"));
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 0, "ошибка тоже в кэше");
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
        assert_ne!(p2.version, full.version, "клиент перезапросит страницу");
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn prebuild_goes_to_disk_and_skips_built() {
        let s = Setup::new(&[("A.typ", "раз")]);
        let (pages, pipeline) = s.pages(true, Duration::ZERO);
        assert!(pages.prebuild(&id("A")).unwrap());
        assert!(!pages.prebuild(&id("A")).unwrap(), "собранное не собирается");
        assert_eq!(pages.cache().memory().0, 0, "прогрев — только на диск");
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
        assert!(Arc::ptr_eq(&a, &b), "второй запрос дождался первой сборки");
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
        assert!(started.elapsed() < Duration::from_millis(550), "не по очереди: {:?}", started.elapsed());
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
        assert!(weak.upgrade().is_none(), "поток релиза не должен держать pipeline");
    }
}
