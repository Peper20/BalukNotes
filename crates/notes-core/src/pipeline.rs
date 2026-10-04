//! Building a note is a pure function of files: the source -> compiling by
//! theme ([`crate::world`]) -> rendering ([`crate::render`], the passes
//! [`crate::passes`]) -> a raw page, then the passes for the settings
//! ([`crate::finish`]: figures).
//!
//! No cache and no locks: the layers above add them ([`crate::pages`],
//! [`crate::page_cache`]). The [`Pipeline`] interface lets those layers be
//! tested without compiling Typst (by a fake build).

use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::diag::Diagnostic;
use crate::figures::FigureOptions;
use crate::finish;
use crate::fonts::Fonts;
use crate::render::{self, LinkResolver, Rendered};
use crate::themes::ThemeSet;
use crate::vault::{Entry, NoteId, Vault};
use crate::version::{Dep, combine};
use crate::world::Compiler;
pub use crate::world::Priority;

/// The result of a build.
#[derive(Debug)]
pub struct Build {
    /// Rendering before figure processing; `None` on an error.
    pub raw: Option<Rendered>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// The files read.
    pub deps: Vec<Dep>,
    /// The version of the files when they were read ([`crate::version`]).
    pub files: String,
    pub took: Duration,
}

/// Building a note.
pub trait Pipeline: Send + Sync + fmt::Debug {
    /// Builds a note (without figure processing).
    fn build(&self, entry: &Entry, priority: Priority) -> Build;

    /// Processes figures for the settings.
    fn finish(&self, raw: &Rendered, opts: FigureOptions) -> Rendered;

    /// Frees the memory of builds (after a warming round).
    fn release_memory(&self) {}
}

/// A build by the Typst compiler.
#[derive(Debug)]
pub struct TypstPipeline {
    vault: Vault,
    compiler: Compiler,
    themes: Arc<ThemeSet>,
}

impl TypstPipeline {
    pub fn new(vault: Vault, compiler: Compiler, themes: Arc<ThemeSet>) -> Self {
        Self { vault, compiler, themes }
    }

    pub fn themes(&self) -> &ThemeSet {
        &self.themes
    }

    pub fn themes_arc(&self) -> Arc<ThemeSet> {
        self.themes.clone()
    }

    pub fn fonts(&self) -> &Fonts {
        self.compiler.fonts()
    }

    pub fn compiler(&self) -> &Compiler {
        &self.compiler
    }

    /// [`lint`](crate::lint) warnings for the vault files the note is built
    /// from (the library and packages are not checked).
    fn lint(&self, deps: &[Dep]) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for dep in deps {
            let Dep::Vault(path) = dep else { continue };
            if Path::new(path).extension().is_none_or(|e| e != "typ") {
                continue;
            }
            let Ok(text) = self.vault.read_text(path) else { continue };
            for l in crate::lint::lint(&text, self.themes.languages()) {
                let (line, column) = crate::lint::line_column(&text, l.offset);
                out.push(Diagnostic::warning_at(l.message, format!("/{path}"), line, column, l.hint));
            }
        }
        out
    }
}

impl Pipeline for TypstPipeline {
    fn build(&self, entry: &Entry, priority: Priority) -> Build {
        let started = Instant::now();
        let _span = tracing::debug_span!("build", id = %entry.id).entered();
        let compilation = self.compiler.compile_html(&entry.main, &self.themes.names(), priority);
        let links = VaultLinks { vault: &self.vault };
        let (raw, errors) = match compilation.docs {
            Ok(docs) => match render::render(docs, &links) {
                Ok(r) => (Some(r), vec![]),
                Err(message) => (None, vec![Diagnostic::error(message)]),
            },
            Err(errors) => (None, errors),
        };
        let took = started.elapsed();
        tracing::debug!(id = %entry.id, ms = took.as_millis(), errors = errors.len(), "built");
        let files = combine(&compilation.deps);
        let deps: Vec<Dep> = compilation.deps.into_iter().map(|(d, _)| d).collect();
        let mut warnings = compilation.warnings;
        if let Some((tags, attrs, urls)) = raw.as_ref().and_then(|r| r.sanitizer) {
            warnings.push(Diagnostic {
                severity: crate::diag::DiagSeverity::Warning,
                message: format!("HTML sanitized: removed {tags} tags, {attrs} attributes, {urls} dangerous URLs"),
                file: None,
                line: None,
                column: None,
                hints: vec![],
            });
        }
        warnings.extend(self.lint(&deps));
        Build { raw, errors, warnings, deps, files, took }
    }

    /// Passes after the cache ([`crate::finish`]): figures for the settings.
    fn finish(&self, raw: &Rendered, opts: FigureOptions) -> Rendered {
        let themes = self.themes.names();
        finish::finish(raw, &finish::Settings { themes: &themes, opts }, finish::FINISH)
    }

    fn release_memory(&self) {
        self.compiler.release_memory();
    }
}

/// Addresses of `#see(...)` links: whether the target exists and where to go
/// (`/n/Network/SSH#anchor`, a client URL).
struct VaultLinks<'a> {
    vault: &'a Vault,
}

impl LinkResolver for VaultLinks<'_> {
    fn href(&self, target: &str, anchor: Option<&str>) -> Option<String> {
        let id = NoteId::new(target).ok()?;
        self.vault.entry(&id).ok()?;
        let fragment = anchor.map(|a| format!("#{}", encode(&render::slug(a)))).unwrap_or_default();
        Some(format!("/n/{}{fragment}", encode(id.as_str())))
    }
}

/// Encodes what would otherwise change the meaning of a URL. Cyrillic stays as
/// is: browsers understand it, and the URL stays readable.
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
        let server = VaultLinks { vault: &vault };
        assert_eq!(server.href("Сеть/SSH", Some("Смена порта")).unwrap(), "/n/Сеть/SSH#Смена-порта");
        assert_eq!(server.href("Итоги 2026", None).unwrap(), "/n/Итоги%202026");
        assert_eq!(server.href("Сеть/Nginx", None), None);
        assert_eq!(server.href("../etc/passwd", None), None);
    }
}
