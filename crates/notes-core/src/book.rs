//! A book by chapters: the page of one chapter from the page of the whole book.
//!
//! A book is built whole (counters, links between chapters), but it is better
//! shown one chapter at a time: laying out a whole book takes hundreds of
//! milliseconds on a PC and seconds on a phone. Chapters are direct children of
//! `<article class="k-doc" data-doc="book">` from one `h2.k-h1` to the next;
//! everything before the first chapter (the title page) goes with the first.
//!
//! Works on the finished HTML (after [`crate::figures`]): cuts the text at the
//! boundaries of the article's direct children. The markup comes from
//! `typst-html`: tags are closed and `<` in text is escaped, so parsing by
//! tags is enough.
//! From the shared glyph set (`svg.k-glyphs`) a chapter gets only its own.
//!
//! Cutting costs one pass over the text (a big book of ~1.5 MB takes
//! milliseconds), so it is not cached.

use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use serde::Serialize;

use crate::notes::NotePage;
use crate::render::Rendered;

/// A chapter in the chapter list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Chapter {
    /// The chapter number (`data-num`) or empty.
    pub num: String,
    /// The title without the number.
    pub title: String,
    /// The `id` of the chapter heading.
    pub id: String,
}

/// Which chapter is shown and where the others are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BookView {
    /// The number of the shown chapter (from zero).
    pub chapter: u32,
    pub chapters: Vec<Chapter>,
    /// Anchor (`id` or `data-k-anchor`) -> chapter number. `id`s are unique; of
    /// equal `data-k-anchor`s ("Summary" in every chapter) the first one wins.
    pub anchors: BTreeMap<String, u32>,
}

/// Which chapter to show.
#[derive(Debug, Clone, Copy, Default)]
pub struct Select<'a> {
    pub chapter: Option<usize>,
    /// An anchor: the chapter that has it (if `chapter` is not set).
    pub anchor: Option<&'a str>,
}

/// The page of one book chapter. Not a book, or fewer than two chapters: `None`
/// (show it whole). A chapter number past the end gives the last chapter.
pub fn chapter_page(page: &NotePage, select: Select<'_>) -> Option<NotePage> {
    let rendered = page.rendered.as_deref()?;
    let layout = Layout::parse(&rendered.body)?;
    let view = layout.view(rendered);
    let k = select
        .chapter
        .or_else(|| select.anchor.and_then(|a| view.anchors.get(a)).map(|&k| k as usize))
        .unwrap_or(0)
        .min(layout.chapters.len() - 1);
    let body = layout.chapter_body(&rendered.body, k);
    Some(NotePage {
        id: page.id.clone(),
        kind: page.kind,
        version: page.version.clone(),
        rendered: Some(Arc::new(Rendered { body, ..rendered.clone() })),
        errors: page.errors.clone(),
        warnings: page.warnings.clone(),
        book: Some(BookView { chapter: u32::try_from(k).unwrap_or(u32::MAX), ..view }),
    })
}

/// The boundaries of the book parts in the page text.
#[derive(Debug, PartialEq, Eq)]
struct Layout {
    /// The hidden `<svg class="k-glyphs">` at the start of the page (or empty).
    sprite: Range<usize>,
    /// The article contents before the first chapter (the title page).
    intro: Range<usize>,
    chapters: Vec<ChapterPart>,
}

#[derive(Debug, PartialEq, Eq)]
struct ChapterPart {
    range: Range<usize>,
    /// The opening tag of the chapter heading, and the whole heading.
    heading_tag: Range<usize>,
    heading: Range<usize>,
}

const SPRITE_OPEN: &str = r#"<svg class="k-glyphs""#;

/// Elements without a closing tag.
const VOID: &[&str] =
    &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];

/// Elements whose text is not markup.
const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title"];

impl Layout {
    fn parse(body: &str) -> Option<Self> {
        let sprite = if body.starts_with(SPRITE_OPEN) {
            let end = body.find("</svg>")? + "</svg>".len();
            0..end
        } else {
            0..0
        };
        let content_start = find_book_article(body)?;
        let mut chapters: Vec<ChapterPart> = Vec::new();
        let mut depth = 0usize;
        let mut pos = content_start;
        let content_end = loop {
            let tag = next_tag(body, pos)?;
            pos = tag.end;
            match tag.kind {
                TagKind::Close => {
                    if depth == 0 {
                        break tag.start;
                    }
                    depth -= 1;
                }
                TagKind::Open { name, self_closing } => {
                    if depth == 0 && name == "h2" && has_class(&body[tag.start..tag.end], "k-h1") {
                        let close = body[tag.end..].find("</h2>")? + tag.end + "</h2>".len();
                        if let Some(last) = chapters.last_mut() {
                            last.range.end = tag.start;
                        }
                        chapters.push(ChapterPart {
                            range: tag.start..0,
                            heading_tag: tag.start..tag.end,
                            heading: tag.start..close,
                        });
                    }
                    if RAW_TEXT.contains(&name) {
                        // Together with the closing tag: the depth does not change.
                        let close = body[pos..].find(&format!("</{name}"))? + pos;
                        pos = body[close..].find('>')? + close + 1;
                    } else if !self_closing && !VOID.contains(&name) {
                        depth += 1;
                    }
                }
                TagKind::Other => {}
            }
        };
        if chapters.len() < 2 {
            return None;
        }
        let intro = content_start..chapters[0].range.start;
        if let Some(last) = chapters.last_mut() {
            last.range.end = content_end;
        }
        Some(Self { sprite, intro, chapters })
    }

    /// The chapter list and the anchor map.
    fn view(&self, rendered: &Rendered) -> BookView {
        // The title comes from the page headings (it is already text there), the
        // number from `data-num` of the heading.
        let body = &rendered.body;
        let chapters = self
            .chapters
            .iter()
            .map(|c| {
                let tag = &body[c.heading_tag.clone()];
                let id = attr(tag, "id").unwrap_or_default();
                let title = rendered
                    .headings
                    .iter()
                    .find(|h| h.id == id)
                    .map_or_else(|| strip_tags(&body[c.heading.clone()]), |h| h.text.clone());
                Chapter { num: attr(tag, "data-num").unwrap_or_default(), title, id }
            })
            .collect();
        let mut anchors = BTreeMap::new();
        for (k, c) in self.chapters.iter().enumerate() {
            let range = if k == 0 { self.intro.start..c.range.end } else { c.range.clone() };
            let k = u32::try_from(k).unwrap_or(u32::MAX);
            for (name, value) in anchor_attrs(&body[range]) {
                if name == "id" {
                    anchors.insert(value, k);
                } else {
                    anchors.entry(value).or_insert(k);
                }
            }
        }
        BookView { chapter: 0, chapters, anchors }
    }

    /// The page text with one chapter `k` (the first one also gets the title page).
    fn chapter_body(&self, body: &str, k: usize) -> String {
        let part = &self.chapters[k];
        let content = if k == 0 { self.intro.start..part.range.end } else { part.range.clone() };
        let first = self.chapters[0].range.start;
        let last = self.chapters[self.chapters.len() - 1].range.end;
        let mut out = String::with_capacity(content.len() + (first - self.sprite.end) + (body.len() - last) + 1024);
        out.push_str(&used_glyphs(&body[self.sprite.clone()], &body[content.clone()]));
        out.push_str(&body[self.sprite.end..self.intro.start]);
        out.push_str(&body[content]);
        out.push_str(&body[last..]);
        out
    }
}

/// The end of the opening tag of the book article (the start of its contents).
fn find_book_article(body: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = body[from..].find("<article ") {
        let start = from + i;
        let tag = next_tag(body, start)?;
        let raw = &body[tag.start..tag.end];
        if has_class(raw, "k-doc") && attr(raw, "data-doc").as_deref() == Some("book") {
            return Some(tag.end);
        }
        from = tag.end;
    }
    None
}

#[derive(Debug)]
enum TagKind<'a> {
    Open {
        name: &'a str,
        self_closing: bool,
    },
    Close,
    /// A comment, `<!doctype>`.
    Other,
}

#[derive(Debug)]
struct Tag<'a> {
    start: usize,
    end: usize,
    kind: TagKind<'a>,
}

/// The next tag from `from`: `>` inside quotes does not close the tag.
fn next_tag(s: &str, from: usize) -> Option<Tag<'_>> {
    let start = from + s[from..].find('<')?;
    let rest = &s[start..];
    if rest.starts_with("<!--") {
        let end = start + rest.find("-->")? + 3;
        return Some(Tag { start, end, kind: TagKind::Other });
    }
    let mut quote = None;
    let mut end = None;
    for (i, b) in rest.bytes().enumerate().skip(1) {
        match (quote, b) {
            (None, b'"' | b'\'') => quote = Some(b),
            (Some(q), _) if q == b => quote = None,
            (None, b'>') => {
                end = Some(start + i + 1);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    let raw = &s[start..end];
    let kind = if raw.starts_with("</") {
        TagKind::Close
    } else if raw.starts_with("<!") || raw.starts_with("<?") {
        TagKind::Other
    } else {
        let name_end =
            raw[1..].find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/').map_or(raw.len(), |i| i + 1);
        TagKind::Open { name: &raw[1..name_end], self_closing: raw.ends_with("/>") }
    };
    Some(Tag { start, end, kind })
}

/// The attributes of an opening tag (values unescaped).
fn attrs(raw: &str) -> Vec<(&str, String)> {
    let mut out = Vec::new();
    let inner = raw.trim_start_matches('<').trim_end_matches('>').trim_end_matches('/');
    let mut rest = inner.find(|c: char| c.is_ascii_whitespace()).map_or("", |i| &inner[i..]);
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let name_end = rest.find(|c: char| c.is_ascii_whitespace() || c == '=').unwrap_or(rest.len());
        let name = &rest[..name_end];
        rest = &rest[name_end..];
        let after = rest.trim_start();
        if let Some(v) = after.strip_prefix('=') {
            let v = v.trim_start();
            let (value, tail) = if let Some(q @ ('"' | '\'')) = v.chars().next() {
                let body = &v[1..];
                let close = body.find(q).unwrap_or(body.len());
                (&body[..close], body.get(close + 1..).unwrap_or(""))
            } else {
                let close = v.find(|c: char| c.is_ascii_whitespace()).unwrap_or(v.len());
                (&v[..close], &v[close..])
            };
            out.push((name, unescape(value)));
            rest = tail;
        } else {
            out.push((name, String::new()));
            rest = after;
        }
        if name.is_empty() {
            break;
        }
    }
    out
}

fn attr(raw: &str, name: &str) -> Option<String> {
    attrs(raw).into_iter().find(|(k, _)| *k == name).map(|(_, v)| v)
}

fn has_class(raw: &str, class: &str) -> bool {
    attr(raw, "class").is_some_and(|c| c.split_ascii_whitespace().any(|c| c == class))
}

/// `id` and `data-k-anchor` of elements (not inside SVG: ids there are internal).
fn anchor_attrs(html: &str) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let mut svg = 0usize;
    let mut pos = 0;
    while let Some(tag) = next_tag(html, pos) {
        pos = tag.end;
        let raw = &html[tag.start..tag.end];
        match tag.kind {
            TagKind::Open { name, self_closing } => {
                if svg == 0 {
                    for (k, v) in attrs(raw) {
                        match k {
                            "id" if !v.is_empty() => out.push(("id", v)),
                            "data-k-anchor" if !v.is_empty() => out.push(("data-k-anchor", v)),
                            _ => {}
                        }
                    }
                }
                if name == "svg" && !self_closing {
                    svg += 1;
                }
                if RAW_TEXT.contains(&name)
                    && let Some(i) = html[pos..].find(&format!("</{name}"))
                {
                    pos += i;
                }
            }
            TagKind::Close if raw == "</svg>" => svg = svg.saturating_sub(1),
            _ => {}
        }
    }
    out
}

/// The glyph set with only those `html` refers to.
fn used_glyphs(sprite: &str, html: &str) -> String {
    if sprite.is_empty() {
        return String::new();
    }
    let mut used = HashSet::new();
    let mut rest = html;
    while let Some(i) = rest.find("href=\"#") {
        rest = &rest[i + 7..];
        let end = rest.find('"').unwrap_or(rest.len());
        used.insert(&rest[..end]);
        rest = &rest[end..];
    }
    let (Some(open), Some(close)) = (sprite.find("<defs>"), sprite.rfind("</defs>")) else {
        return sprite.to_owned();
    };
    let open = open + "<defs>".len();
    let mut out = String::from(&sprite[..open]);
    let mut rest = &sprite[open..close];
    let mut kept = 0;
    while let Some(start) = rest.find("<symbol") {
        let Some(len) = rest[start..].find("</symbol>") else { break };
        let symbol = &rest[start..start + len + "</symbol>".len()];
        let tag_end = symbol.find('>').unwrap_or(symbol.len());
        if attr(&symbol[..=tag_end.min(symbol.len() - 1)], "id").is_some_and(|id| used.contains(id.as_str())) {
            out.push_str(symbol);
            kept += 1;
        }
        rest = &rest[start + symbol.len()..];
    }
    if kept == 0 {
        return String::new();
    }
    out.push_str(&sprite[close..]);
    out
}

/// Text without tags (the fallback for a chapter title).
fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    // The chapter number is not part of the title.
    if let Some(i) = rest.find(r#"<span class="k-num">"#)
        && let Some(j) = rest[i..].find("</span>")
    {
        out.push_str(&strip_all(&rest[..i]));
        rest = &rest[i + j + "</span>".len()..];
    }
    out.push_str(&strip_all(rest));
    out.trim().to_owned()
}

fn strip_all(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    unescape(&out)
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Heading;

    const BOOK: &str = concat!(
        r#"<svg class="k-glyphs" aria-hidden="true"><defs><symbol id="g1"><path d="M0"/></symbol><symbol id="g2"><path d="M1"/></symbol></defs></svg>"#,
        r#"<article class="k-doc" data-k-figs="ab" data-doc="book">"#,
        "\n<header class=\"k-title\"><h1>Книга</h1></header>\n",
        r#"<h2 class="k-h k-h1" data-num="1" id="гл-1" data-k-anchor="Первая"><span class="k-num">1</span>Первая</h2>"#,
        "\n<p title=\"a > b\">текст<br>ещё</p>",
        r#"<h3 class="k-h k-h2" id="Итоги" data-k-anchor="Итоги">Итоги</h3>"#,
        r##"<div class="k-frame"><svg><defs><clipPath id="c1"/></defs><use xlink:href="#g1"/></svg></div>"##,
        "<style>h2 { } </h2></style>\n",
        r#"<h2 class="k-h k-h1" data-num="2" id="Вторая" data-k-anchor="Вторая"><span class="k-num">2</span>Вторая &amp; последняя</h2>"#,
        r#"<h3 class="k-h k-h2" id="Итоги-2" data-k-anchor="Итоги">Итоги</h3>"#,
        r##"<div><h2 class="k-h1">не глава: не прямой потомок</h2><use href="#g2"/></div>"##,
        "</article>\n<footer>низ</footer>",
    );

    fn page() -> NotePage {
        let heading = |level, id: &str, text: &str| Heading {
            level,
            id: id.into(),
            anchor: id.into(),
            text: text.into(),
            html: None,
        };
        NotePage {
            id: crate::NoteId::new("Книга").unwrap(),
            kind: crate::NoteKind::Book,
            version: "v".into(),
            rendered: Some(Arc::new(Rendered {
                title: Some("Книга".into()),
                styles: String::new(),
                body: BOOK.into(),
                headings: vec![heading(1, "гл-1", "Первая"), heading(1, "Вторая", "Вторая & последняя")],
                links: vec![],
                tags: vec![],
                sanitizer: None,
            })),
            errors: vec![],
            warnings: vec![],
            book: None,
        }
    }

    fn body(p: &NotePage) -> &str {
        &p.rendered.as_ref().unwrap().body
    }

    #[test]
    fn chapters_and_anchors() {
        let first = chapter_page(&page(), Select::default()).unwrap();
        let view = first.book.as_ref().unwrap();
        assert_eq!(view.chapter, 0);
        assert_eq!(
            view.chapters,
            vec![
                Chapter { num: "1".into(), title: "Первая".into(), id: "гл-1".into() },
                Chapter {
                    num: "2".into(), title: "Вторая & последняя".into(), id: "Вторая".into()
                },
            ]
        );
        let a = |k: &str| view.anchors.get(k).copied();
        assert_eq!(
            (a("гл-1"), a("Первая"), a("Итоги"), a("Итоги-2"), a("Вторая")),
            (Some(0), Some(0), Some(0), Some(1), Some(1))
        );
        assert_eq!(a("c1"), None, "ids inside SVG are not anchors");

        let b = body(&first);
        assert!(b.contains("<header") && b.contains("гл-1") && b.contains("<style>h2 { } </h2></style>"));
        assert!(!b.contains("Итоги-2"));
        assert!(b.contains(r#"<symbol id="g1">"#) && !b.contains(r#"<symbol id="g2">"#));
        assert!(b.ends_with("</article>\n<footer>низ</footer>"));
    }

    #[test]
    fn select_by_number_and_anchor() {
        let p = page();
        let second = chapter_page(&p, Select { chapter: Some(1), anchor: None }).unwrap();
        let b = body(&second);
        assert_eq!(second.book.as_ref().unwrap().chapter, 1);
        assert!(!b.contains("<header") && !b.contains("гл-1") && b.contains("не прямой потомок"));
        assert!(b.contains(r#"<symbol id="g2">"#) && !b.contains(r#"<symbol id="g1">"#));
        assert!(
            b.starts_with(SPRITE_OPEN) && b.contains(r#"<article class="k-doc" data-k-figs="ab" data-doc="book">"#)
        );

        let by_anchor = |a| chapter_page(&p, Select { chapter: None, anchor: Some(a) }).unwrap().book.unwrap().chapter;
        assert_eq!((by_anchor("Итоги-2"), by_anchor("Итоги"), by_anchor("нет такого")), (1, 0, 0));
        let past_end = chapter_page(&p, Select { chapter: Some(9), anchor: None }).unwrap();
        assert_eq!(past_end.book.unwrap().chapter, 1);
    }

    #[test]
    fn not_a_book() {
        let mut p = page();
        let r = Arc::get_mut(p.rendered.as_mut().unwrap()).unwrap();
        r.body = r.body.replace("data-doc=\"book\"", "data-doc=\"note\"");
        assert!(chapter_page(&p, Select::default()).is_none());
        let one = BOOK.split("<h2 class=\"k-h k-h1\" data-num=\"2\"").next().unwrap().to_owned() + "</article>";
        let r = Arc::get_mut(p.rendered.as_mut().unwrap()).unwrap();
        r.body = one;
        assert!(chapter_page(&p, Select::default()).is_none(), "one chapter: shown whole");
    }
}
