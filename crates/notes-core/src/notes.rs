//! Notes as a service: the facade of the core for the server and the CLI.
//!
//! Layers (each with its own tests, without compiling Typst):
//!
//! - [`crate::storage`]: vault files ([`Storage`]: a directory on disk, in
//!   tests - memory);
//! - [`crate::pipeline`]: the build - source -> compilation per theme ->
//!   rendering, figure processing;
//! - [`crate::page_cache`]: the page cache - memory (LRU) + disk
//!   ([`crate::cache`]), versions by files ([`crate::version`]);
//! - [`crate::pages`]: a page on request - builds one at a time, an error
//!   on top of the previous rendering;
//! - [`crate::warm`]: warming on top of `pages`.
//!
//! Here they come together, plus the source index (links, search) and PDF.
//! Only the server starts the file watcher ([`crate::watch`],
//! [`Notes::watch`]): the index does not scan an unchanged vault, warming
//! wakes up on changes; the CLI does without it.

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
use crate::pipeline::TypstPipeline;
use crate::storage::Storage;
use crate::themes::ThemeSet;
use crate::vault::{Entry, NoteId, NoteKind, Vault};
use crate::vault_data::VaultData;
use crate::vault_graph::{GraphData, GraphFilter, GraphLayout, Layouts};
use crate::version::Versions;
use crate::warm::{RESCAN, RESCAN_UNWATCHED, WarmStats, Warmer};
use crate::watch::{Change, Changes};
use crate::world::{Compiler, LibrarySource};
use crate::{Error, Result};

pub use crate::pages::NotePage;
pub use crate::pipeline::encode;

/// How to open a vault.
#[derive(Debug, Clone)]
pub struct NotesConfig {
    /// Vault root.
    pub vault: PathBuf,
    /// The styling library (`baluk/`), seen by notes as `/_baluk/`.
    pub library: LibrarySource,
    /// Extra font directories (on top of the system ones and those built into Typst).
    pub font_dirs: Vec<PathBuf>,
    /// Disk cache directory (`<data>/cache`, `None` - memory only): `pages/`
    /// for rendered notes, `fonts/` for font chunks for the browser.
    pub cache: Option<PathBuf>,
    /// Where deleted notes go ([`Notes::delete`]): `None` - the system trash
    /// (they can be restored there), a directory - into it (tests).
    pub trash: Option<PathBuf>,
}

/// Resources shared by several `Notes` (the server: the library core and
/// the vaults). Themes depend only on the library and the fonts, not on the
/// vault.
#[derive(Debug, Clone, Default)]
pub struct SharedAssets {
    /// Loaded fonts.
    pub fonts: Option<Arc<Fonts>>,
    /// Themes of the library.
    pub themes: Option<Arc<ThemeSet>>,
}

/// An open vault: pages, index, graph, warming and the watcher.
#[derive(Debug)]
pub struct Notes {
    pages: Pages,
    typst: Arc<TypstPipeline>,
    links: Arc<SourceIndex>,
    layouts: Arc<Layouts>,
    warmer: Arc<Warmer>,
    changes: Arc<Changes>,
    /// Library directory on disk (`--library`, a debug build): the watcher
    /// watches it too.
    library_dir: Option<PathBuf>,
    /// Vault directory on disk ([`Notes::open`]); `None` in memory.
    dir: Option<PathBuf>,
    /// Which Typst packages may be imported (a device setting).
    packages: Arc<crate::packages::PackagePolicy>,
}

impl Notes {
    /// The vault is the directory `config.vault`.
    pub fn open(config: &NotesConfig) -> Result<Self> {
        Self::open_with_shared(config, &SharedAssets::default())
    }

    /// The same, with shared fonts and themes (the server).
    pub fn open_with_shared(config: &NotesConfig, shared: &SharedAssets) -> Result<Self> {
        let root = &config.vault;
        let storage = crate::storage::DirStorage::open(root).map_err(|e| Error::io(root, e))?;
        let storage = storage.with_trash(config.trash.clone());
        let mut notes = Self::with_vault(Vault::new(Arc::new(storage)), config, shared)?;
        notes.dir = Some(root.clone());
        Ok(notes)
    }

    /// The vault is any [`Storage`] (`config.vault` is not used).
    pub fn with_storage(storage: Arc<dyn Storage>, config: &NotesConfig) -> Result<Self> {
        Self::with_vault(Vault::new(storage), config, &SharedAssets::default())
    }

    fn with_vault(vault: Vault, config: &NotesConfig, shared: &SharedAssets) -> Result<Self> {
        let library = match &config.library {
            LibrarySource::Dir(dir) => LibrarySource::Dir(fs::canonicalize(dir).map_err(|e| Error::io(dir, e))?),
            LibrarySource::Embedded => LibrarySource::Embedded,
        };
        if !library.is_valid() {
            return Err(Error::Library(format!("library {library:?} has no lib.typ")));
        }
        let fonts = shared.fonts.clone().unwrap_or_else(|| {
            Arc::new(Fonts::load(&config.font_dirs).with_web_cache(config.cache.as_ref().map(|c| c.join("fonts"))))
        });
        let links = Arc::new(SourceIndex::default());
        let changes = Arc::new(Changes::default());
        links.set_changes(changes.clone());
        let layouts = Arc::new(Layouts::default());
        let packages = Arc::new(crate::packages::PackagePolicy::default());
        // Vault data for notes: `/_vault/<prefix>/...`.
        let data = VaultData::new()
            .with(crate::packages::DATA_PREFIX, crate::packages::PolicyData(packages.clone()))
            .with(
                crate::vault_graph::DATA_PREFIX,
                GraphData { vault: vault.clone(), index: links.clone(), layouts: layouts.clone() },
            )
            .with(crate::graph::TITLE_PREFIX, crate::graph::TitleData { vault: vault.clone(), index: links.clone() });
        let versions = Versions::new(vault.storage().clone()).with_data(data);
        let library_dir = match &library {
            LibrarySource::Dir(dir) => Some(dir.clone()),
            LibrarySource::Embedded => None,
        };
        let stamp = crate::cache::stamp(&[library.fingerprint(), fonts.fingerprint()]);
        let compiler = Compiler::new(versions.clone(), library, fonts).with_packages(packages.clone());
        let themes = match shared.themes.clone() {
            Some(themes) => themes,
            None => Arc::new(ThemeSet::load(&compiler)?),
        };
        let disk = config.cache.as_ref().map(|dir| DiskCache::new(dir.join("pages"), &vault.location(), stamp));
        let cache = PageCache::new(versions, disk, MEMORY_BUDGET);
        let typst = Arc::new(TypstPipeline::new(vault.clone(), compiler, themes));
        let pages = Pages::new(vault, typst.clone(), cache);
        // The link index adds the links of built pages (computed paths). A
        // weak reference: the page cache holds the index itself via the graph.
        let cache = Arc::downgrade(pages.cache());
        links.set_built(Box::new(move |id: &NoteId| cache.upgrade()?.links(id)));
        let warmer = Arc::new(Warmer::default());
        let weak = Arc::downgrade(&warmer);
        changes.subscribe(move |_| {
            if let Some(w) = weak.upgrade() {
                w.poke();
            }
        });
        Ok(Self { pages, typst, links, layouts, warmer, changes, library_dir, dir: None, packages })
    }

    /// The vault files.
    pub fn vault(&self) -> &Vault {
        self.pages.vault()
    }

    /// Vault directory on disk; `None` for a vault in memory.
    pub fn dir(&self) -> Option<&std::path::Path> {
        self.dir.as_deref()
    }

    /// Themes of the library.
    pub fn themes(&self) -> &ThemeSet {
        self.typst.themes()
    }

    /// Fonts and themes of this core, for other cores with the same library.
    pub fn shared_assets(&self) -> SharedAssets {
        SharedAssets { fonts: Some(self.typst.compiler().fonts().clone()), themes: Some(self.typst.themes_arc()) }
    }

    /// Fonts for Typst and the browser.
    pub fn fonts(&self) -> &Fonts {
        self.typst.fonts()
    }

    /// Notes and books of the vault.
    pub fn entries(&self) -> Result<Vec<Entry>> {
        self.vault().entries()
    }

    /// The source index (no compiling): links, graph, titles and tags,
    /// sections for search.
    pub fn index(&self) -> Result<Snapshot> {
        self.links.snapshot(self.vault())
    }

    /// The vault graph by the filter, laid out (the layout comes from the
    /// cache if this graph was laid out before).
    pub fn graph_layout(&self, filter: &GraphFilter) -> Result<GraphLayout> {
        Ok((*self.index()?.graph_layout_cached(filter, &self.layouts)).clone())
    }

    /// Preview of a note (and a section) for the tooltip over a link.
    pub fn preview(&self, id: &NoteId, anchor: Option<&str>) -> Result<crate::search::Preview> {
        crate::search::preview(&self.index()?, id, anchor).ok_or_else(|| Error::NotFound(id.to_string()))
    }

    /// Full-text search over all notes.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<crate::search::SearchHit>> {
        Ok(crate::search::search(&self.index()?, query, limit))
    }

    /// Search in one note (book): all sections in text order.
    pub fn search_in(&self, id: &NoteId, query: &str, limit: usize) -> Result<Vec<crate::search::SearchHit>> {
        let index = self.index()?;
        if !index.exists(id.as_str()) {
            return Err(Error::NotFound(id.to_string()));
        }
        Ok(crate::search::search_in(&index, id, query, limit))
    }

    /// The note page for the server: from the cache if its files did not
    /// change (see [`Pages::page`]).
    pub fn page(&self, id: &NoteId, opts: FigureOptions) -> Result<Arc<NotePage>> {
        self.pages.page(id, opts)
    }

    /// The current note version. For a built note only the `stat` of its
    /// files, no compiling; a new one gets built.
    pub fn version(&self, id: &NoteId, opts: FigureOptions) -> Result<String> {
        self.pages.version(id, opts)
    }

    /// The note as PDF (the PDF look of baluk) in the theme `theme`; no cache.
    pub fn pdf(&self, id: &NoteId, theme: &str) -> Result<std::result::Result<Vec<u8>, Vec<Diagnostic>>> {
        let entry = self.vault().entry(id)?;
        if !self.themes().names().iter().any(|t| t == theme) {
            return Err(Error::Setting { key: "theme".into(), reason: format!("no theme \"{theme}\"") });
        }
        Ok(self.typst.compiler().compile_pdf(&entry.main, theme))
    }

    /// Deletes a note or a book (the whole folder) to the trash
    /// ([`Storage::trash`]). The note list and the link index update at
    /// once, without waiting for the watcher.
    pub fn delete(&self, id: &NoteId) -> Result<()> {
        let entry = self.vault().entry(id)?;
        let path = match entry.kind {
            NoteKind::Note => entry.main.to_string_lossy().replace('\\', "/"),
            NoteKind::Book => id.as_str().to_owned(),
        };
        self.vault().storage().trash(&path).map_err(|e| self.vault().io_error(&path, e))?;
        tracing::info!("moved to trash: {}", self.vault().storage().display(&path).display());
        self.changes.local(vec![path]);
        Ok(())
    }

    /// Deletes a vault folder entirely (subfolders, notes, books) to the
    /// trash. The path is checked by the rules of [`NoteId`]; not a folder -
    /// `NotFound`.
    pub fn delete_folder(&self, path: &NoteId) -> Result<()> {
        let path = path.as_str();
        let storage = self.vault().storage();
        if !storage.stat(path).is_ok_and(|m| m.is_dir) {
            return Err(Error::NotFound(path.to_owned()));
        }
        storage.trash(path).map_err(|e| self.vault().io_error(path, e))?;
        tracing::info!("moved to trash: {}", storage.display(path).display());
        self.changes.local(vec![path.to_owned()]);
        Ok(())
    }

    /// Renames a note, book or folder ([`crate::rename`]): the title, the
    /// file name and the links to it. `apply` = `false` - only the plan.
    pub fn rename(
        &self,
        kind: crate::rename::RenameKind,
        from: &NoteId,
        title: &str,
        apply: bool,
    ) -> Result<crate::rename::RenamePlan> {
        if !apply {
            return crate::rename::plan(self.vault(), kind, from, title);
        }
        let (plan, changed) = crate::rename::apply(self.vault(), kind, from, title)?;
        self.changes.local(changed);
        Ok(plan)
    }

    // -- Warming (see crate::warm) --------------------------------------------

    /// Applies the device settings on the fly: the number of builds, Typst
    /// memory, cache limits, the warming mode.
    pub fn apply_device(&self, device: &crate::settings::Device) {
        let compiler = self.typst.compiler();
        compiler.set_parallel(device.builds);
        compiler.set_memo(device.memo);
        self.pages.cache().set_limits(device.memory, device.disk);
        self.warmer.set_mode(device.warm);
        if self.packages.set_extra(&device.packages) {
            tracing::info!(packages = ?device.packages, "packages on top of the whitelist");
        }
    }

    /// Compresses font chunks for the browser in advance (background at
    /// server start) and cleans their disk cache.
    pub fn warm_fonts(&self) {
        let families = self.themes().web_fonts();
        self.fonts().warm_web(families);
        let removed = self.fonts().prune_web(families, self.pages.cache().disk_limits().foreign_ttl);
        if removed > 0 {
            tracing::info!(removed, "font cache cleaned");
        }
    }

    /// Tells warming what to build first (notes in tabs, recent ones).
    pub fn hint_warm(&self, ids: Vec<NoteId>) {
        self.warmer.hint(ids);
    }

    /// One warming pass: builds everything unbuilt in order.
    pub fn warm_pass(&self) -> WarmStats {
        self.warmer.pass(&self.pages)
    }

    /// Warms until the vault is closed ([`Notes::close`]; a server background thread).
    pub fn warm_forever(&self) {
        let changes = self.changes.clone();
        self.warmer.forever(&self.pages, move || if changes.watching() { RESCAN } else { RESCAN_UNWATCHED });
    }

    // -- Vault changes (see crate::watch) -------------------------------------

    /// Starts the watcher of the vault files (the server), and of the library
    /// directory if it is on disk. `false`: the storage cannot watch or it
    /// failed; everything works by scanning, as without it.
    pub fn watch(&self) -> bool {
        if !self.changes.start(&**self.vault().storage()) {
            return false;
        }
        // An edit of the library is a change too: notes rebuild by themselves.
        if let Some(dir) = &self.library_dir {
            match crate::storage::DirStorage::open(dir) {
                Ok(lib) => {
                    self.changes.also(&lib, crate::world::LIB_DIR);
                }
                Err(e) => tracing::warn!("library {}: {e}", dir.display()),
            }
        }
        true
    }

    /// Closes the vault (it is being renamed or deleted): warming and the
    /// watcher stop. No requests go to a closed vault.
    pub fn close(&self) {
        self.warmer.stop();
        self.changes.stop();
    }

    /// Whether the watcher works.
    pub fn watching(&self) -> bool {
        self.changes.watching()
    }

    /// Calls `f` with every batch of vault changes (from the watcher thread).
    pub fn on_change(&self, f: impl Fn(&Change) + Send + Sync + 'static) {
        self.changes.subscribe(f);
    }

    /// How many pages and bytes the memory cache holds.
    pub fn memory(&self) -> (usize, usize) {
        self.pages.cache().memory()
    }
}
