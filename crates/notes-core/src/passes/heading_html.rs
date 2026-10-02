//! HTML заголовков для оглавления: формулы (MathML), выделение, код — как
//! в самом заголовке, а не плоским текстом (`ℝ𝑛` вместо $RR^n$).
//!
//! По тексту готовой страницы (после цветов кода): у каждого заголовка из
//! [`Rendered::headings`] берётся содержимое `<hN id="…">…</hN>` без номера
//! (`span.k-num`), ссылок (оглавление само — ссылка) и `id` (они уже есть в
//! странице). Заголовок из одного текста — без `html`: хватает `text`. С
//! рисунком (SVG) — тоже без `html`: рисунку в строке оглавления не место.

use std::collections::HashMap;

use super::TextPass;
use crate::render::Rendered;

pub const PASS: TextPass = TextPass { name: "заголовки для оглавления", run };

fn run(page: &mut Rendered) {
    if page.headings.is_empty() {
        return;
    }
    let inner = heading_inners(&page.body);
    for h in &mut page.headings {
        h.html = inner.get(h.id.as_str()).and_then(|html| toc_html(html));
    }
}

/// `id` заголовка → его содержимое (HTML между открывающим и закрывающим тегом).
fn heading_inners(body: &str) -> HashMap<String, &str> {
    let mut out = HashMap::new();
    let mut rest = body;
    let mut offset = 0;
    while let Some(at) = rest.find("<h") {
        let start = offset + at;
        let after = &body[start + 2..];
        let level = after.as_bytes().first().copied().filter(|b| (b'1'..=b'6').contains(b));
        let (Some(level), Some(end)) = (level, after.find('>')) else {
            offset = start + 2;
            rest = &body[offset..];
            continue;
        };
        let open = &after[1..end];
        let content_start = start + 2 + end + 1;
        let close = format!("</h{}>", char::from(level));
        let Some(len) = body[content_start..].find(&close) else { break };
        if let Some(id) = attr_value(open, "id") {
            out.insert(unescape(id), &body[content_start..content_start + len]);
        }
        offset = content_start + len + close.len();
        rest = &body[offset..];
    }
    out
}

/// Значение атрибута `name="…"` в тексте открывающего тега.
fn attr_value<'a>(open: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = open.find(&key)? + key.len();
    let len = open[at..].find('"')?;
    Some(&open[at..at + len])
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// Содержимое заголовка → HTML строки оглавления; `None` — хватает текста.
fn toc_html(inner: &str) -> Option<String> {
    if inner.contains("<svg") {
        return None;
    }
    let mut out = String::with_capacity(inner.len());
    let mut rest = inner;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let Some(len) = rest[at..].find('>') else {
            out.push_str(&rest[at..]);
            rest = "";
            break;
        };
        let tag = &rest[at..=at + len];
        rest = &rest[at + len + 1..];
        if tag.starts_with("<span") && attr_value(tag, "class").is_some_and(|c| c.split(' ').any(|c| c == "k-num")) {
            // Номер: `span.k-num` без вложенных span — до его `</span>`.
            match rest.find("</span>") {
                Some(end) => rest = &rest[end + "</span>".len()..],
                None => rest = "",
            }
        } else if tag == "</a>" || tag.starts_with("<a ") || tag == "<a>" {
            // Ссылка в заголовке — её текст (строка оглавления сама ссылка).
        } else {
            out.push_str(&drop_id(tag));
        }
    }
    out.push_str(rest);
    let out = out.trim();
    out.contains('<').then(|| out.to_owned())
}

/// Тег без атрибута `id` (в странице он уже есть — повтор был бы ошибкой).
fn drop_id(tag: &str) -> String {
    let Some(at) = tag.find(" id=\"") else { return tag.to_owned() };
    let Some(len) = tag[at + 5..].find('"') else { return tag.to_owned() };
    format!("{}{}", &tag[..at], &tag[at + 5 + len + 1..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Heading;

    fn heading(id: &str, text: &str) -> Heading {
        Heading { level: 2, id: id.into(), anchor: id.into(), text: text.into(), html: None }
    }

    #[test]
    fn math_and_markup_kept_number_and_links_dropped() {
        let mut page = Rendered {
            title: None,
            styles: String::new(),
            body: concat!(
                r#"<h2 class="k-h k-h2" id="Скобки" data-k-anchor="Скобки"><span class="k-num">1</span>Скобки</h2>"#,
                r#"<hr><p>текст</p>"#,
                r#"<h2 class="k-h k-h2" id="R" data-k-anchor="R"><span class="k-num">2</span>Пространство <math><msup><mi>ℝ</mi><mi id="x">𝑛</mi></msup></math> и <a href="/n/A">ссылка</a></h2>"#,
                r#"<h3 class="k-h k-h3" id="a&amp;b"><strong>Жирно</strong> &amp; код</h3>"#,
                r#"<h2 id="fig">С рисунком <div class="k-frame"><svg></svg></div></h2>"#,
            )
            .into(),
            headings: vec![heading("Скобки", "Скобки"), heading("R", "Пространство ℝ𝑛"), heading("a&b", "Жирно & код"), heading("fig", "С рисунком")],
            links: vec![],
            tags: vec![],
            sanitizer: None,
        };
        run(&mut page);
        let html: Vec<_> = page.headings.iter().map(|h| h.html.as_deref()).collect();
        assert_eq!(
            html,
            [
                None,
                Some("Пространство <math><msup><mi>ℝ</mi><mi>𝑛</mi></msup></math> и ссылка"),
                Some("<strong>Жирно</strong> &amp; код"),
                None,
            ]
        );
    }

    #[test]
    fn chapter_number_with_word_dropped() {
        assert_eq!(
            toc_html(r#"<span class="k-num" data-word="Глава">1</span>Ряды <em>Тейлора</em>"#).as_deref(),
            Some("Ряды <em>Тейлора</em>")
        );
        assert_eq!(toc_html(r#"<span class="k-num">1</span>Просто"#), None);
    }
}
