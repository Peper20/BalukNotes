//! Заметки как сервис: ленивая компиляция, кэш и версии.
//!
//! Заметка собирается, только когда её запросили. Вместе с результатом
//! запоминается список файлов, которые прочитала компиляция (сама заметка,
//! библиотека, код из `код-из-файла`, картинки…). **Версия** — хэш времён
//! изменения и размеров этих файлов: узнать, изменилась ли заметка, стоит
//! нескольких `stat`, без компиляции. Никаких фоновых наблюдателей: см.
//! `docs/architecture.md`, «Обновление — по запросу».
//!
//! Обработка рисунков ([`crate::figures`]) зависит от настроек, поэтому в
//! кэше лежит и сырая отрисовка: смена точности не перекомпилирует заметку.
//! Настройки входят в версию страницы — клиент перезапросит её сам.

use std::collections::HashMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use parking_lot::Mutex;
use serde::Serialize;

use crate::diag::Diagnostic;
use crate::figures::{self, FigureOptions};
use crate::fonts::Fonts;
use crate::render::{self, LinkResolver, Rendered};
use crate::themes::ThemeSet;
use crate::vault::{Entry, NoteId, NoteKind, Vault};
use crate::world::Compiler;
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct NotesConfig {
    /// Корень хранилища.
    pub vault: PathBuf,
    /// Библиотека оформления (`konspekt/`), видна заметкам как `/_konspekt/`.
    pub library: PathBuf,
    /// Дополнительные каталоги шрифтов (к системным и встроенным в Typst).
    pub font_dirs: Vec<PathBuf>,
}

/// Куда ведут ссылки между заметками.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStyle {
    /// `/n/Сеть/SSH#якорь` — для сервера и клиента-SPA.
    Server,
    /// `../Сеть/SSH.html#якорь` — для статического сайта (`notes build`).
    Static,
}

/// Заметка, готовая к показу.
#[derive(Debug, Serialize)]
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
}

#[derive(Debug)]
struct Cached {
    page: Arc<NotePage>,
    /// Отрисовка до обработки рисунков (последняя удачная).
    raw: Option<Arc<Rendered>>,
    opts: FigureOptions,
    deps: Vec<PathBuf>,
    /// Версия файлов, по которой собрана страница.
    files: String,
}

#[derive(Debug)]
pub struct Notes {
    vault: Vault,
    compiler: Compiler,
    themes: ThemeSet,
    cache: Mutex<HashMap<NoteId, Cached>>,
}

impl Notes {
    pub fn open(config: &NotesConfig) -> Result<Self> {
        let vault = Vault::open(&config.vault)?;
        let library = fs::canonicalize(&config.library).map_err(|e| Error::io(&config.library, e))?;
        if !library.join("lib.typ").is_file() {
            return Err(Error::Library(format!("в {} нет lib.typ", library.display())));
        }
        let fonts = Arc::new(Fonts::load(&config.font_dirs));
        let compiler = Compiler::new(vault.root(), &library, fonts);
        let themes = ThemeSet::load(&compiler)?;
        Ok(Self { vault, compiler, themes, cache: Mutex::default() })
    }

    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    pub fn themes(&self) -> &ThemeSet {
        &self.themes
    }

    pub fn fonts(&self) -> &Fonts {
        self.compiler.fonts()
    }

    pub fn entries(&self) -> Result<Vec<Entry>> {
        self.vault.entries()
    }

    /// Страница заметки для сервера: из кэша, если её файлы не менялись.
    pub fn page(&self, id: &NoteId, opts: FigureOptions) -> Result<Arc<NotePage>> {
        let entry = self.vault.entry(id)?;
        let previous_raw = {
            let cache = self.cache.lock();
            match cache.get(id) {
                Some(c) if version_of(&c.deps) == c.files => {
                    if c.opts == opts {
                        return Ok(c.page.clone());
                    }
                    // Файлы те же, настройки другие — только обработка рисунков.
                    let (page, raw, deps, files) = (c.page.clone(), c.raw.clone(), c.deps.clone(), c.files.clone());
                    drop(cache);
                    let rendered = raw.as_deref().map(|r| Arc::new(self.finish(r, opts)));
                    let page = Arc::new(NotePage {
                        id: page.id.clone(),
                        kind: page.kind,
                        version: page_version(&files, opts),
                        rendered,
                        errors: page.errors.clone(),
                        warnings: page.warnings.clone(),
                    });
                    self.cache.lock().insert(id.clone(), Cached { page: page.clone(), raw, opts, deps, files });
                    return Ok(page);
                }
                Some(c) => c.raw.clone(),
                None => None,
            }
        };
        let built = self.build(&entry, LinkStyle::Server, opts);
        let (rendered, raw) = match built.raw {
            Some(raw) => (built.rendered, Some(raw)),
            // Ошибка — показываем прежнюю удачную отрисовку.
            None => (previous_raw.as_deref().map(|r| Arc::new(self.finish(r, opts))), previous_raw),
        };
        let page = Arc::new(NotePage { rendered, ..built.page });
        let cached = Cached { page: page.clone(), raw, opts, deps: built.deps, files: built.files };
        self.cache.lock().insert(id.clone(), cached);
        Ok(page)
    }

    /// Текущая версия заметки. Для уже собранной — только `stat` её файлов,
    /// без компиляции; для новой — собирает.
    pub fn version(&self, id: &NoteId, opts: FigureOptions) -> Result<String> {
        if let Some(c) = self.cache.lock().get(id) {
            return Ok(page_version(&version_of(&c.deps), opts));
        }
        Ok(self.page(id, opts)?.version.clone())
    }

    /// Страница для статического сайта: без кэша, относительные ссылки.
    pub fn page_static(&self, entry: &Entry, opts: FigureOptions) -> NotePage {
        let built = self.build(entry, LinkStyle::Static, opts);
        NotePage { rendered: built.rendered, ..built.page }
    }

    fn build(&self, entry: &Entry, style: LinkStyle, opts: FigureOptions) -> Built {
        let started = std::time::Instant::now();
        let compilation = self.compiler.compile_html(&entry.main, &self.themes.names());
        let links = VaultLinks { vault: &self.vault, style, from: &entry.id };
        let (raw, errors) = match compilation.docs {
            Ok(docs) => match render::render(docs, &links) {
                Ok(r) => (Some(Arc::new(r)), vec![]),
                Err(message) => (None, vec![Diagnostic::error(message)]),
            },
            Err(errors) => (None, errors),
        };
        let rendered = raw.as_deref().map(|r| Arc::new(self.finish(r, opts)));
        tracing::debug!(id = %entry.id, ms = started.elapsed().as_millis(), errors = errors.len(), "собрана");
        let files = version_of(&compilation.deps);
        let page = NotePage {
            id: entry.id.clone(),
            kind: entry.kind,
            version: page_version(&files, opts),
            rendered: None,
            errors,
            warnings: compilation.warnings,
        };
        Built { page, rendered, raw, deps: compilation.deps, files }
    }

    /// Обработка рисунков: общие глифы, один SVG на темы, округление.
    fn finish(&self, raw: &Rendered, opts: FigureOptions) -> Rendered {
        let o = figures::optimize(&raw.body, &self.themes.names(), opts);
        tracing::debug!(
            before = raw.body.len(),
            after = o.body.len(),
            figures = o.stats.figures,
            merged = o.stats.merged,
            glyphs = o.stats.glyphs,
            colors = o.stats.colors,
            "рисунки"
        );
        Rendered {
            title: raw.title.clone(),
            styles: format!("{}{}", raw.styles, o.styles),
            body: o.body,
            headings: raw.headings.clone(),
            links: raw.links.clone(),
            tags: raw.tags.clone(),
        }
    }
}

/// Результат сборки: страница без отрисовки (её кладёт вызывающий —
/// свежую или прежнюю), отрисовка, сырая отрисовка, файлы.
struct Built {
    page: NotePage,
    rendered: Option<Arc<Rendered>>,
    raw: Option<Arc<Rendered>>,
    deps: Vec<PathBuf>,
    files: String,
}

fn page_version(files: &str, opts: FigureOptions) -> String {
    format!("{files}-{}", opts.key())
}

/// Хэш (путь, время изменения, размер) по всем файлам. Пропавший файл тоже
/// меняет версию.
fn version_of(deps: &[PathBuf]) -> String {
    let mut h = DefaultHasher::new();
    for path in deps {
        path.hash(&mut h);
        match fs::metadata(path) {
            Ok(meta) => {
                meta.len().hash(&mut h);
                meta.modified().ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).hash(&mut h);
            }
            Err(_) => "нет".hash(&mut h),
        }
    }
    format!("{:016x}", h.finish())
}

/// Адреса ссылок `#см(…)`: существует ли цель, и куда вести.
struct VaultLinks<'a> {
    vault: &'a Vault,
    style: LinkStyle,
    from: &'a NoteId,
}

impl LinkResolver for VaultLinks<'_> {
    fn href(&self, target: &str, anchor: Option<&str>) -> Option<String> {
        let id = NoteId::new(target).ok()?;
        self.vault.entry(&id).ok()?;
        let fragment = anchor.map(|a| format!("#{}", encode(&render::slug(a)))).unwrap_or_default();
        Some(match self.style {
            LinkStyle::Server => format!("/n/{}{fragment}", encode(id.as_str())),
            LinkStyle::Static => {
                let up = "../".repeat(self.from.as_str().matches('/').count());
                format!("{up}{}.html{fragment}", encode(id.as_str()))
            }
        })
    }
}

/// Кодирует то, что в адресе иначе поменяет смысл. Кириллица остаётся как
/// есть: браузеры её понимают, а адрес читается.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '%' => out.push_str("%25"),
            '"' => out.push_str("%22"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            _ => out.push(ch),
        }
    }
    out
}

/// Путь страницы заметки в статическом сайте.
pub fn static_path(id: &NoteId) -> PathBuf {
    Path::new(id.as_str()).with_extension("html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_changes_with_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.typ");
        fs::write(&f, "1").unwrap();
        let deps = vec![f.clone()];
        let v1 = version_of(&deps);
        assert_eq!(v1, version_of(&deps));
        fs::write(&f, "12").unwrap();
        assert_ne!(v1, version_of(&deps));
        fs::remove_file(&f).unwrap();
        assert_ne!(v1, version_of(&deps));
    }

    #[test]
    fn link_addresses() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Сеть")).unwrap();
        fs::write(dir.path().join("Сеть/SSH.typ"), "").unwrap();
        fs::write(dir.path().join("Итоги 2026.typ"), "").unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let from = NoteId::new("Сеть/UFW").unwrap();
        let server = VaultLinks { vault: &vault, style: LinkStyle::Server, from: &from };
        assert_eq!(server.href("Сеть/SSH", Some("Смена порта")).unwrap(), "/n/Сеть/SSH#Смена-порта");
        assert_eq!(server.href("Итоги 2026", None).unwrap(), "/n/Итоги%202026");
        assert_eq!(server.href("Сеть/Nginx", None), None);
        assert_eq!(server.href("../etc/passwd", None), None);
        let stat = VaultLinks { vault: &vault, style: LinkStyle::Static, from: &from };
        assert_eq!(stat.href("Сеть/SSH", None).unwrap(), "../Сеть/SSH.html");
    }
}
