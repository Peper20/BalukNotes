//! Heading anchors. A heading without a label gets an `id` from its text
//! (`Port change` -> `Port-change`, [`slug`]), and every heading gets
//! `data-k-anchor` with the same slug: so a link finds the section both by
//! text and by label. Headings collect in [`Context::headings`] (the contents).

use typst_html::{HtmlElement, attr, tag};

use super::{Context, TreePass};
use crate::render::{DATA_ANCHOR, Heading, has_class, slug, text_of, unique};

pub const PASS: TreePass = TreePass { name: "anchors", visit };

fn visit(ctx: &mut Context<'_>, el: &mut HtmlElement) {
    // k-h is a baluk heading; no classes means a plain `=` of pure Typst.
    if !(is_heading(el) && (has_class(el, "k-h") || el.attrs.get(attr::class).is_none())) {
        return;
    }
    let text = text_of(el, Some("k-num")).trim().to_owned();
    let anchor = slug(&text);
    let id = if let Some(id) = el.attrs.get(attr::id) {
        id.to_string()
    } else {
        let id = unique(&anchor, &mut ctx.ids);
        el.attrs.push(attr::id, id.as_str());
        id
    };
    el.attrs.push(*DATA_ANCHOR, anchor.as_str());
    // The class k-hN is the design level (chapter, section, ...), not the tag.
    let level = class_level(el).or_else(|| tag_level(el)).unwrap_or(2);
    ctx.headings.push(Heading { level, id, anchor, text, html: None });
}

fn is_heading(el: &HtmlElement) -> bool {
    [tag::h2, tag::h3, tag::h4, tag::h5, tag::h6].contains(&el.tag)
}

/// `<h3>` -> 3: in pure Typst `=` is `<h2>`, a section (like `k-h2` of a note).
fn tag_level(el: &HtmlElement) -> Option<u8> {
    [tag::h2, tag::h3, tag::h4, tag::h5, tag::h6]
        .iter()
        .position(|t| *t == el.tag)
        .and_then(|i| u8::try_from(i + 2).ok())
}

/// `k-h2` -> 2.
fn class_level(el: &HtmlElement) -> Option<u8> {
    el.attrs.get(attr::class)?.split_whitespace().find_map(|c| c.strip_prefix("k-h").and_then(|n| n.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::test_util::{ctx, el, node, text};

    #[test]
    fn headings_get_ids_and_anchors() {
        let themes = vec![];
        let mut c = ctx(&themes);
        c.ids.insert("Итоги".into());
        let num = el(tag::span, &[(attr::class, "k-num")], vec![text("1.2")]);
        let mut h = el(tag::h3, &[(attr::class, "k-h k-h2")], vec![node(num), text(" Итоги")]);
        visit(&mut c, &mut h);
        assert_eq!(h.attrs.get(attr::id).unwrap(), "Итоги-2", "the id is unique");
        assert_eq!(h.attrs.get(*DATA_ANCHOR).unwrap(), "Итоги");
        let mut labelled = el(tag::h2, &[(attr::id, "особый")], vec![text("Раздел")]);
        visit(&mut c, &mut labelled);
        assert_eq!(labelled.attrs.get(attr::id).unwrap(), "особый", "the label stays the id");
        let mut other = el(tag::h2, &[(attr::class, "k-box-title")], vec![text("Не заголовок")]);
        visit(&mut c, &mut other);
        let got: Vec<_> = c.headings.iter().map(|h| (h.level, h.id.as_str(), h.text.as_str())).collect();
        assert_eq!(got, [(2, "Итоги-2", "Итоги"), (2, "особый", "Раздел")]);
    }
}
