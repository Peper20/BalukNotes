//! Сборка заметки — чистая функция от файлов: исходник → компиляция по
//! темам ([`crate::world`]) → отрисовка ([`crate::render`]) → сырая
//! страница, и обработка рисунков под настройки ([`crate::figures`]).
//!
//! Ни кэша, ни блокировок: их добавляют слои выше ([`crate::pages`],
//! [`crate::page_cache`]). Интерфейс [`Pipeline`] — чтобы проверять эти слои
//! без компиляции Typst (подменой сборки).

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::diag::Diagnostic;
use crate::figures::{self, FigureOptions};
use crate::fonts::Fonts;
use crate::render::{self, LinkResolver, Rendered};
use crate::themes::ThemeSet;
use crate::vault::{Entry, NoteId, Vault};
use crate::version::{Dep, combine};
use crate::world::Compiler;

/// Куда ведут ссылки между заметками.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStyle {
    /// `/n/Сеть/SSH#якорь` — для сервера и клиента-SPA.
    Server,
    /// `../Сеть/SSH.html#якорь` — для статического сайта (`notes build`).
    Static,
}

/// Результат сборки.
#[derive(Debug)]
pub struct Build {
    /// Отрисовка до обработки рисунков; `None` — ошибка.
    pub raw: Option<Rendered>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// Прочитанные файлы.
    pub deps: Vec<Dep>,
    /// Версия файлов на момент чтения ([`crate::version`]).
    pub files: String,
    pub took: Duration,
}

/// Сборка заметки.
pub trait Pipeline: Send + Sync + fmt::Debug {
    /// Собрать заметку (без обработки рисунков).
    fn build(&self, entry: &Entry, links: LinkStyle) -> Build;

    /// Обработать рисунки под настройки.
    fn finish(&self, raw: &Rendered, opts: FigureOptions) -> Rendered;
}

/// Сборка компилятором Typst.
#[derive(Debug)]
pub struct TypstPipeline {
    vault: Vault,
    compiler: Compiler,
    themes: ThemeSet,
}

impl TypstPipeline {
    pub fn new(vault: Vault, compiler: Compiler, themes: ThemeSet) -> Self {
        Self { vault, compiler, themes }
    }

    pub fn themes(&self) -> &ThemeSet {
        &self.themes
    }

    pub fn fonts(&self) -> &Fonts {
        self.compiler.fonts()
    }

    pub fn compiler(&self) -> &Compiler {
        &self.compiler
    }

    /// Предупреждения [`lint`](crate::lint) для файлов хранилища, из которых
    /// собрана заметка (библиотеку и пакеты не проверяем).
    fn lint(&self, deps: &[Dep]) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for dep in deps {
            let Dep::Vault(path) = dep else { continue };
            if Path::new(path).extension().is_none_or(|e| e != "typ") {
                continue;
            }
            let Ok(text) = self.vault.read_text(path) else { continue };
            for l in crate::lint::lint(&text) {
                let (line, column) = crate::lint::line_column(&text, l.offset);
                out.push(Diagnostic::warning_at(l.message, format!("/{path}"), line, column, l.hint));
            }
        }
        out
    }
}

impl Pipeline for TypstPipeline {
    fn build(&self, entry: &Entry, style: LinkStyle) -> Build {
        let started = Instant::now();
        let compilation = self.compiler.compile_html(&entry.main, &self.themes.names());
        let links = VaultLinks { vault: &self.vault, style, from: &entry.id };
        let (raw, errors) = match compilation.docs {
            Ok(docs) => match render::render(docs, &links) {
                Ok(r) => (Some(r), vec![]),
                Err(message) => (None, vec![Diagnostic::error(message)]),
            },
            Err(errors) => (None, errors),
        };
        let took = started.elapsed();
        tracing::debug!(id = %entry.id, ms = took.as_millis(), errors = errors.len(), "собрана");
        let files = combine(&compilation.deps);
        let deps: Vec<Dep> = compilation.deps.into_iter().map(|(d, _)| d).collect();
        let mut warnings = compilation.warnings;
        warnings.extend(self.lint(&deps));
        Build { raw, errors, warnings, deps, files, took }
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

/// Адреса ссылок `#see(…)`: существует ли цель, и куда вести.
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
    use std::fs;

    use super::*;

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
