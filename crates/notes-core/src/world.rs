//! Компилятор Typst поверх хранилища.
//!
//! - Корень проекта Typst — корень хранилища.
//! - `/_konspekt/…` — **виртуальный** каталог: файлы берутся из библиотеки
//!   оформления приложения, копии в хранилище нет.
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

use crate::diag::Diagnostic;
use crate::fonts::Fonts;

/// Имя виртуального каталога библиотеки оформления в хранилище.
pub const LIB_DIR: &str = "_konspekt";

/// Имя входа Typst, через который передаётся тема.
pub const THEME_INPUT: &str = "тема";

/// Откуда берутся файлы проекта, библиотеки и пакетов.
#[derive(Debug)]
struct Loader {
    vault: PathBuf,
    lib: PathBuf,
    packages: SystemPackages,
}

impl Loader {
    /// Настоящий путь файла. Для пакетов — после скачивания, если нужно.
    fn resolve(&self, id: FileId) -> FileResult<PathBuf> {
        let vpath = id.vpath();
        let (root, vpath) = match id.root() {
            VirtualRoot::Project => match lib_relative(vpath)? {
                Some(rest) => (self.lib.clone(), rest),
                None => (self.vault.clone(), vpath.clone()),
            },
            VirtualRoot::Package(spec) => (self.packages.obtain(spec)?.path().to_path_buf(), vpath.clone()),
        };
        vpath.realize(&root).map_err(Into::into)
    }

    /// Путь файла хранилища или библиотеки (не пакета) — для версий.
    fn local_path(&self, id: FileId) -> Option<PathBuf> {
        matches!(id.root(), VirtualRoot::Project).then(|| self.resolve(id).ok()).flatten()
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
        let path = self.resolve(id)?;
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
    /// Компиляции идут по одной: `FileStore` сбрасывается перед каждой, чтобы
    /// увидеть изменения файлов и собрать список зависимостей именно этой
    /// заметки. Внутри компиляции Typst сам распараллеливает работу.
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
    pub fn new(vault: &Path, lib: &Path, fonts: Arc<Fonts>) -> Self {
        let packages = SystemPackages::new(SystemDownloader::new(concat!("baluk-notes/", env!("CARGO_PKG_VERSION"))));
        let loader = Loader { vault: vault.to_path_buf(), lib: lib.to_path_buf(), packages };
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
        let mut docs = Vec::with_capacity(themes.len());
        let mut errors = None;
        let mut warnings: Vec<Diagnostic> = Vec::new();
        for theme in themes {
            let library = self.library(theme);
            let world = CompileWorld { files: &files, fonts: &self.fonts, library: &library, main, time: &time };
            let result = typst::compile::<HtmlDocument>(&world);
            for w in result.warnings.iter().filter(|w| !Diagnostic::is_noise(w)) {
                let d = Diagnostic::from_typst(&world, w);
                if !warnings.contains(&d) {
                    warnings.push(d);
                }
            }
            match result.output {
                Ok(doc) => docs.push((theme.clone(), doc)),
                Err(errs) => {
                    errors = Some(errs.iter().map(|e| Diagnostic::from_typst(&world, e)).collect());
                    break; // в других темах ошибка та же
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
