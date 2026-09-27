//! Заметки как сервис — фасад ядра для сервера и CLI.
//!
//! Слои (каждый со своими тестами, без компиляции Typst):
//!
//! - [`crate::storage`] — файлы хранилища ([`Storage`]: каталог на диске, в
//!   тестах — память);
//! - [`crate::pipeline`] — сборка: исходник → компиляция по темам →
//!   отрисовка, обработка рисунков;
//! - [`crate::page_cache`] — кэш страниц: память (LRU) + диск
//!   ([`crate::cache`]), версии по файлам ([`crate::version`]);
//! - [`crate::pages`] — страница по запросу: сборки по одной, ошибка поверх
//!   прежней отрисовки;
//! - [`crate::warm`] — прогрев поверх `pages`.
//!
//! Здесь они собираются вместе; сюда же — индекс исходников (ссылки,
//! поиск), PDF и страницы статического сайта. Наблюдатель файлов
//! ([`crate::watch`]) включает только сервер ([`Notes::watch`]): индекс не
//! обходит хранилище без изменений, прогрев просыпается от них; CLI
//! обходится без него.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::cache::DiskCache;
use crate::diag::Diagnostic;
use crate::figures::FigureOptions;
use crate::fonts::Fonts;
use crate::graph::{Snapshot, SourceIndex};
use crate::page_cache::{MEMORY_BUDGET, PageCache};
use crate::pages::Pages;
use crate::pipeline::{Pipeline, TypstPipeline};
use crate::storage::Storage;
use crate::themes::ThemeSet;
use crate::vault::{Entry, NoteId, Vault};
use crate::vault_data::VaultData;
use crate::vault_graph::{GraphData, GraphFilter, GraphLayout, Layouts};
use crate::version::Versions;
use crate::warm::{RESCAN, RESCAN_UNWATCHED, WarmStats, Warmer};
use crate::watch::{Change, Changes};
use crate::world::{Compiler, LibrarySource};
use crate::{Error, Result};

pub use crate::pages::NotePage;
pub use crate::pipeline::{LinkStyle, encode, static_path};

#[derive(Debug, Clone)]
pub struct NotesConfig {
    /// Корень хранилища.
    pub vault: PathBuf,
    /// Библиотека оформления (`baluk/`), видна заметкам как `/_baluk/`.
    pub library: LibrarySource,
    /// Дополнительные каталоги шрифтов (к системным и встроенным в Typst).
    pub font_dirs: Vec<PathBuf>,
    /// Каталог кэша на диске (`<данные>/cache`, `None` — только в памяти):
    /// `pages/` — отрисовка заметок, `fonts/` — части шрифтов для браузера.
    pub cache: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Notes {
    pages: Pages,
    typst: Arc<TypstPipeline>,
    links: Arc<SourceIndex>,
    layouts: Arc<Layouts>,
    warmer: Arc<Warmer>,
    changes: Arc<Changes>,
}

impl Notes {
    /// Хранилище — каталог `config.vault`.
    pub fn open(config: &NotesConfig) -> Result<Self> {
        Self::with_vault(Vault::open(&config.vault)?, config)
    }

    /// Хранилище — любой [`Storage`] (`config.vault` не используется).
    pub fn with_storage(storage: Arc<dyn Storage>, config: &NotesConfig) -> Result<Self> {
        Self::with_vault(Vault::new(storage), config)
    }

    fn with_vault(vault: Vault, config: &NotesConfig) -> Result<Self> {
        let library = match &config.library {
            LibrarySource::Dir(dir) => LibrarySource::Dir(fs::canonicalize(dir).map_err(|e| Error::io(dir, e))?),
            LibrarySource::Embedded => LibrarySource::Embedded,
        };
        if !library.is_valid() {
            return Err(Error::Library(format!("в библиотеке {library:?} нет lib.typ")));
        }
        let fonts =
            Arc::new(Fonts::load(&config.font_dirs).with_web_cache(config.cache.as_ref().map(|c| c.join("fonts"))));
        let links = Arc::new(SourceIndex::default());
        let changes = Arc::new(Changes::default());
        links.set_changes(changes.clone());
        let layouts = Arc::new(Layouts::default());
        // Данные хранилища для заметок: `/_vault/<префикс>/…`.
        let data = VaultData::new().with(
            crate::vault_graph::DATA_PREFIX,
            GraphData { vault: vault.clone(), index: links.clone(), layouts: layouts.clone() },
        );
        let versions = Versions::new(vault.storage().clone()).with_data(data);
        let stamp = crate::cache::stamp(&[library.fingerprint(), fonts.fingerprint()]);
        let compiler = Compiler::new(versions.clone(), library, fonts);
        let themes = ThemeSet::load(&compiler)?;
        let disk = config.cache.as_ref().map(|dir| DiskCache::new(dir.join("pages"), &vault.location(), stamp));
        let cache = PageCache::new(versions, disk, MEMORY_BUDGET);
        let typst = Arc::new(TypstPipeline::new(vault.clone(), compiler, themes));
        let pages = Pages::new(vault, typst.clone(), cache);
        // Индекс ссылок дополняется ссылками собранных страниц (вычисляемые
        // пути). Слабая ссылка: кэш страниц сам держит индекс через граф.
        let cache = Arc::downgrade(pages.cache());
        links.set_built(Box::new(move |id: &NoteId| cache.upgrade()?.links(id)));
        let warmer = Arc::new(Warmer::default());
        let weak = Arc::downgrade(&warmer);
        changes.subscribe(move |_| {
            if let Some(w) = weak.upgrade() {
                w.poke();
            }
        });
        Ok(Self { pages, typst, links, layouts, warmer, changes })
    }

    pub fn vault(&self) -> &Vault {
        self.pages.vault()
    }

    pub fn themes(&self) -> &ThemeSet {
        self.typst.themes()
    }

    pub fn fonts(&self) -> &Fonts {
        self.typst.fonts()
    }

    pub fn entries(&self) -> Result<Vec<Entry>> {
        self.vault().entries()
    }

    /// Индекс исходников (без компиляции): ссылки, граф, названия и теги,
    /// разделы для поиска.
    pub fn index(&self) -> Result<Snapshot> {
        self.links.snapshot(self.vault())
    }

    /// Граф хранилища по фильтру, разложенный (раскладка — из кэша, если
    /// такой граф уже раскладывали).
    pub fn graph_layout(&self, filter: &GraphFilter) -> Result<GraphLayout> {
        Ok((*self.index()?.graph_layout_cached(filter, &self.layouts)).clone())
    }

    /// Превью заметки (и раздела) для подсказки при наведении на ссылку.
    pub fn preview(&self, id: &NoteId, anchor: Option<&str>) -> Result<crate::search::Preview> {
        crate::search::preview(&self.index()?, id, anchor).ok_or_else(|| Error::NotFound(id.to_string()))
    }

    /// Поиск по тексту всех заметок.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<crate::search::SearchHit>> {
        Ok(crate::search::search(&self.index()?, query, limit))
    }

    /// Индекс поиска заметок, для которых `keep` (статический сайт — собранных).
    pub fn search_documents(&self, keep: impl Fn(&NoteId) -> bool) -> Result<Vec<crate::search::SearchDoc>> {
        Ok(crate::search::documents(&self.index()?, keep))
    }

    /// Поиск в одной заметке (книге): все разделы по порядку текста.
    pub fn search_in(&self, id: &NoteId, query: &str, limit: usize) -> Result<Vec<crate::search::SearchHit>> {
        let index = self.index()?;
        if !index.exists(id.as_str()) {
            return Err(Error::NotFound(id.to_string()));
        }
        Ok(crate::search::search_in(&index, id, query, limit))
    }

    /// Страница заметки для сервера: из кэша, если её файлы не менялись
    /// (см. [`Pages::page`]).
    pub fn page(&self, id: &NoteId, opts: FigureOptions) -> Result<Arc<NotePage>> {
        self.pages.page(id, opts)
    }

    /// Текущая версия заметки. Для уже собранной — только `stat` её файлов,
    /// без компиляции; для новой — собирает.
    pub fn version(&self, id: &NoteId, opts: FigureOptions) -> Result<String> {
        self.pages.version(id, opts)
    }

    /// Заметка в PDF (вид PDF из baluk) в теме `theme`; без кэша.
    pub fn pdf(&self, id: &NoteId, theme: &str) -> Result<std::result::Result<Vec<u8>, Vec<Diagnostic>>> {
        let entry = self.vault().entry(id)?;
        if !self.themes().names().iter().any(|t| t == theme) {
            return Err(Error::Setting { key: "тема".into(), reason: format!("нет темы «{theme}»") });
        }
        Ok(self.typst.compiler().compile_pdf(&entry.main, theme))
    }

    /// Страница для статического сайта: без кэша, относительные ссылки.
    pub fn page_static(&self, entry: &Entry, opts: FigureOptions) -> NotePage {
        let built = self.typst.build(entry, LinkStyle::Static);
        NotePage {
            id: entry.id.clone(),
            kind: entry.kind,
            version: crate::page_cache::page_version(&built.files, opts),
            rendered: built.raw.map(|raw| Arc::new(self.typst.finish(&raw, opts))),
            errors: built.errors,
            warnings: built.warnings,
            book: None,
        }
    }

    // ── Прогрев (см. crate::warm) ─────────────────────────────────────────

    /// Подсказать прогреву, что собрать первым (заметки во вкладках, недавние).
    pub fn hint_warm(&self, ids: Vec<NoteId>) {
        self.warmer.hint(ids);
    }

    /// Один проход прогрева: собрать всё несобранное по порядку.
    pub fn warm_pass(&self) -> WarmStats {
        self.warmer.pass(&self.pages)
    }

    /// Прогревать бесконечно (фоновый поток сервера).
    pub fn warm_forever(&self) -> ! {
        let changes = self.changes.clone();
        self.warmer.forever(&self.pages, move || if changes.watching() { RESCAN } else { RESCAN_UNWATCHED })
    }

    // ── Изменения хранилища (см. crate::watch) ─────────────────────────────

    /// Включить наблюдатель файлов хранилища (сервер). `false` — хранилище
    /// не умеет или не вышло: всё работает обходом, как без него.
    pub fn watch(&self) -> bool {
        self.changes.start(&**self.vault().storage())
    }

    /// Работает ли наблюдатель.
    pub fn watching(&self) -> bool {
        self.changes.watching()
    }

    /// Звать `f` с каждой пачкой изменений хранилища (из потока наблюдателя).
    pub fn on_change(&self, f: impl Fn(&Change) + Send + Sync + 'static) {
        self.changes.subscribe(f);
    }

    /// Сколько страниц и байт держит кэш в памяти.
    pub fn memory(&self) -> (usize, usize) {
        self.pages.cache().memory()
    }
}
