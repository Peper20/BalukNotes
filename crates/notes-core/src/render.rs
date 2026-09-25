//! HTML-документы заметки (по одному на тему) → одна страница.
//!
//! Работаем с деревом `typst-html` до сериализации, а не с текстом HTML:
//!
//! 1. **Склейка тем.** Текст одинаков во всех темах, различаются только
//!    рисунки (SVG с вшитыми цветами). Каждый `div.k-frame` базового документа
//!    получает по варианту рисунка на тему: `div.k-frame-v[data-theme]`,
//!    CSS показывает вариант текущей темы. Порядок рисунков во всех темах
//!    одинаковый — это проверяется.
//! 2. **Якоря заголовков.** Заголовку без метки даётся `id` из его текста
//!    (`Смена порта` → `Смена-порта`), всем — `data-k-anchor` с тем же слагом:
//!    так ссылка находит раздел и по тексту, и по метке.
//! 3. **Ссылки между заметками** (`a.k-link` из `#см`) получают адрес от
//!    [`LinkResolver`]; ссылки на несуществующие заметки помечаются.
//! 4. **Цвета кода.** В HTML подсветка идёт опорными цветами (см.
//!    `konspekt/code.typ`), здесь они становятся CSS-переменными темы.
//! 5. **Скобки в формулах.** Typst помечает парные скобки растягиваемыми, а
//!    Chrome рисует растягиваемую скобку с широкими полями: `f ( x )`. Если
//!    внутри нет высокого (дробей, корней, пределов, матриц), растягивать
//!    нечего — ставим `stretchy="false"`, и скобки плотные, как в PDF.

use std::collections::HashSet;
use std::sync::LazyLock;

use ecow::EcoVec;
use serde::{Deserialize, Serialize};
use typst::model::Document as _;
use typst_html::tag::mathml;
use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlFrame, HtmlNode, HtmlOptions, HtmlTag, attr, tag};

// Константой (HtmlAttr::constant) интернируются только имена до 12 символов.
static DATA_THEME: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-theme"));
static DATA_TARGET: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-k-target"));
static DATA_ANCHOR: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-k-anchor"));

fn attr_name(name: &str) -> HtmlAttr {
    HtmlAttr::intern(name).expect("имя атрибута задано в коде и верно")
}

const STRETCHY: HtmlAttr = HtmlAttr::constant("stretchy");

/// Скобки, которые Typst растягивает по содержимому.
const FENCES: &[&str] = &["(", ")", "[", "]", "{", "}", "|", "‖", "⟨", "⟩", "⌊", "⌋", "⌈", "⌉"];

/// Элементы MathML, ради которых скобку стоит растягивать.
const TALL: [HtmlTag; 7] =
    [mathml::mfrac, mathml::mtable, mathml::msqrt, mathml::mroot, mathml::munderover, mathml::munder, mathml::mover];

/// Опорные цвета подсветки кода (`konspekt/code.typ`) → переменные CSS.
const CODE_COLORS: [(&str, &str); 8] = [
    ("#010100", "text"),
    ("#010101", "key"),
    ("#010102", "type"),
    ("#010103", "string"),
    ("#010104", "number"),
    ("#010105", "comment"),
    ("#010106", "function"),
    ("#010107", "hl"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    /// Уровень оформления: 1 — глава книги, 2 — раздел, …
    pub level: u8,
    pub id: String,
    /// Слаг текста заголовка — по нему ссылка находит раздел.
    pub anchor: String,
    pub text: String,
}

/// Ссылка из заметки, как она написана в `#см(…)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkRef {
    pub target: String,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    let mut state =
        State { themes: &themes, frames, frame: 0, ids, links, out_headings: vec![], out_links: vec![], tags: vec![] };
    walk_mut(base.root_mut(), &mut |el| state.visit(el));
    let State { out_headings: headings, out_links, tags, .. } = state;

    let title = base.info().title.as_ref().map(ToString::to_string);
    let html = typst_html::html(&base, &HtmlOptions::default())
        .map_err(|errs| errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; "))?;
    let (styles, body) = split_html(&html);
    Ok(Rendered { title, styles, body: replace_code_colors(&body), headings, links: out_links, tags })
}

struct State<'a> {
    themes: &'a [String],
    /// Рисунки небазовых тем, по порядку.
    frames: Vec<Vec<HtmlFrame>>,
    frame: usize,
    ids: HashSet<String>,
    links: &'a dyn LinkResolver,
    out_headings: Vec<Heading>,
    out_links: Vec<LinkRef>,
    tags: Vec<String>,
}

impl State<'_> {
    fn visit(&mut self, el: &mut HtmlElement) {
        if has_class(el, "k-frame") {
            self.theme_frame(el);
        } else if is_heading(el) && has_class(el, "k-h") {
            self.heading(el);
        } else if el.tag == tag::a && has_class(el, "k-link") {
            self.link(el);
        } else if el.tag == mathml::mrow {
            tighten_fences(el);
        } else if el.tag == tag::ul && has_class(el, "k-tags") {
            self.tags.extend(el.children.iter().filter_map(|c| match c {
                HtmlNode::Element(li) => Some(text_of(li, None)),
                _ => None,
            }));
        }
    }

    /// div.k-frame > svg  →  div.k-frame > div.k-frame-v[data-theme] × темы.
    fn theme_frame(&mut self, el: &mut HtmlElement) {
        let Some(base) = el.children.iter().find_map(|c| match c {
            HtmlNode::Frame(f) => Some(f.clone()),
            _ => None,
        }) else {
            return;
        };
        let i = self.frame;
        self.frame += 1;
        let mut children = EcoVec::new();
        for (t, theme) in self.themes.iter().enumerate() {
            let frame = if t == 0 {
                base.clone()
            } else {
                // id и якоря внутри SVG есть только у базового варианта,
                // иначе на странице окажутся повторяющиеся id.
                let mut f = self.frames[t - 1][i].clone();
                f.id = None;
                f.anchors = EcoVec::new();
                f
            };
            children.push(HtmlNode::Element(
                HtmlElement::new(tag::div)
                    .with_attr(attr::class, "k-frame-v")
                    .with_attr(*DATA_THEME, theme.as_str())
                    .with_children(EcoVec::from([HtmlNode::Frame(frame)])),
            ));
        }
        el.children = children;
    }

    fn heading(&mut self, el: &mut HtmlElement) {
        let text = text_of(el, Some("k-num")).trim().to_owned();
        let anchor = slug(&text);
        let id = if let Some(id) = el.attrs.get(attr::id) {
            id.to_string()
        } else {
            let id = unique(&anchor, &mut self.ids);
            el.attrs.push(attr::id, id.as_str());
            id
        };
        el.attrs.push(*DATA_ANCHOR, anchor.as_str());
        // Класс k-hN — уровень оформления (глава, раздел, …), не тег.
        let level = class_level(el).unwrap_or(2);
        self.out_headings.push(Heading { level, id, anchor, text });
    }

    fn link(&mut self, el: &mut HtmlElement) {
        let Some(target) = el.attrs.get(*DATA_TARGET).map(ToString::to_string) else { return };
        let anchor = el.attrs.get(*DATA_ANCHOR).map(ToString::to_string);
        if let Some(href) = self.links.href(&target, anchor.as_deref()) {
            el.attrs.push(attr::href, href);
        } else {
            if let Some(class) = el.attrs.get_mut(attr::class) {
                class.push_str(" k-link-broken");
            }
            el.attrs.push(attr::title, format!("нет заметки «{target}»"));
        }
        let link = LinkRef { target, anchor };
        if !self.out_links.contains(&link) {
            self.out_links.push(link);
        }
    }
}

/// `mrow` вида `( … )` без высокого внутри → скобки не растягиваются.
fn tighten_fences(row: &mut HtmlElement) {
    let elems: Vec<usize> =
        row.children.iter().enumerate().filter_map(|(i, c)| matches!(c, HtmlNode::Element(_)).then_some(i)).collect();
    let (Some(&first), Some(&last)) = (elems.first(), elems.last()) else { return };
    if first == last {
        return;
    }
    let is_fence = |node: &HtmlNode| {
        matches!(node, HtmlNode::Element(e)
            if e.tag == mathml::mo && e.attrs.get(STRETCHY).is_none() && FENCES.contains(&text_of(e, None).as_str()))
    };
    if !is_fence(&row.children[first]) || !is_fence(&row.children[last]) {
        return;
    }
    let mut tall = false;
    for node in &row.children[first + 1..last] {
        if let HtmlNode::Element(e) = node {
            walk(e, &mut |x| tall |= TALL.contains(&x.tag));
        }
    }
    if tall {
        return;
    }
    let children = row.children.make_mut();
    for i in [first, last] {
        if let HtmlNode::Element(mo) = &mut children[i] {
            mo.attrs.push(STRETCHY, "false");
        }
    }
}

/// Слаг для якоря: буквы и цифры (любого алфавита), `-` и `_`; остальное —
/// разделители, схлопываются в один `-`. Регистр сохраняется.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut sep = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            if sep && !out.is_empty() {
                out.push('-');
            }
            sep = false;
            out.push(ch);
        } else {
            sep = true;
        }
    }
    if out.is_empty() { "раздел".into() } else { out }
}

fn unique(base: &str, used: &mut HashSet<String>) -> String {
    let mut id = base.to_owned();
    let mut n = 2;
    while used.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    used.insert(id.clone());
    id
}

fn has_class(el: &HtmlElement, class: &str) -> bool {
    el.attrs.get(attr::class).is_some_and(|c| c.split_whitespace().any(|c| c == class))
}

fn is_heading(el: &HtmlElement) -> bool {
    [tag::h2, tag::h3, tag::h4, tag::h5, tag::h6].contains(&el.tag)
}

/// `k-h2` → 2.
fn class_level(el: &HtmlElement) -> Option<u8> {
    el.attrs.get(attr::class)?.split_whitespace().find_map(|c| c.strip_prefix("k-h").and_then(|n| n.parse().ok()))
}

fn text_of(el: &HtmlElement, skip_class: Option<&str>) -> String {
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

fn walk(el: &HtmlElement, f: &mut dyn FnMut(&HtmlElement)) {
    f(el);
    for child in &el.children {
        if let HtmlNode::Element(e) = child {
            walk(e, f);
        }
    }
}

fn walk_mut(el: &mut HtmlElement, f: &mut dyn FnMut(&mut HtmlElement)) {
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

fn replace_code_colors(html: &str) -> String {
    let mut out = html.to_owned();
    for (hex, name) in CODE_COLORS {
        // Только в CSS-свойствах (style="color: …"): в SVG опорных цветов нет,
        // но и случайное совпадение в тексте не трогаем.
        out = out.replace(&format!("color: {hex}"), &format!("color: var(--k-code-{name})"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Смена порта"), "Смена-порта");
        assert_eq!(slug("  Вход по ключу. "), "Вход-по-ключу");
        assert_eq!(slug("C++ и сборка"), "C-и-сборка");
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

    #[test]
    fn code_colors_become_variables() {
        let got = replace_code_colors(r##"<span style="color: #010101">fn</span> fill="#010101""##);
        assert_eq!(got, r##"<span style="color: var(--k-code-key)">fn</span> fill="#010101""##);
    }
}
