//! HTML-документы заметки (по одному на тему) → одна сырая страница.
//!
//! Работаем с деревом `typst-html` до сериализации, а не с текстом HTML.
//! Сама обработка — цепочка проходов [`crate::passes`]: склейка тем, якоря
//! заголовков, ссылки между заметками, скобки в формулах, теги (по дереву),
//! затем цвета кода (по тексту). Здесь — типы страницы, проверка, что
//! рисунков во всех темах поровну, сериализация и общие помощники проходов.

use std::collections::HashSet;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use typst::model::Document as _;
use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlFrame, HtmlNode, HtmlOptions, attr};

use crate::passes::{self, Context};

// Константой (HtmlAttr::constant) интернируются только имена до 12 символов.
pub(crate) static DATA_TARGET: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-k-target"));
pub(crate) static DATA_ANCHOR: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-k-anchor"));

pub(crate) fn attr_name(name: &str) -> HtmlAttr {
    HtmlAttr::intern(name).expect("имя атрибута задано в коде и верно")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Heading {
    /// Уровень оформления: 1 — глава книги, 2 — раздел, …
    pub level: u8,
    pub id: String,
    /// Слаг текста заголовка — по нему ссылка находит раздел.
    pub anchor: String,
    pub text: String,
    /// HTML заголовка для оглавления (формулы, выделение) — без номера и
    /// ссылок; `None` — заголовок из одного текста (`passes::heading_html`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub html: Option<String>,
}

/// Ссылка из заметки, как она написана в `#see(…)`.
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
    /// `<style>` из `<head>` (стили MathML от Typst).
    pub styles: String,
    /// Содержимое `<body>`.
    pub body: String,
    pub headings: Vec<Heading>,
    pub links: Vec<LinkRef>,
    pub tags: Vec<String>,
}

/// Адрес ссылки на заметку. `None` — такой заметки нет.
pub trait LinkResolver {
    fn href(&self, target: &str, anchor: Option<&str>) -> Option<String>;
}

/// Склеивает документы тем (первый — базовый) в страницу.
pub fn render(docs: Vec<(String, HtmlDocument)>, links: &dyn LinkResolver) -> Result<Rendered, String> {
    let mut docs = docs.into_iter();
    let (base_theme, mut base) = docs.next().ok_or("нет ни одного документа")?;

    let mut themes = vec![base_theme];
    let mut frames: Vec<Vec<HtmlFrame>> = Vec::new();
    for (theme, doc) in docs {
        frames.push(collect_frames(doc.root()));
        themes.push(theme);
    }
    let base_count = count_frames(base.root());
    if let Some((i, f)) = frames.iter().enumerate().find(|(_, f)| f.len() != base_count) {
        return Err(format!(
            "в теме «{}» рисунков {}, а в «{}» — {base_count}: рисунок зависит от темы не только цветом",
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
    passes::run_tree(base.root_mut(), &mut ctx, passes::TREE);
    let Context { headings, out_links, tags, .. } = ctx;

    let title = base.info().title.as_ref().map(ToString::to_string);
    let html = passes::timed("сериализация", || typst_html::html(&base, &HtmlOptions::default()))
        .map_err(|errs| errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; "))?;
    let (styles, body) = split_html(&html);
    let mut page = Rendered { title, styles, body, headings, links: out_links, tags };
    passes::run_text(&mut page, passes::TEXT);
    Ok(page)
}

/// Знаки, которые в слаге становятся словами: иначе «C» и «C++» получили
/// бы один слаг (`C` и `C-2` по порядку), а ссылка по тексту — не тот раздел.
const SIGN_WORDS: [(char, &str); 5] = [('+', "plus"), ('#', "sharp"), ('%', "percent"), ('&', "and"), ('@', "at")];

/// Слаг для якоря: буквы и цифры (любого алфавита), `-` и `_`; знаки из
/// [`SIGN_WORDS`] — словами (`C++` → `C-plus-plus`); остальное —
/// разделители, схлопываются в один `-`. Регистр сохраняется.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut sep = false;
    let word = |out: &mut String, sep: &mut bool, w: &str| {
        if *sep && !out.is_empty() {
            out.push('-');
        }
        *sep = false;
        out.push_str(w);
    };
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            word(&mut out, &mut sep, ch.encode_utf8(&mut [0; 4]));
        } else if let Some((_, w)) = SIGN_WORDS.iter().find(|(c, _)| *c == ch) {
            sep = true;
            word(&mut out, &mut sep, w);
            sep = true;
        } else {
            sep = true;
        }
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

/// Из полного HTML — стили `<head>` и содержимое `<body>`.
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
        assert_eq!(slug("C++ и сборка"), "C-plus-plus-и-сборка");
        assert_eq!(slug("C"), "C");
        assert_eq!(slug("C#"), "C-sharp");
        assert_eq!(slug("50% готово"), "50-percent-готово");
        assert_eq!(slug("a+b"), "a-plus-b");
        assert_eq!(slug("+"), "plus");
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
