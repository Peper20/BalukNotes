//! Processing the note HTML: an explicit chain of passes.
//!
//! The raw rendering ([`crate::render::render`]) has two stages before the cache:
//!
//! 1. [`TREE`]: on the `typst-html` tree before serializing. Each pass is an
//!    element visitor ([`TreePass`]) with a shared [`Context`]: the themes and
//!    their figures, taken `id`s, link addresses; the results (headings,
//!    links, tags) collect there too.
//! 2. [`TEXT`]: on the text of the raw page ([`TextPass`]).
//!
//! After the cache, for the reader's settings: [`crate::finish`] (figures).
//! A book chapter ([`crate::book`]) is not a pass but a view of the finished
//! page on request.
//!
//! A new pass is a module here and one line in the list. Each pass's time is
//! in the log (`RUST_LOG=notes_core=debug`, the "pass" line). The files of
//! this directory are part of the disk cache label (`build.rs`): editing a
//! pass rebuilds the notes.

mod anchors;
mod code_colors;
mod fences;
mod heading_html;
mod links;
mod operators;
mod sanitize;
mod tags;
mod themes;

use std::collections::HashSet;
use std::time::Instant;

use typst_html::{HtmlElement, HtmlFrame};

use crate::render::{Heading, LinkRef, LinkResolver, Rendered, walk_mut};

/// Tree passes, in order.
// Sanitizing comes first: it removes dangerous elements and attributes before serializing.
pub const TREE: &[TreePass] =
    &[sanitize::PASS, themes::PASS, anchors::PASS, links::PASS, fences::PASS, operators::PASS, tags::PASS];

/// Passes over the text of the raw page, in order.
pub const TEXT: &[TextPass] = &[code_colors::PASS, heading_html::PASS];

/// A tree pass: `visit` is called for every element (the parent first, then
/// the children, after the parent was edited).
#[derive(Debug, Clone, Copy)]
pub struct TreePass {
    pub name: &'static str,
    pub visit: fn(&mut Context<'_>, &mut HtmlElement),
}

/// A pass over the text of the raw page.
#[derive(Debug, Clone, Copy)]
pub struct TextPass {
    pub name: &'static str,
    pub run: fn(&mut Rendered),
}

/// The context shared by the passes of one page.
pub struct Context<'a> {
    /// The themes in order; the first is the base document.
    pub themes: &'a [String],
    /// Figures of the non-base themes, in order of appearance.
    pub frames: Vec<Vec<HtmlFrame>>,
    /// How many figures of the base document are already joined.
    pub frame: usize,
    /// Taken `id`s of the page.
    pub ids: HashSet<String>,
    pub links: &'a dyn LinkResolver,
    /// Results: headings, links (no repeats), tags.
    pub headings: Vec<Heading>,
    pub out_links: Vec<LinkRef>,
    pub tags: Vec<String>,
    /// Sanitizing: how much was removed.
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

/// Runs the tree passes.
pub fn run_tree(root: &mut HtmlElement, ctx: &mut Context<'_>, passes: &[TreePass]) {
    for pass in passes {
        timed(pass.name, || walk_mut(root, &mut |el| (pass.visit)(ctx, el)));
    }
}

/// Runs the text passes.
pub fn run_text(page: &mut Rendered, passes: &[TextPass]) {
    for pass in passes {
        timed(pass.name, || (pass.run)(page));
    }
}

/// Runs `f` and logs its time.
pub fn timed<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let out = f();
    tracing::debug!(pass = name, us = started.elapsed().as_micros(), "pass");
    out
}

#[cfg(test)]
pub(crate) mod test_util {
    use ecow::EcoVec;
    use typst::syntax::Span;
    use typst_html::{HtmlAttr, HtmlElement, HtmlNode, HtmlTag};

    use super::Context;
    use crate::render::LinkResolver;

    /// Links: notes `A` and `B` exist.
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
