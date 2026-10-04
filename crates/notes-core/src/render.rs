//! HTML documents of a note (one per theme) -> one raw page.
//!
//! The work is on the `typst-html` tree before serializing, not on HTML text.
//! The processing itself is a chain of [`crate::passes`]: joining themes,
//! heading anchors, links between notes, brackets in formulas, tags (on the
//! tree), then code colors (on the text). Here: the page types, the check that
//! every theme has the same number of figures, serializing and helpers shared
//! by the passes.

use std::collections::HashSet;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use typst::model::Document as _;
use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlFrame, HtmlNode, HtmlOptions, attr};

use crate::passes::{self, Context};

// Only names up to 12 characters are interned as a constant (HtmlAttr::constant).
pub(crate) static DATA_TARGET: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-k-target"));
pub(crate) static DATA_ANCHOR: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-k-anchor"));

#[expect(clippy::expect_used, reason = "names are literals in this crate, checked by its tests")]
pub(crate) fn attr_name(name: &str) -> HtmlAttr {
    HtmlAttr::intern(name).expect("attribute names are set in code and valid")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Heading {
    /// The design level: 1 is a book chapter, 2 a section, ...
    pub level: u8,
    pub id: String,
    /// The slug of the heading text: a link finds the section by it.
    pub anchor: String,
    pub text: String,
    /// Heading HTML for the contents (formulas, emphasis) without the number
    /// and links; `None` for a plain-text heading (`passes::heading_html`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub html: Option<String>,
}

/// A link from a note as written in `#see(...)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LinkRef {
    pub target: String,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Rendered {
    pub title: Option<String>,
    /// `<style>` from `<head>` (MathML styles from Typst).
    pub styles: String,
    /// The contents of `<body>`.
    pub body: String,
    pub headings: Vec<Heading>,
    pub links: Vec<LinkRef>,
    pub tags: Vec<String>,
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub sanitizer: Option<(usize, usize, usize)>,
}

/// The URL of a link to a note. `None` if there is no such note.
pub trait LinkResolver {
    fn href(&self, target: &str, anchor: Option<&str>) -> Option<String>;
}

/// Joins the theme documents (the first is the base) into a page.
pub fn render(docs: Vec<(String, HtmlDocument)>, links: &dyn LinkResolver) -> Result<Rendered, String> {
    render_with(docs, links, passes::TREE)
}

/// [`render`] with the given tree passes (tests compare pages with and without one).
pub fn render_with(
    docs: Vec<(String, HtmlDocument)>,
    links: &dyn LinkResolver,
    tree: &[passes::TreePass],
) -> Result<Rendered, String> {
    let mut docs = docs.into_iter();
    let (base_theme, mut base) = docs.next().ok_or("no documents")?;

    let mut themes = vec![base_theme];
    let mut frames: Vec<Vec<HtmlFrame>> = Vec::new();
    for (theme, doc) in docs {
        frames.push(collect_frames(doc.root()));
        themes.push(theme);
    }
    let base_count = count_frames(base.root());
    if let Some((i, f)) = frames.iter().enumerate().find(|(_, f)| f.len() != base_count) {
        return Err(format!(
            "theme \"{}\" has {} figures and \"{}\" has {base_count}: a figure depends on the theme by more than color",
            themes[i + 1],
            f.len(),
            themes[0],
        ));
    }

    let mut ids = HashSet::new();
    walk(base.root(), &mut |el| {
        if let Some(id) = el.attrs.get(attr::id) {
            ids.insert(id.to_string());
        }
    });

    let mut ctx = Context::new(&themes, frames, ids, links);
    passes::run_tree(base.root_mut(), &mut ctx, tree);
    let Context { headings, out_links, tags, removed_tags, removed_attrs, removed_urls, .. } = ctx;

    let title = base.info().title.as_ref().map(ToString::to_string);
    let html = passes::timed("serialize", || typst_html::html(&base, &HtmlOptions::default()))
        .map_err(|errs| errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; "))?;
    let (styles, body) = split_html(&html);
    let sanitizer =
        (removed_tags + removed_attrs + removed_urls > 0).then_some((removed_tags, removed_attrs, removed_urls));
    let mut page = Rendered { title, styles, body, headings, links: out_links, tags, sanitizer };
    passes::run_text(&mut page, passes::TEXT);
    Ok(page)
}

/// Ordinary punctuation separates words in a slug, like a space. Other
/// characters (`+ # % & @ = * → <` ...) tell headings apart ("C" and "C++",
/// "a=b" and "a b") and stay in the slug as is; in a URL they are encoded by
/// [`crate::pipeline::encode`] and the client's `encodeURIComponent`.
const SEPARATORS: &str = ".,;:!?'\"`«»„“”‘’()[]{}/\\|—–…·";

/// A slug for an anchor: letters and digits (of any alphabet), `-`, `_` and
/// symbols (`C++` -> `C++`, `C#` -> `C#`); spaces and [`SEPARATORS`] separate
/// words and collapse into one `-`. Case is kept. Nothing left gives "раздел"
/// ("section"): it is part of note URLs, so it stays as is.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut sep = false;
    for ch in text.chars() {
        if ch.is_whitespace() || ch.is_control() || SEPARATORS.contains(ch) {
            sep = true;
            continue;
        }
        if sep && !out.is_empty() {
            out.push('-');
        }
        sep = false;
        out.push(ch);
    }
    if out.is_empty() { "раздел".into() } else { out }
}

pub(crate) fn unique(base: &str, used: &mut HashSet<String>) -> String {
    let mut id = base.to_owned();
    let mut n = 2;
    while used.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    used.insert(id.clone());
    id
}

pub(crate) fn has_class(el: &HtmlElement, class: &str) -> bool {
    el.attrs.get(attr::class).is_some_and(|c| c.split_whitespace().any(|c| c == class))
}

pub(crate) fn text_of(el: &HtmlElement, skip_class: Option<&str>) -> String {
    let mut out = String::new();
    collect_text(el, skip_class, &mut out);
    out
}

fn collect_text(el: &HtmlElement, skip_class: Option<&str>, out: &mut String) {
    for child in &el.children {
        match child {
            HtmlNode::Text(text, _) => out.push_str(text),
            HtmlNode::Element(e) if !skip_class.is_some_and(|c| has_class(e, c)) => collect_text(e, skip_class, out),
            _ => {}
        }
    }
}

pub(crate) fn walk(el: &HtmlElement, f: &mut dyn FnMut(&HtmlElement)) {
    f(el);
    for child in &el.children {
        if let HtmlNode::Element(e) = child {
            walk(e, f);
        }
    }
}

pub(crate) fn walk_mut(el: &mut HtmlElement, f: &mut dyn FnMut(&mut HtmlElement)) {
    f(el);
    for child in el.children.make_mut() {
        if let HtmlNode::Element(e) = child {
            walk_mut(e, f);
        }
    }
}

fn count_frames(root: &HtmlElement) -> usize {
    let mut n = 0;
    walk(root, &mut |el| n += usize::from(has_class(el, "k-frame")));
    n
}

fn collect_frames(root: &HtmlElement) -> Vec<HtmlFrame> {
    let mut out = Vec::new();
    walk(root, &mut |el| {
        if has_class(el, "k-frame")
            && let Some(f) = el.children.iter().find_map(|c| match c {
                HtmlNode::Frame(f) => Some(f.clone()),
                _ => None,
            })
        {
            out.push(f);
        }
    });
    out
}

/// Styles of `<head>` and the contents of `<body>` from a whole HTML page.
fn split_html(html: &str) -> (String, String) {
    let between = |open: &str, close: &str| {
        let start = html.find(open).map(|i| i + open.len())?;
        let end = html[start..].find(close)? + start;
        Some(&html[start..end])
    };
    let head = between("<head>", "</head>").unwrap_or_default();
    let mut styles = String::new();
    let mut rest = head;
    while let Some(start) = rest.find("<style>") {
        let Some(len) = rest[start..].find("</style>") else { break };
        let end = start + len + "</style>".len();
        styles.push_str(&rest[start..end]);
        rest = &rest[end..];
    }
    let body = between("<body>", "</body>").unwrap_or(html);
    (styles, body.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Смена порта"), "Смена-порта");
        assert_eq!(slug("  Вход по ключу. "), "Вход-по-ключу");
        assert_eq!(slug("C++ и сборка"), "C++-и-сборка");
        assert_eq!(slug("C"), "C");
        assert_eq!(slug("C#"), "C#");
        assert_eq!(slug("50% готово"), "50%-готово");
        assert_eq!(slug("a+b"), "a+b");
        assert_eq!(slug("+"), "+");
        assert_eq!(slug("a=b"), "a=b");
        assert_eq!(slug("a = b"), "a-=-b");
        assert_ne!(slug("a=b"), slug("a b"));
        assert_eq!(slug("A → B & C"), "A-→-B-&-C");
        assert_eq!(slug("Функция f(x), «итог»!"), "Функция-f-x-итог");
        assert_eq!(slug("sec-классы"), "sec-классы");
        assert_eq!(slug("«»"), "раздел");
    }

    #[test]
    fn unique_ids() {
        let mut used = HashSet::from(["Итог".to_owned()]);
        assert_eq!(unique("Итог", &mut used), "Итог-2");
        assert_eq!(unique("Итог", &mut used), "Итог-3");
        assert_eq!(unique("План", &mut used), "План");
    }

    #[test]
    fn splits_head_and_body() {
        let html = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><style>a{}</style><title>x</title><style>b{}</style></head><body><p>текст</p></body></html>";
        let (styles, body) = split_html(html);
        assert_eq!(styles, "<style>a{}</style><style>b{}</style>");
        assert_eq!(body, "<p>текст</p>");
    }
}
