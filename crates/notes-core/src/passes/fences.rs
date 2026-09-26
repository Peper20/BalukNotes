//! Скобки в формулах. Typst помечает парные скобки растягиваемыми, а
//! Chrome рисует растягиваемую скобку с широкими полями: `f ( x )`. Если
//! внутри нет высокого (дробей, корней, пределов, матриц — [`TALL`]),
//! растягивать нечего — ставим `stretchy="false"`, и скобки плотные, как в
//! PDF.

use typst_html::tag::mathml;
use typst_html::{HtmlAttr, HtmlElement, HtmlNode, HtmlTag};

use super::{Context, TreePass};
use crate::render::{text_of, walk};

pub const PASS: TreePass = TreePass { name: "скобки", visit };

const STRETCHY: HtmlAttr = HtmlAttr::constant("stretchy");

/// Скобки, которые Typst растягивает по содержимому.
const FENCES: &[&str] = &["(", ")", "[", "]", "{", "}", "|", "‖", "⟨", "⟩", "⌊", "⌋", "⌈", "⌉"];

/// Элементы MathML, ради которых скобку стоит растягивать.
const TALL: [HtmlTag; 7] =
    [mathml::mfrac, mathml::mtable, mathml::msqrt, mathml::mroot, mathml::munderover, mathml::munder, mathml::mover];

/// `mrow` вида `( … )` без высокого внутри → скобки не растягиваются.
fn visit(_: &mut Context<'_>, row: &mut HtmlElement) {
    if row.tag != mathml::mrow {
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::test_util::{ctx, el, node, text};

    fn row(inner: HtmlElement) -> HtmlElement {
        let mo = |s: &str| node(el(mathml::mo, &[], vec![text(s)]));
        el(mathml::mrow, &[], vec![mo("("), node(inner), mo(")")])
    }

    fn stretchy(row: &HtmlElement) -> Vec<Option<String>> {
        row.children
            .iter()
            .filter_map(|c| match c {
                HtmlNode::Element(e) if e.tag == mathml::mo => Some(e.attrs.get(STRETCHY).map(ToString::to_string)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn plain_fences_do_not_stretch() {
        let themes = vec![];
        let mut c = ctx(&themes);
        let mut plain = row(el(mathml::mi, &[], vec![text("x")]));
        visit(&mut c, &mut plain);
        assert_eq!(stretchy(&plain), [Some("false".into()), Some("false".into())]);
        let frac = el(mathml::mfrac, &[], vec![node(el(mathml::mn, &[], vec![text("1")]))]);
        let mut tall = row(el(mathml::mrow, &[], vec![node(frac)]));
        visit(&mut c, &mut tall);
        assert_eq!(stretchy(&tall), [None, None], "дробь внутри — скобки растягиваются");
    }
}
