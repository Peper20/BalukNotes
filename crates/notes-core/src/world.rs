//! Компилятор Typst поверх хранилища.
//!
//! - Корень проекта Typst — корень хранилища.
//! - `/_baluk/…` — **виртуальный** каталог: файлы берутся из библиотеки
//!   оформления приложения, копии в хранилище нет. Библиотека — каталог на
//!   диске (разработка: правки видны сразу и меняют версии заметок) или
//!   встроенная в бинарник копия `baluk/` (релиз: бинарник самодостаточен).
//! - `/_vault/…` — **данные хранилища** для заметок (граф: `/_vault/graph/…`),
//!   их на лету отдают поставщики реестра [`crate::vault_data::VaultData`]; в версию заметки
//!   такой файл входит отпечатком поставщика (см. [`crate::vault_data`]).
//! - Файлы хранилища читаются через [`Storage`]; при чтении запоминается
//!   отпечаток файла ([`Versions::token`]) — из них версия заметки.
//! - Пакеты (`@preview/cetz`) — из кэша Typst, при отсутствии скачиваются.
//! - Тема передаётся входом `тема` (`sys.inputs.тема`): на каждую тему — своя
//!   стандартная библиотека Typst, созданная один раз.
//!
//! Файлы кэширует [`FileStore`]: при повторной компиляции разбор идёт
//! инкрементально, а список прочитанных файлов даёт версию заметки.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
use crate::storage::Storage;
use crate::version::{Dep, StableHasher, Token, Versions};

/// Имя виртуального каталога библиотеки оформления в хранилище.
pub const LIB_DIR: &str = "_baluk";

/// Имя виртуального каталога данных хранилища (граф) для заметок.
pub const VAULT_DIR: &str = "_vault";

/// Имя входа Typst, через который передаётся тема.
pub const THEME_INPUT: &str = "theme";

/// Библиотека оформления `baluk/`, встроенная в бинарник. В отладочной
/// сборке rust-embed читает её с диска.
#[derive(RustEmbed)]
#[folder = "../../baluk/"]
#[include = "*.typ"]
struct EmbeddedLibrary;

/// Откуда берётся библиотека оформления.
#[derive(Debug, Clone)]
pub enum LibrarySource {
    /// Каталог на диске: файлы библиотеки входят в версии заметок.
    Dir(PathBuf),
    /// Встроенная копия: меняется только с бинарником.
    Embedded,
}

impl LibrarySource {
    /// Есть ли в библиотеке точка входа `lib.typ`.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Dir(dir) => dir.join("lib.typ").is_file(),
            Self::Embedded => EmbeddedLibrary::get("lib.typ").is_some(),
        }
    }

    /// Отпечаток для метки кэша на диске: встроенная библиотека — хэш
    /// всех её файлов (в версии заметок она не входит); каталог — 0 (его
    /// файлы входят в версии заметок).
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

/// Откуда берутся файлы проекта, библиотеки и пакетов.
#[derive(Debug)]
struct Loader {
    storage: Arc<dyn Storage>,
    /// Отпечатки файлов; в них же — данные хранилища `/_vault/…`.
    versions: Versions,
    lib: LibrarySource,
    packages: Arc<SystemPackages>,
    /// Отпечатки прочитанных файлов на момент чтения (с последнего сброса).
    read: Mutex<HashMap<FileId, Token>>,
}

/// Где лежит файл.
enum Location {
    /// Файл хранилища, путь от корня без `/` в начале.
    Vault(String),
    /// Файл на диске (библиотека каталогом, пакет).
    Disk(PathBuf),
    /// Путь внутри встроенной библиотеки, без `/` в начале.
    Embedded(String),
    /// Файл данных хранилища (`/_vault/…`), путь без `_vault/`.
    Data(String),
}

impl Loader {
    /// Где файл. Для пакетов — после скачивания, если нужно.
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
            VirtualRoot::Package(spec) => (self.packages.obtain(spec)?.path().to_path_buf(), vpath.clone()),
        };
        vpath.realize(&root).map(Location::Disk).map_err(Into::into)
    }

    /// Файл хранилища или библиотеки на диске (не пакет) — для версий.
    fn dep(&self, id: FileId) -> Option<Dep> {
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

/// `/_baluk/lib.typ` → `Some(/lib.typ)`; остальное → `None`.
fn lib_relative(vpath: &VirtualPath) -> FileResult<Option<VirtualPath>> {
    let path = vpath.get_without_slash();
    let Some(rest) = path.strip_prefix(LIB_DIR) else { return Ok(None) };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return Ok(None); // «_baluk2/…» — обычный файл хранилища
    }
    VirtualPath::new(if rest.is_empty() { "/" } else { rest })
        .map(Some)
        .map_err(|e| FileError::Other(Some(eco_format!("{e}"))))
}

/// `/_vault/graph/x.json` → `Some("graph/x.json")`; остальное → `None`.
fn vault_relative(vpath: &VirtualPath) -> Option<String> {
    let rest = vpath.get_without_slash().strip_prefix(VAULT_DIR)?.strip_prefix('/')?;
    Some(rest.to_owned())
}

impl FileLoader for Loader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        // Отпечаток — до чтения: правка после него сделает сборку устаревшей.
        if let Some(dep) = self.dep(id) {
            self.read.lock().insert(id, self.versions.token(&dep));
        }
        self.load_located(self.locate(id)?)
    }
}

/// Результат компиляции одного файла во всех темах.
#[derive(Debug)]
pub struct Compilation {
    /// По документу на тему (в порядке запроса) или ошибки.
    pub docs: Result<Vec<(String, HtmlDocument)>, Vec<Diagnostic>>,
    /// Предупреждения без повторов (в каждой теме они одни и те же).
    pub warnings: Vec<Diagnostic>,
    /// Файлы хранилища и библиотеки, которые прочитала компиляция, с
    /// отпечатками на момент чтения (по порядку, без повторов).
    pub deps: Vec<(Dep, Token)>,
}

/// Кэши файлов для одновременных сборок: каждая сборка берёт свой
/// `FileStore` (он сбрасывается перед сборкой, чтобы увидеть изменения
/// файлов и собрать список зависимостей именно этой заметки) и возвращает
/// его после. Одновременно — не больше `max` сборок, остальные ждут.
struct Stores {
    storage: Arc<dyn Storage>,
    versions: Versions,
    lib: LibrarySource,
    packages: Arc<SystemPackages>,
    max: usize,
    /// Свободные кэши и сколько сейчас занято.
    state: Mutex<(Vec<FileStore<Loader>>, usize)>,
    freed: Condvar,
}

impl Stores {
    /// Взять кэш файлов (подождать, если заняты все `max`).
    fn take(&self) -> StoreGuard<'_> {
        let mut state = self.state.lock();
        while state.1 >= self.max {
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
                read: Mutex::default(),
            })
        });
        StoreGuard { stores: self, store: Some(store) }
    }
}

/// Взятый кэш файлов; при сбросе возвращается в пул.
struct StoreGuard<'a> {
    stores: &'a Stores,
    store: Option<FileStore<Loader>>,
}

impl std::ops::Deref for StoreGuard<'_> {
    type Target = FileStore<Loader>;
    fn deref(&self) -> &Self::Target {
        self.store.as_ref().expect("до сброса")
    }
}

impl std::ops::DerefMut for StoreGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.store.as_mut().expect("до сброса")
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

/// Сколько сборок разных заметок идёт одновременно: половина ядер, но не
/// меньше двух (открыть заметку, пока собирается книга). Темы одной
/// заметки — ещё и параллельно между собой.
fn max_parallel() -> usize {
    std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(2))
}

pub struct Compiler {
    /// Кэши файлов: сборки разных заметок идут параллельно, каждая со своим.
    stores: Stores,
    fonts: Arc<Fonts>,
    /// Стандартная библиотека Typst на каждую тему ("" — без темы).
    libraries: RwLock<HashMap<String, Arc<LazyHash<Library>>>>,
}

impl std::fmt::Debug for Compiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compiler").field("fonts", &self.fonts).finish_non_exhaustive()
    }
}

impl Compiler {
    /// Хранилище и данные `/_vault/…` — из `versions` ([`Versions::with_data`]).
    pub fn new(versions: Versions, lib: LibrarySource, fonts: Arc<Fonts>) -> Self {
        let downloader = SystemDownloader::new(concat!("baluk-notes/", env!("CARGO_PKG_VERSION")));
        let stores = Stores {
            storage: versions.storage().clone(),
            versions,
            lib,
            packages: Arc::new(SystemPackages::new(downloader)),
            max: max_parallel(),
            state: Mutex::default(),
            freed: Condvar::new(),
        };
        Self { stores, fonts, libraries: RwLock::default() }
    }

    pub fn fonts(&self) -> &Arc<Fonts> {
        &self.fonts
    }

    /// Компилирует `main` (путь от корня хранилища) в HTML по разу на тему.
    /// Пустой список тем — одна компиляция без входа `тема`.
    pub fn compile_html(&self, main: &Path, themes: &[String]) -> Compilation {
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
        // Темы собираются параллельно: у каждой свой мир, кэш файлов общий.
        // Typst и сам распараллеливает вёрстку, но рисунки CeTZ считаются
        // в основном в одном потоке — две темы параллельно почти вдвое быстрее.
        let store = &*files;
        let results: Vec<ThemeResult> = std::thread::scope(|scope| {
            let handles: Vec<_> = themes
                .iter()
                .map(|theme| {
                    let library = self.library(theme);
                    let (fonts, time) = (&*self.fonts, &time);
                    std::thread::Builder::new()
                        .name(format!("typst-{theme}"))
                        .stack_size(COMPILE_STACK)
                        .spawn_scoped(scope, move || {
                            let world = CompileWorld { files: store, fonts, library: &library, main, time };
                            compile_theme(&world, theme)
                        })
                        .expect("поток компиляции запускается")
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("компиляция темы не паникует")).collect()
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
                // В других темах ошибка та же — показываем первую.
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

        // Кэш comemo растёт с каждой компиляцией; typst-cli чистит его так же.
        comemo::evict(10);

        Compilation { docs: errors.map_or(Ok(docs), Err), warnings, deps }
    }

    /// Компилирует `main` в PDF в одной теме (пустая — без входа `тема`).
    pub fn compile_pdf(&self, main: &Path, theme: &str) -> Result<Vec<u8>, Vec<Diagnostic>> {
        let main = main_id(main).map_err(|m| vec![Diagnostic::error(m)])?;
        let mut files = self.stores.take();
        files.reset();
        let time = Time::system();
        let library = self.library(theme);
        let world = CompileWorld { files: &files, fonts: &self.fonts, library: &library, main, time: &time };
        let to_diags = |errs: &[typst::diag::SourceDiagnostic]| -> Vec<Diagnostic> {
            errs.iter().map(|e| Diagnostic::from_typst(&world, e)).collect()
        };
        let doc = typst::compile::<PagedDocument>(&world).output.map_err(|e| to_diags(&e))?;
        let pdf = typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()).map_err(|e| to_diags(&e));
        drop(files);
        comemo::evict(10);
        pdf
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

/// Стек потока компиляции: глубокая вложенность разметки рекурсивна.
const COMPILE_STACK: usize = 64 << 20;

struct ThemeResult {
    theme: String,
    doc: Result<HtmlDocument, Vec<Diagnostic>>,
    warnings: Vec<Diagnostic>,
}

fn compile_theme(world: &CompileWorld, theme: &str) -> ThemeResult {
    let result = typst::compile::<HtmlDocument>(world);
    let warnings =
        result.warnings.iter().filter(|w| !Diagnostic::is_noise(w)).map(|w| Diagnostic::from_typst(world, w)).collect();
    let doc = result.output.map_err(|errs| errs.iter().map(|e| Diagnostic::from_typst(world, e)).collect());
    ThemeResult { theme: theme.to_owned(), doc, warnings }
}

fn main_id(main: &Path) -> Result<FileId, String> {
    let path = main.to_str().ok_or_else(|| format!("путь не в UTF-8: {}", main.display()))?;
    let vpath = VirtualPath::new(path.replace('\\', "/")).map_err(|e| format!("{path}: {e}"))?;
    Ok(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

/// Мир одной компиляции: общий кэш файлов + библиотека темы + главный файл.
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
}
