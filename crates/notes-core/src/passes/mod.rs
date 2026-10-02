//! Обработка HTML заметки — явная цепочка проходов.
//!
//! Сырая отрисовка ([`crate::render::render`]) — два этапа до кэша:
//!
//! 1. [`TREE`] — по дереву `typst-html` до сериализации. Каждый проход —
//!    посетитель элементов ([`TreePass`]) с общим [`Context`]: темы и их
//!    рисунки, занятые `id`, адреса ссылок; итоги (заголовки, ссылки, теги)
//!    копятся там же.
//! 2. [`TEXT`] — по тексту сырой страницы ([`TextPass`]).
//!
//! После кэша, под настройки читателя, — [`crate::finish`] (рисунки).
//! Глава книги ([`crate::book`]) — не проход, а вид готовой страницы по
//! запросу.
//!
//! Новый проход — модуль здесь и одна строка в списке. Время каждого
//! прохода — в логе (`RUST_LOG=notes_core=debug`, строка «проход»). Файлы
//! этого каталога входят в метку кэша на диске (`build.rs`): правка прохода
//! пересобирает заметки.

mod anchors;
mod code_colors;
mod fences;
mod heading_html;
mod links;
mod sanitize;
mod tags;
mod themes;

use std::collections::HashSet;
use std::time::Instant;

use typst_html::{HtmlElement, HtmlFrame};

use crate::render::{Heading, LinkRef, LinkResolver, Rendered, walk_mut};

/// Проходы по дереву — по порядку.
// Санитизация — первой: удаляет опасные элементы и атрибуты до сериализации.
pub const TREE: &[TreePass] = &[sanitize::PASS, themes::PASS, anchors::PASS, links::PASS, fences::PASS, tags::PASS];

/// Проходы по тексту сырой страницы — по порядку.
pub const TEXT: &[TextPass] = &[code_colors::PASS, heading_html::PASS];

/// Проход по дереву: `visit` вызывается для каждого элемента (сначала
/// родитель, потом дети — уже после его правки).
#[derive(Debug, Clone, Copy)]
pub struct TreePass {
    pub name: &'static str,
    pub visit: fn(&mut Context<'_>, &mut HtmlElement),
}

/// Проход по тексту сырой страницы.
#[derive(Debug, Clone, Copy)]
pub struct TextPass {
    pub name: &'static str,
    pub run: fn(&mut Rendered),
}

/// Общий контекст проходов одной страницы.
pub struct Context<'a> {
    /// Темы по порядку; первая — базовый документ.
    pub themes: &'a [String],
    /// Рисунки небазовых тем, по порядку появления.
    pub frames: Vec<Vec<HtmlFrame>>,
    /// Сколько рисунков базового документа уже склеено.
    pub frame: usize,
    /// Занятые `id` страницы.
    pub ids: HashSet<String>,
    pub links: &'a dyn LinkResolver,
    /// Итоги: заголовки, ссылки (без повторов), теги.
    pub headings: Vec<Heading>,
    pub out_links: Vec<LinkRef>,
    pub tags: Vec<String>,
    /// Санитизация: сколько удалено.
    pub removed_tags: usize,
    pub removed_attrs: usize,
    pub removed_urls: usize,
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").field("themes", &self.themes).field("frame", &self.frame).finish_non_exhaustive()
    }
}

impl<'a> Context<'a> {
    pub fn new(
        themes: &'a [String],
        frames: Vec<Vec<HtmlFrame>>,
        ids: HashSet<String>,
        links: &'a dyn LinkResolver,
    ) -> Self {
        Self {
            themes,
            frames,
            frame: 0,
            ids,
            links,
            headings: vec![],
            out_links: vec![],
            tags: vec![],
            removed_tags: 0,
            removed_attrs: 0,
            removed_urls: 0,
        }
    }
}

/// Выполнить проходы по дереву.
pub fn run_tree(root: &mut HtmlElement, ctx: &mut Context<'_>, passes: &[TreePass]) {
    for pass in passes {
        timed(pass.name, || walk_mut(root, &mut |el| (pass.visit)(ctx, el)));
    }
}

/// Выполнить проходы по тексту.
pub fn run_text(page: &mut Rendered, passes: &[TextPass]) {
    for pass in passes {
        timed(pass.name, || (pass.run)(page));
    }
}

/// Выполнить и записать время в лог.
pub fn timed<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let out = f();
    tracing::debug!(pass = name, us = started.elapsed().as_micros(), "проход");
    out
}

#[cfg(test)]
pub(crate) mod test_util {
    use ecow::EcoVec;
    use typst::syntax::Span;
    use typst_html::{HtmlAttr, HtmlElement, HtmlNode, HtmlTag};

    use super::Context;
    use crate::render::LinkResolver;

    /// Ссылки: существуют заметки `A` и `B`.
    pub struct Links;

    impl LinkResolver for Links {
        fn href(&self, target: &str, anchor: Option<&str>) -> Option<String> {
            ["A", "B"]
                .contains(&target)
                .then(|| format!("/n/{target}{}", anchor.map(|a| format!("#{a}")).unwrap_or_default()))
        }
    }

    pub fn ctx(themes: &[String]) -> Context<'_> {
        Context::new(themes, vec![], std::collections::HashSet::default(), &Links)
    }

    pub fn text(s: &str) -> HtmlNode {
        HtmlNode::Text(s.into(), Span::detached())
    }

    pub fn el(tag: HtmlTag, attrs: &[(HtmlAttr, &str)], children: Vec<HtmlNode>) -> HtmlElement {
        let mut e = HtmlElement::new(tag).with_children(EcoVec::from(children));
        for (k, v) in attrs {
            e.attrs.push(*k, *v);
        }
        e
    }

    pub fn node(e: HtmlElement) -> HtmlNode {
        HtmlNode::Element(e)
    }
}
