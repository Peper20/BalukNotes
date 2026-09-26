//! Книга по главам: страница одной главы из страницы всей книги.
//!
//! Книга собирается целиком (счётчики, ссылки между главами), но показывать
//! её лучше по главе: вёрстка всей книги — сотни миллисекунд на ПК и секунды
//! на телефоне. Главы — прямые потомки `<article class="k-doc"
//! data-doc="book">` от одного `h2.k-h1` до следующего; всё до первой главы
//! (титул) идёт с первой.
//!
//! Работает с готовым HTML (после [`crate::figures`]): режет текст по
//! границам прямых потомков статьи. Разметку пишет `typst-html` — теги
//! закрыты, `<` в тексте экранирован, — поэтому хватает разбора по тегам.
//! Из общего набора глифов (`svg.k-glyphs`) глава получает только свои.
//!
//! Нарезка стоит один проход по тексту («Матан», ~1,5 МБ, — миллисекунды),
//! поэтому не кэшируется.

use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use serde::Serialize;

use crate::notes::NotePage;
use crate::render::Rendered;

/// Глава в списке глав.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Chapter {
    /// Номер главы (`data-num`) или пусто.
    pub num: String,
    /// Название без номера.
    pub title: String,
    /// `id` заголовка главы.
    pub id: String,
}

/// Какая глава показана и где остальные.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BookView {
    /// Номер показанной главы (с нуля).
    pub chapter: u32,
    pub chapters: Vec<Chapter>,
    /// Якорь (`id` или `data-k-anchor`) → номер главы. `id` уникальны; из
    /// одинаковых `data-k-anchor` («Итоги» в каждой главе) — первый.
    pub anchors: BTreeMap<String, u32>,
}

/// Какую главу показать.
#[derive(Debug, Clone, Copy, Default)]
pub struct Select<'a> {
    pub chapter: Option<usize>,
    /// Якорь — глава, где он есть (если `chapter` не задан).
    pub anchor: Option<&'a str>,
}

/// Страница одной главы книги. Не книга или в ней меньше двух глав — `None`
/// (показывать целиком). Номер главы за концом — последняя глава.
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

/// Границы частей книги в тексте страницы.
#[derive(Debug, PartialEq, Eq)]
struct Layout {
    /// Скрытый `<svg class="k-glyphs">` в начале страницы (или пусто).
    sprite: Range<usize>,
    /// Содержимое статьи до первой главы (титул).
    intro: Range<usize>,
    chapters: Vec<ChapterPart>,
}

#[derive(Debug, PartialEq, Eq)]
struct ChapterPart {
    range: Range<usize>,
    /// Открывающий тег заголовка главы и весь заголовок.
    heading_tag: Range<usize>,
    heading: Range<usize>,
}

const SPRITE_OPEN: &str = r#"<svg class="k-glyphs""#;

/// Элементы без закрывающего тега.
const VOID: &[&str] =
    &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];

/// Элементы, внутри которых текст — не разметка.
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
                        // Вместе с закрывающим тегом: глубина не меняется.
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

    /// Список глав и карта якорей.
    fn view(&self, rendered: &Rendered) -> BookView {
        // Название — из заголовков страницы (там оно уже текстом), номер —
        // из `data-num` заголовка.
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

    /// Текст страницы с одной главой `k` (у первой — ещё и титул).
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

/// Конец открывающего тега статьи-книги (начало её содержимого).
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
    /// Комментарий, `<!doctype>`.
    Other,
}

#[derive(Debug)]
struct Tag<'a> {
    start: usize,
    end: usize,
    kind: TagKind<'a>,
}

/// Следующий тег, начиная с `from`: `>` внутри кавычек тег не закрывает.
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

/// Атрибуты открывающего тега (значения — без экранирования).
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

/// `id` и `data-k-anchor` элементов (не внутри SVG: там id служебные).
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

/// Набор глифов только с теми, на которые ссылается `html`.
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

/// Текст без тегов (запасной путь для названия главы).
fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    // Номер главы — не часть названия.
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
        let heading =
            |level, id: &str, text: &str| Heading { level, id: id.into(), anchor: id.into(), text: text.into() };
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
        assert_eq!(a("c1"), None, "id внутри SVG — не якоря");

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
        assert!(chapter_page(&p, Select::default()).is_none(), "одна глава — целиком");
    }
}
