//! The Typst compiler over a vault.
//!
//! - The Typst project root is the vault root.
//! - `/_baluk/...` is a **virtual** directory: the files come from the app's
//!   design library, there is no copy in the vault. The library is a directory
//!   on disk (development: edits show at once and change note versions) or a
//!   copy of `baluk/` embedded in the binary (release: the binary is
//!   self-contained).
//! - `/_vault/...` is **vault data** for notes (the graph: `/_vault/graph/...`),
//!   served on the fly by the providers of [`crate::vault_data::VaultData`];
//!   such a file enters the note version as the provider's fingerprint (see
//!   [`crate::vault_data`]).
//! - Vault files are read through [`Storage`]; reading records the file's
//!   fingerprint ([`Versions::token`]), and the note version comes from them.
//! - Packages (`@preview/cetz`) come from the Typst cache and are downloaded
//!   if missing.
//! - The theme is passed as the input `theme` (`sys.inputs.theme`): each theme
//!   has its own Typst standard library, created once.
//!
//! [`FileStore`] caches files: a repeated compile parses incrementally, and the
//! list of files read gives the note version.

use std::any::Any;
use std::collections::HashMap;
use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ecow::eco_format;
use parking_lot::{Condvar, Mutex, RwLock};
use rust_embed::RustEmbed;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Dict, Duration, IntoValue};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Features, Library, LibraryExt, World};
use typst_html::HtmlDocument;
use typst_kit::datetime::Time;
use typst_kit::downloader::SystemDownloader;
use typst_kit::files::{FileLoader, FileStore};
use typst_kit::packages::SystemPackages;
use typst_layout::PagedDocument;

use crate::diag::Diagnostic;
use crate::fonts::Fonts;
use crate::packages::PackagePolicy;
use crate::storage::Storage;
use crate::version::{Dep, StableHasher, Token, Versions};

/// The name of the virtual directory of the design library in the vault.
pub const LIB_DIR: &str = "_baluk";

/// The name of the virtual directory of vault data (the graph) for notes.
pub const VAULT_DIR: &str = "_vault";

/// The name of the Typst input that carries the theme.
pub const THEME_INPUT: &str = "theme";

/// The design library `baluk/` embedded in the binary. In a debug build
/// rust-embed reads it from disk.
#[derive(RustEmbed)]
#[folder = "../../baluk/"]
#[include = "*.typ"]
struct EmbeddedLibrary;

/// Where the design library comes from.
#[derive(Debug, Clone)]
pub enum LibrarySource {
    /// A directory on disk: library files are part of note versions.
    Dir(PathBuf),
    /// The embedded copy: it changes only with the binary.
    Embedded,
}

impl LibrarySource {
    /// Whether the library has the entry point `lib.typ`.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Dir(dir) => dir.join("lib.typ").is_file(),
            Self::Embedded => EmbeddedLibrary::get("lib.typ").is_some(),
        }
    }

    /// The fingerprint for the disk cache label: the embedded library is a hash
    /// of all its files (it is not part of note versions); a directory is 0
    /// (its files are part of note versions).
    pub fn fingerprint(&self) -> u64 {
        match self {
            Self::Dir(_) => 0,
            Self::Embedded => {
                let mut names: Vec<_> = EmbeddedLibrary::iter().collect();
                names.sort();
                let mut h = StableHasher::new();
                for name in names {
                    let data = EmbeddedLibrary::get(&name).map(|f| f.data.into_owned()).unwrap_or_default();
                    h.str(&name).bytes(&data);
                }
                h.finish()
            }
        }
    }
}

/// Where project, library and package files come from.
#[derive(Debug)]
struct Loader {
    storage: Arc<dyn Storage>,
    /// File fingerprints; vault data `/_vault/...` is there too.
    versions: Versions,
    lib: LibrarySource,
    packages: Arc<SystemPackages>,
    /// Which packages may be used ([`crate::packages`]).
    policy: Arc<PackagePolicy>,
    /// Fingerprints of the files read, at the time of reading (since the last reset).
    read: Mutex<HashMap<FileId, Token>>,
}

/// Where a file is.
enum Location {
    /// A vault file, the path from the root without a leading `/`.
    Vault(String),
    /// A file on disk (the library as a directory, a package).
    Disk(PathBuf),
    /// A path inside the embedded library, without a leading `/`.
    Embedded(String),
    /// A vault data file (`/_vault/...`), the path without `_vault/`.
    Data(String),
}

impl Loader {
    /// Where the file is. For packages, after downloading if needed.
    fn locate(&self, id: FileId) -> FileResult<Location> {
        let vpath = id.vpath();
        if matches!(id.root(), VirtualRoot::Project)
            && let Some(rest) = vault_relative(vpath)
        {
            return Ok(Location::Data(rest));
        }
        let (root, vpath) = match id.root() {
            VirtualRoot::Project => match (lib_relative(vpath)?, &self.lib) {
                (Some(rest), LibrarySource::Embedded) => {
                    return Ok(Location::Embedded(rest.get_without_slash().to_owned()));
                }
                (Some(rest), LibrarySource::Dir(dir)) => (dir.clone(), rest),
                (None, _) => return Ok(Location::Vault(vpath.get_without_slash().to_owned())),
            },
            VirtualRoot::Package(spec) => {
                // Not on the list: a build error, and the package is not even downloaded.
                self.policy.check(spec).map_err(|e| FileError::Other(Some(e.into())))?;
                (self.packages.obtain(spec)?.path().to_path_buf(), vpath.clone())
            }
        };
        vpath.realize(&root).map(Location::Disk).map_err(Into::into)
    }

    /// A vault file or a library file on disk, for versions; for a package, the
    /// list of allowed packages (a package itself never changes within a version).
    fn dep(&self, id: FileId) -> Option<Dep> {
        if matches!(id.root(), VirtualRoot::Package(_)) {
            return Some(Dep::Data(crate::packages::POLICY_FILE.to_owned()));
        }
        match (id.root(), self.locate(id)) {
            (VirtualRoot::Project, Ok(Location::Vault(path))) => Some(Dep::Vault(path)),
            (VirtualRoot::Project, Ok(Location::Disk(path))) => Some(Dep::Library(path)),
            (VirtualRoot::Project, Ok(Location::Data(rest))) => Some(Dep::Data(rest)),
            _ => None,
        }
    }

    fn load_located(&self, location: Location) -> FileResult<Bytes> {
        let path = match location {
            Location::Vault(rel) => {
                let shown = self.storage.display(&rel);
                let meta = self.storage.stat(&rel).map_err(|e| FileError::from_io(e, &shown))?;
                if meta.is_dir {
                    return Err(FileError::IsDirectory);
                }
                return self.storage.read(&rel).map(Bytes::new).map_err(|e| FileError::from_io(e, &shown));
            }
            Location::Disk(path) => path,
            Location::Embedded(rel) => {
                let file = EmbeddedLibrary::get(&rel).ok_or_else(|| FileError::NotFound(rel.into()))?;
                return Ok(Bytes::new(file.data.into_owned()));
            }
            Location::Data(rest) => {
                return self.versions.data().read(&rest).map(Bytes::new).map_err(|e| FileError::Other(Some(e.into())));
            }
        };
        let meta = fs::metadata(&path).map_err(|e| FileError::from_io(e, &path))?;
        if meta.is_dir() {
            return Err(FileError::IsDirectory);
        }
        fs::read(&path).map(Bytes::new).map_err(|e| FileError::from_io(e, &path))
    }
}

/// `/_baluk/lib.typ` -> `Some(/lib.typ)`; anything else -> `None`.
fn lib_relative(vpath: &VirtualPath) -> FileResult<Option<VirtualPath>> {
    let path = vpath.get_without_slash();
    let Some(rest) = path.strip_prefix(LIB_DIR) else { return Ok(None) };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return Ok(None); // "_baluk2/..." is an ordinary vault file
    }
    VirtualPath::new(if rest.is_empty() { "/" } else { rest })
        .map(Some)
        .map_err(|e| FileError::Other(Some(eco_format!("{e}"))))
}

/// `/_vault/graph/x.json` -> `Some("graph/x.json")`; anything else -> `None`.
fn vault_relative(vpath: &VirtualPath) -> Option<String> {
    let rest = vpath.get_without_slash().strip_prefix(VAULT_DIR)?.strip_prefix('/')?;
    Some(rest.to_owned())
}

impl FileLoader for Loader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        // The fingerprint comes before reading: an edit after it makes the build stale.
        if let Some(dep) = self.dep(id) {
            self.read.lock().insert(id, self.versions.token(&dep));
        }
        self.load_located(self.locate(id)?)
    }
}

/// The result of compiling one file in every theme.
#[derive(Debug)]
pub struct Compilation {
    /// One document per theme (in the requested order), or the errors.
    pub docs: Result<Vec<(String, HtmlDocument)>, Vec<Diagnostic>>,
    /// Warnings without repeats (they are the same in every theme).
    pub warnings: Vec<Diagnostic>,
    /// Vault and library files the compile read, with their fingerprints at
    /// the time of reading (in order, without repeats).
    pub deps: Vec<(Dep, Token)>,
}

/// File caches for concurrent builds: each build takes its own `FileStore`
/// (reset before the build to see file changes and to collect the
/// dependencies of exactly this note) and returns it after. At most `max`
/// builds run at once (a device setting); the others wait.
struct Stores {
    storage: Arc<dyn Storage>,
    versions: Versions,
    lib: LibrarySource,
    packages: Arc<SystemPackages>,
    policy: Arc<PackagePolicy>,
    max: AtomicUsize,
    /// Free caches and how many are taken now.
    state: Mutex<(Vec<FileStore<Loader>>, usize)>,
    freed: Condvar,
}

impl Stores {
    /// Takes a file cache (waits if all `max` are taken).
    fn take(&self) -> StoreGuard<'_> {
        let mut state = self.state.lock();
        while state.1 >= self.max.load(Ordering::Relaxed) {
            self.freed.wait(&mut state);
        }
        state.1 += 1;
        let store = state.0.pop();
        drop(state);
        let store = store.unwrap_or_else(|| {
            FileStore::new(Loader {
                storage: self.storage.clone(),
                versions: self.versions.clone(),
                lib: self.lib.clone(),
                packages: self.packages.clone(),
                policy: self.policy.clone(),
                read: Mutex::default(),
            })
        });
        StoreGuard { stores: self, store: Some(store) }
    }
}

/// A taken file cache; it goes back to the pool on drop.
struct StoreGuard<'a> {
    stores: &'a Stores,
    store: Option<FileStore<Loader>>,
}

impl std::ops::Deref for StoreGuard<'_> {
    type Target = FileStore<Loader>;
    #[expect(clippy::expect_used, reason = "`store` is `None` only inside `drop`")]
    fn deref(&self) -> &Self::Target {
        self.store.as_ref().expect("before drop")
    }
}

impl std::ops::DerefMut for StoreGuard<'_> {
    #[expect(clippy::expect_used, reason = "`store` is `None` only inside `drop`")]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.store.as_mut().expect("before drop")
    }
}

impl Drop for StoreGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.stores.state.lock();
        if let Some(store) = self.store.take() {
            state.0.push(store);
        }
        state.1 -= 1;
        drop(state);
        self.stores.freed.notify_one();
    }
}

/// How many builds of different notes run at once until the device setting
/// arrives ([`Compiler::set_parallel`]): two, to open a note while a book
/// builds. The themes of one note also build in parallel (except warming
/// builds).
pub const PARALLEL: usize = 2;

/// How many builds remember figures (`comemo::evict`) until the device setting
/// arrives ([`Compiler::set_memo`]).
pub const MEMO: usize = 10;

/// Who waits for the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// The user: themes in parallel.
    User,
    /// Warming: themes one by one in one thread with a lower priority, so a
    /// background build does not get in the way of work on the computer.
    Background,
}

/// Pages of a note as PNG ([`Compiler::compile_png`]).
#[derive(Debug, Clone)]
pub struct PngPages {
    /// Pages in the document.
    pub count: usize,
    /// The rendered pages: (number from 1, PNG).
    pub pages: Vec<(usize, Vec<u8>)>,
}

/// The Typst compiler of one vault: file caches, fonts and a standard library
/// per theme, shared by every build.
pub struct Compiler {
    /// File caches: builds of different notes run in parallel, each with its own.
    stores: Stores,
    /// How many builds remember figures: `comemo::evict(memo)` after a build.
    memo: AtomicUsize,
    fonts: Arc<Fonts>,
    /// The Typst standard library per theme ("" - no theme).
    libraries: RwLock<HashMap<String, Arc<LazyHash<Library>>>>,
}

impl std::fmt::Debug for Compiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compiler").field("fonts", &self.fonts).finish_non_exhaustive()
    }
}

impl Compiler {
    /// The vault and the data `/_vault/...` come from `versions` ([`Versions::with_data`]).
    pub fn new(versions: Versions, lib: LibrarySource, fonts: Arc<Fonts>) -> Self {
        let downloader = SystemDownloader::new(concat!("baluk-notes/", env!("CARGO_PKG_VERSION")));
        let stores = Stores {
            storage: versions.storage().clone(),
            versions,
            lib,
            packages: Arc::new(SystemPackages::new(downloader)),
            policy: Arc::default(),
            max: AtomicUsize::new(PARALLEL),
            state: Mutex::default(),
            freed: Condvar::new(),
        };
        Self { stores, memo: AtomicUsize::new(MEMO), fonts, libraries: RwLock::default() }
    }

    /// How many builds of different notes run at once (at least one).
    pub fn set_parallel(&self, n: usize) {
        self.stores.max.store(n.max(1), Ordering::Relaxed);
        // Under the lock: a waiter in `take` does not miss the wakeup.
        drop(self.stores.state.lock());
        self.stores.freed.notify_all();
    }

    /// Packages by this list (shared with the provider `/_vault/packages/...`);
    /// by default only the allowlist. Before the first build.
    #[must_use]
    pub fn with_packages(mut self, policy: Arc<PackagePolicy>) -> Self {
        self.stores.policy = policy;
        self
    }

    /// How many builds Typst remembers figures for: more means faster rebuilds
    /// after an edit, but more memory.
    pub fn set_memo(&self, n: usize) {
        self.memo.store(n, Ordering::Relaxed);
    }

    /// Forgets everything Typst remembers (after a warming round).
    pub fn release_memory(&self) {
        comemo::evict(0);
    }

    fn evict(&self) {
        // The comemo cache grows with every compile; typst-cli trims it the same way.
        comemo::evict(self.memo.load(Ordering::Relaxed));
    }

    pub fn fonts(&self) -> &Arc<Fonts> {
        &self.fonts
    }

    /// Compiles `main` (a path from the vault root) to HTML once per theme.
    /// An empty theme list means one compile without the `theme` input.
    pub fn compile_html(&self, main: &Path, themes: &[String], priority: Priority) -> Compilation {
        let no_theme = [String::new()];
        let themes = if themes.is_empty() { &no_theme[..] } else { themes };
        let main = match main_id(main) {
            Ok(id) => id,
            Err(message) => {
                return Compilation { docs: Err(vec![Diagnostic::error(message)]), warnings: vec![], deps: vec![] };
            }
        };

        let mut files = self.stores.take();
        files.reset();
        files.loader().read.lock().clear();
        let time = Time::system();
        // Themes build in parallel: each has its own world, the file cache is
        // shared. Typst parallelizes layout itself, but CeTZ figures are
        // computed mostly in one thread, so two themes in parallel are almost
        // twice as fast. Warming goes one by one, in one thread with a lower priority.
        let store = &*files;
        let compile = |theme: &String| {
            let library = self.library(theme);
            let world = CompileWorld { files: store, fonts: &self.fonts, library: &library, main, time: &time };
            compile_theme(&world, theme)
        };
        let results: Vec<ThemeResult> = std::thread::scope(|scope| {
            let handles: Vec<_> = match priority {
                Priority::User => themes
                    .iter()
                    .map(|theme| spawn_compile(scope, format!("typst-{theme}"), move || vec![compile(theme)]))
                    .collect(),
                Priority::Background => vec![spawn_compile(scope, "typst-warm".into(), move || {
                    lower_priority();
                    themes.iter().map(compile).collect()
                })],
            };
            handles.into_iter().flat_map(|h| joined(h.join())).collect()
        });

        let mut docs = Vec::with_capacity(themes.len());
        let mut errors = None;
        let mut warnings: Vec<Diagnostic> = Vec::new();
        for r in results {
            for w in r.warnings {
                if !warnings.contains(&w) {
                    warnings.push(w);
                }
            }
            match r.doc {
                Ok(doc) => docs.push((r.theme, doc)),
                // The error is the same in the other themes: show the first one.
                Err(errs) => {
                    errors.get_or_insert(errs);
                }
            }
        }
        let (loader, ids) = files.dependencies();
        let read = loader.read.lock();
        let mut deps: Vec<(Dep, Token)> = ids
            .filter_map(|id| {
                let dep = loader.dep(id)?;
                let token = read.get(&id).copied().unwrap_or_else(|| loader.versions.token(&dep));
                Some((dep, token))
            })
            .collect();
        drop(read);
        deps.sort();
        deps.dedup_by(|a, b| a.0 == b.0);
        drop(files);
        self.evict();

        Compilation { docs: errors.map_or(Ok(docs), Err), warnings, deps }
    }

    /// Compiles `main` to PDF in one theme (empty: without the `theme` input).
    pub fn compile_pdf(&self, main: &Path, theme: &str) -> Result<Vec<u8>, Vec<Diagnostic>> {
        self.compile_paged(main, theme, |doc| typst_pdf::pdf(doc, &typst_pdf::PdfOptions::default()))
    }

    /// Compiles `main` in one theme and renders pages to PNG as the PDF looks:
    /// `pages` are numbers from 1 (empty: all), `pixel_per_pt` is the scale
    /// (`dpi / 72`).
    pub fn compile_png(
        &self,
        main: &Path,
        theme: &str,
        pages: &[usize],
        pixel_per_pt: f64,
    ) -> Result<PngPages, Vec<Diagnostic>> {
        let options =
            typst_render::RenderOptions { pixel_per_pt: typst::utils::Scalar::new(pixel_per_pt), render_bleed: false };
        let (count, rendered) = self.compile_paged(main, theme, |doc| {
            let all = doc.pages();
            if pages.iter().any(|&n| n == 0 || n > all.len()) {
                return Ok((all.len(), Vec::new()));
            }
            let picked: Vec<_> = (1..=all.len())
                .filter(|n| pages.is_empty() || pages.contains(n))
                .map(|n| (n, typst_render::render(&all[n - 1], &options)))
                .collect();
            Ok((all.len(), picked))
        })?;
        if let Some(missing) = pages.iter().find(|&&n| n == 0 || n > count) {
            return Err(vec![Diagnostic::error(format!("no page {missing}: the note has {count}"))]);
        }
        let pages = rendered
            .into_iter()
            .map(|(n, pixmap)| pixmap.encode_png().map(|png| (n, png)))
            .collect::<Result<_, _>>()
            .map_err(|e| vec![Diagnostic::error(format!("encoding PNG: {e}"))])?;
        Ok(PngPages { count, pages })
    }

    /// Compiles `main` to pages in one theme and hands the document to `export`.
    fn compile_paged<T>(
        &self,
        main: &Path,
        theme: &str,
        export: impl FnOnce(&PagedDocument) -> typst::diag::SourceResult<T>,
    ) -> Result<T, Vec<Diagnostic>> {
        let main = main_id(main).map_err(|m| vec![Diagnostic::error(m)])?;
        let mut files = self.stores.take();
        files.reset();
        let time = Time::system();
        let library = self.library(theme);
        let world = CompileWorld { files: &files, fonts: &self.fonts, library: &library, main, time: &time };
        let to_diags = |errs: &[typst::diag::SourceDiagnostic]| -> Vec<Diagnostic> {
            errs.iter().map(|e| Diagnostic::from_typst(&world, e)).collect()
        };
        let built = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let doc = typst::compile::<PagedDocument>(&world).output.map_err(|e| to_diags(&e))?;
            export(&doc).map_err(|e| to_diags(&e))
        }));
        let exported = built.unwrap_or_else(|panic| Err(vec![panic_error(&*panic)]));
        drop(files);
        self.evict();
        exported
    }

    fn library(&self, theme: &str) -> Arc<LazyHash<Library>> {
        if let Some(lib) = self.libraries.read().get(theme) {
            return lib.clone();
        }
        let mut inputs = Dict::new();
        if !theme.is_empty() {
            inputs.insert(THEME_INPUT.into(), theme.into_value());
        }
        let library =
            Library::builder().with_inputs(inputs).with_features(Features::from_iter([Feature::Html])).build();
        self.libraries.write().entry(theme.to_owned()).or_insert_with(|| Arc::new(LazyHash::new(library))).clone()
    }
}

/// The stack of a compile thread: deeply nested markup is recursive.
const COMPILE_STACK: usize = 64 << 20;

/// A compile thread (with a big stack).
#[expect(clippy::expect_used, reason = "the OS refuses a thread only when out of resources")]
fn spawn_compile<'scope, T: Send + 'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    name: String,
    job: impl FnOnce() -> T + Send + 'scope,
) -> std::thread::ScopedJoinHandle<'scope, T> {
    std::thread::Builder::new()
        .name(name)
        .stack_size(COMPILE_STACK)
        .spawn_scoped(scope, job)
        .expect("a compile thread starts")
}

/// Lowers the priority of the current thread (a warming build): `nice` 10. On
/// Linux (and Android) priority belongs to a thread, not a process. Typst's own
/// layout threads (a shared pool) stay as they were.
fn lower_priority() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if let Err(e) = rustix::process::setpriority_process(Some(rustix::thread::gettid()), 10) {
        tracing::debug!("warming thread priority not lowered: {e}");
    }
}

struct ThemeResult {
    theme: String,
    doc: Result<HtmlDocument, Vec<Diagnostic>>,
    warnings: Vec<Diagnostic>,
}

/// Results of a compile thread; a panic in Typst (a bug in it or in a
/// package) becomes a build error of the note instead of taking the server down.
fn joined(results: std::thread::Result<Vec<ThemeResult>>) -> Vec<ThemeResult> {
    results.unwrap_or_else(|panic| {
        vec![ThemeResult { theme: String::new(), doc: Err(vec![panic_error(&*panic)]), warnings: Vec::new() }]
    })
}

/// The build error for a panic caught during a compilation.
fn panic_error(panic: &(dyn Any + Send)) -> Diagnostic {
    let reason = panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("no message");
    Diagnostic::error(format!("Typst crashed while compiling the note: {reason}"))
}

fn compile_theme(world: &CompileWorld, theme: &str) -> ThemeResult {
    let result = typst::compile::<HtmlDocument>(world);
    let warnings =
        result.warnings.iter().filter(|w| !Diagnostic::is_noise(w)).map(|w| Diagnostic::from_typst(world, w)).collect();
    let doc = result.output.map_err(|errs| errs.iter().map(|e| Diagnostic::from_typst(world, e)).collect());
    ThemeResult { theme: theme.to_owned(), doc, warnings }
}

fn main_id(main: &Path) -> Result<FileId, String> {
    let path = main.to_str().ok_or_else(|| format!("the path is not UTF-8: {}", main.display()))?;
    let vpath = VirtualPath::new(path.replace('\\', "/")).map_err(|e| format!("{path}: {e}"))?;
    Ok(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

/// The world of one compile: the shared file cache + the theme library + the main file.
struct CompileWorld<'a> {
    files: &'a FileStore<Loader>,
    fonts: &'a Fonts,
    library: &'a LazyHash<Library>,
    main: FileId,
    time: &'a Time,
}

impl World for CompileWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_paths_are_virtual() {
        let v = |s| VirtualPath::new(s).unwrap();
        assert_eq!(lib_relative(&v("/_baluk/lib.typ")).unwrap(), Some(v("/lib.typ")));
        assert_eq!(lib_relative(&v("/_baluk")).unwrap(), Some(v("/")));
        assert_eq!(lib_relative(&v("/_baluk2/x.typ")).unwrap(), None);
        assert_eq!(lib_relative(&v("/Сеть/SSH.typ")).unwrap(), None);
        assert_eq!(vault_relative(&v("/_vault/graph/x.json")).as_deref(), Some("graph/x.json"));
        assert_eq!(vault_relative(&v("/_vault2/x")), None);
        assert_eq!(vault_relative(&v("/Сеть/SSH.typ")), None);
    }

    #[test]
    fn panic_is_build_error() {
        let message = |panic: Box<dyn Any + Send>| {
            let results = joined(Err(panic));
            let [ThemeResult { doc: Err(errors), .. }] = results.as_slice() else { panic!("one failed result") };
            errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>()
        };
        assert_eq!(
            message(Box::new("index out of bounds")),
            ["Typst crashed while compiling the note: index out of bounds"]
        );
        assert_eq!(message(Box::new(String::from("boom"))), ["Typst crashed while compiling the note: boom"]);
        assert_eq!(message(Box::new(42)), ["Typst crashed while compiling the note: no message"]);
    }
}
