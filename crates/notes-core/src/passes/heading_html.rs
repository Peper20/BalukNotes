//! Heading HTML for the contents: formulas (MathML), emphasis, code as in the
//! heading itself, not as plain text (`ℝ𝑛` instead of $RR^n$).
//!
//! On the text of the finished page (after code colors): for each heading of
//! [`Rendered::headings`] it takes the contents of `<hN id="...">...</hN>`
//! without the number (`span.k-num`), links (the contents entry is a link
//! itself) and `id`s (the page already has them). A plain-text heading gets no
//! `html`: `text` is enough. Neither does one with a figure (SVG): a figure has
//! no place in a contents line.

use std::collections::HashMap;

use super::TextPass;
use crate::render::Rendered;

pub const PASS: TextPass = TextPass { name: "contents headings", run };

fn run(page: &mut Rendered) {
    if page.headings.is_empty() {
        return;
    }
    let inner = heading_inners(&page.body);
    for h in &mut page.headings {
        h.html = inner.get(h.id.as_str()).and_then(|html| toc_html(html));
    }
}

/// A heading `id` -> its contents (the HTML between the opening and closing tags).
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

/// The value of the attribute `name="..."` in the text of an opening tag.
fn attr_value<'a>(open: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = open.find(&key)? + key.len();
    let len = open[at..].find('"')?;
    Some(&open[at..at + len])
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// Heading contents -> the HTML of a contents line; `None` if the text is enough.
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
            // The number: `span.k-num` without nested spans, up to its `</span>`.
            match rest.find("</span>") {
                Some(end) => rest = &rest[end + "</span>".len()..],
                None => rest = "",
            }
        } else if tag == "</a>" || tag.starts_with("<a ") || tag == "<a>" {
            // A link in a heading: its text (the contents line is a link itself).
        } else {
            out.push_str(&drop_id(tag));
        }
    }
    out.push_str(rest);
    let out = out.trim();
    out.contains('<').then(|| out.to_owned())
}

/// A tag without the `id` attribute (the page already has it, a repeat would be an error).
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
