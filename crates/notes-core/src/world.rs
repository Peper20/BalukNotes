//! Компилятор Typst поверх хранилища.
//!
//! - Корень проекта Typst — корень хранилища.
//! - `/_konspekt/…` — **виртуальный** каталог: файлы берутся из библиотеки
//!   оформления приложения, копии в хранилище нет. Библиотека — каталог на
//!   диске (разработка: правки видны сразу и меняют версии заметок) или
//!   встроенная в бинарник копия `konspekt/` (релиз: бинарник самодостаточен).
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
use parking_lot::{Mutex, RwLock};
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

/// Имя виртуального каталога библиотеки оформления в хранилище.
pub const LIB_DIR: &str = "_konspekt";

/// Имя входа Typst, через который передаётся тема.
pub const THEME_INPUT: &str = "тема";

/// Библиотека оформления `konspekt/`, встроенная в бинарник. В отладочной
/// сборке rust-embed читает её с диска.
#[derive(RustEmbed)]
#[folder = "../../konspekt/"]
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
}

/// Откуда берутся файлы проекта, библиотеки и пакетов.
#[derive(Debug)]
struct Loader {
    vault: PathBuf,
    lib: LibrarySource,
    packages: SystemPackages,
}

/// Где лежит файл.
enum Location {
    Disk(PathBuf),
    /// Путь внутри встроенной библиотеки, без `/` в начале.
    Embedded(String),
}

impl Loader {
    /// Где файл. Для пакетов — после скачивания, если нужно.
    fn locate(&self, id: FileId) -> FileResult<Location> {
        let vpath = id.vpath();
        let (root, vpath) = match id.root() {
            VirtualRoot::Project => match (lib_relative(vpath)?, &self.lib) {
                (Some(rest), LibrarySource::Embedded) => {
                    return Ok(Location::Embedded(rest.get_without_slash().to_owned()));
                }
                (Some(rest), LibrarySource::Dir(dir)) => (dir.clone(), rest),
                (None, _) => (self.vault.clone(), vpath.clone()),
            },
            VirtualRoot::Package(spec) => (self.packages.obtain(spec)?.path().to_path_buf(), vpath.clone()),
        };
        vpath.realize(&root).map(Location::Disk).map_err(Into::into)
    }

    /// Путь файла хранилища или библиотеки на диске (не пакета) — для версий.
    fn local_path(&self, id: FileId) -> Option<PathBuf> {
        match (id.root(), self.locate(id)) {
            (VirtualRoot::Project, Ok(Location::Disk(path))) => Some(path),
            _ => None,
        }
    }
}

/// `/_konspekt/lib.typ` → `Some(/lib.typ)`; остальное → `None`.
fn lib_relative(vpath: &VirtualPath) -> FileResult<Option<VirtualPath>> {
    let path = vpath.get_without_slash();
    let Some(rest) = path.strip_prefix(LIB_DIR) else { return Ok(None) };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return Ok(None); // «_konspekt2/…» — обычный файл хранилища
    }
    VirtualPath::new(if rest.is_empty() { "/" } else { rest })
        .map(Some)
        .map_err(|e| FileError::Other(Some(eco_format!("{e}"))))
}

impl FileLoader for Loader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        let path = match self.locate(id)? {
            Location::Disk(path) => path,
            Location::Embedded(rel) => {
                let file = EmbeddedLibrary::get(&rel).ok_or_else(|| FileError::NotFound(rel.into()))?;
                return Ok(Bytes::new(file.data.into_owned()));
            }
        };
        let meta = fs::metadata(&path).map_err(|e| FileError::from_io(e, &path))?;
        if meta.is_dir() {
            return Err(FileError::IsDirectory);
        }
        fs::read(&path).map(Bytes::new).map_err(|e| FileError::from_io(e, &path))
    }
}

/// Результат компиляции одного файла во всех темах.
#[derive(Debug)]
pub struct Compilation {
    /// По документу на тему (в порядке запроса) или ошибки.
    pub docs: Result<Vec<(String, HtmlDocument)>, Vec<Diagnostic>>,
    /// Предупреждения без повторов (в каждой теме они одни и те же).
    pub warnings: Vec<Diagnostic>,
    /// Файлы хранилища и библиотеки, которые прочитала компиляция.
    pub deps: Vec<PathBuf>,
}

pub struct Compiler {
    /// Заметки собираются по одной: `FileStore` сбрасывается перед каждой,
    /// чтобы увидеть изменения файлов и собрать список зависимостей именно
    /// этой заметки. Темы одной заметки — параллельно.
    files: Mutex<FileStore<Loader>>,
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
    pub fn new(vault: &Path, lib: LibrarySource, fonts: Arc<Fonts>) -> Self {
        let packages = SystemPackages::new(SystemDownloader::new(concat!("baluk-notes/", env!("CARGO_PKG_VERSION"))));
        let loader = Loader { vault: vault.to_path_buf(), lib, packages };
        Self { files: Mutex::new(FileStore::new(loader)), fonts, libraries: RwLock::default() }
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

        let mut files = self.files.lock();
        files.reset();
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
        let mut deps: Vec<PathBuf> = ids.filter_map(|id| loader.local_path(id)).collect();
        deps.sort();
        deps.dedup();
        drop(files);

        // Кэш comemo растёт с каждой компиляцией; typst-cli чистит его так же.
        comemo::evict(10);

        Compilation { docs: errors.map_or(Ok(docs), Err), warnings, deps }
    }

    /// Компилирует `main` в PDF в одной теме (пустая — без входа `тема`).
    pub fn compile_pdf(&self, main: &Path, theme: &str) -> Result<Vec<u8>, Vec<Diagnostic>> {
        let main = main_id(main).map_err(|m| vec![Diagnostic::error(m)])?;
        let mut files = self.files.lock();
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
        assert_eq!(lib_relative(&v("/_konspekt/lib.typ")).unwrap(), Some(v("/lib.typ")));
        assert_eq!(lib_relative(&v("/_konspekt")).unwrap(), Some(v("/")));
        assert_eq!(lib_relative(&v("/_konspekt2/x.typ")).unwrap(), None);
        assert_eq!(lib_relative(&v("/Сеть/SSH.typ")).unwrap(), None);
    }
}
