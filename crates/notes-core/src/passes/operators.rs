//! The space after an operator with limits or scripts in formulas
//! (`sum_(i=1)^n f`). MathML Core treats `<msubsup><mo>∑</mo>...</msubsup>` as
//! an embellished operator: Chrome draws the operator's `rspace` after the
//! whole construct, like the PDF. WebKit leaves it inside the `<mo>`, between
//! the operator and its limits, so the formula comes out solid (`i=1f`).
//! We put the space where Chrome puts it explicitly: the base `<mo>` gets
//! `rspace="0em"` and an `<mspace>` of the same width follows the construct.
//! The look in Chrome does not change (checked: a shift under 0.1 px).
//!
//! The space is the explicit `rspace` of the base (Typst writes it for `lim`,
//! `max`, `sin`) or, for the large operators of [`LARGE`], the dictionary
//! default (thin math space) that Typst does not write out. A standalone large
//! operator (`sum f`) gets the same explicit `rspace` too: WebKit's dictionary
//! gives it a smaller one. Anything the pass does not understand stays as is.
//! Architecture §4, `docs/research/E7.md`.

use typst_html::tag::mathml;
use typst_html::{HtmlAttr, HtmlElement, HtmlNode, HtmlTag};

use super::{Context, TreePass};
use crate::render::text_of;

pub const PASS: TreePass = TreePass { name: "operators", visit };

const RSPACE: HtmlAttr = HtmlAttr::constant("rspace");
const WIDTH: HtmlAttr = HtmlAttr::constant("width");

/// A thin math space (3/18 em), the dictionary `rspace` of large operators; the format is Typst's own.
const THIN: &str = "0.16666666666666666em";

/// Large operators: sums, products, integrals, big unions and intersections.
const LARGE: &[&str] = &["∑", "∏", "∐", "∫", "∬", "∭", "∮", "∯", "∰", "⋀", "⋁", "⋂", "⋃"];

/// Elements whose first child is the base (the operator) and the rest are scripts or limits.
const SCRIPTS: [HtmlTag; 6] =
    [mathml::msub, mathml::msup, mathml::msubsup, mathml::munder, mathml::mover, mathml::munderover];

/// Parents where one more child is fine: an `mrow` or an element with an inferred `mrow`.
/// In `mfrac`, `msup` and the like the number of children is fixed: an extra one breaks the formula.
const ROWS: [HtmlTag; 4] = [mathml::math, mathml::mrow, mathml::mtd, mathml::msqrt];

fn visit(_: &mut Context<'_>, parent: &mut HtmlElement) {
    let is_row = ROWS.contains(&parent.tag);
    // Back to front: an inserted `mspace` does not shift the indices still to visit.
    for i in (0..parent.children.len()).rev() {
        let HtmlNode::Element(el) = &mut parent.children.make_mut()[i] else { continue };
        if el.tag == mathml::mo {
            if is_large(el) && el.attrs.get(RSPACE).is_none() {
                el.attrs.push(RSPACE, THIN);
            }
        } else if is_row
            && SCRIPTS.contains(&el.tag)
            && let Some(space) = take_base_rspace(el)
        {
            let mut mspace = HtmlElement::new(mathml::mspace);
            mspace.attrs.push(WIDTH, space);
            parent.children.insert(i + 1, HtmlNode::Element(mspace));
        }
    }
}

fn is_large(mo: &HtmlElement) -> bool {
    LARGE.contains(&text_of(mo, None).as_str())
}

/// Takes the space after the base operator of a construct (leaves `rspace="0em"` there).
/// `None`: the base is not an operator, there is no space, or it is not in `em`.
fn take_base_rspace(el: &mut HtmlElement) -> Option<String> {
    let Some(HtmlNode::Element(base)) = el.children.make_mut().iter_mut().find(|c| matches!(c, HtmlNode::Element(_)))
    else {
        return None;
    };
    if base.tag != mathml::mo {
        return None;
    }
    let space = match base.attrs.get(RSPACE) {
        Some(value) => value.to_string(),
        None if is_large(base) => THIN.to_string(),
        None => return None,
    };
    let em: f64 = space.strip_suffix("em")?.parse().ok()?;
    if em <= 0.0 {
        return None;
    }
    match base.attrs.get_mut(RSPACE) {
        Some(value) => *value = "0em".into(),
        None => base.attrs.push(RSPACE, "0em"),
    }
    Some(space)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::test_util::{ctx, el, node, text};

    fn mo(s: &str, attrs: &[(HtmlAttr, &str)]) -> HtmlNode {
        node(el(mathml::mo, attrs, vec![text(s)]))
    }

    fn mi(s: &str) -> HtmlNode {
        node(el(mathml::mi, &[], vec![text(s)]))
    }

    /// `<math>` of the given children after the pass, as a compact string:
    /// the name, the `rspace` of an `mo`, the width of an `mspace`.
    fn run(children: Vec<HtmlNode>) -> String {
        let themes = vec![];
        let mut c = ctx(&themes);
        let mut math = el(mathml::math, &[], children);
        crate::render::walk_mut(&mut math, &mut |e| visit(&mut c, e));
        describe(&math)
    }

    fn describe(e: &HtmlElement) -> String {
        let name = format!("{:?}", e.tag).trim_matches(|c| c == '<' || c == '>').to_string();
        let attrs: Vec<String> =
            [RSPACE, WIDTH].iter().filter_map(|a| e.attrs.get(*a).map(|v| format!("{v}"))).collect();
        let kids: Vec<String> = e
            .children
            .iter()
            .filter_map(|c| match c {
                HtmlNode::Element(k) => Some(describe(k)),
                _ => None,
            })
            .collect();
        match (attrs.is_empty(), kids.is_empty()) {
            (true, true) => name,
            (false, true) => format!("{name}[{}]", attrs.join(",")),
            (true, false) => format!("{name}({})", kids.join(" ")),
            (false, false) => format!("{name}[{}]({})", attrs.join(","), kids.join(" ")),
        }
    }

    #[test]
    fn space_moves_after_the_construct() {
        // A sum with limits: the dictionary space moves out of the operator.
        let sum = node(el(mathml::munderover, &[], vec![mo("∑", &[]), mi("a"), mi("b")]));
        assert_eq!(run(vec![sum, mi("f")]), "math(munderover(mo[0em] mi mi) mspace[0.16666666666666666em] mi)");
        // An integral with scripts and an explicit space (`lim`) the same way.
        let int = node(el(mathml::msubsup, &[], vec![mo("∫", &[]), mi("a"), mi("b")]));
        let lim = node(el(mathml::msub, &[], vec![mo("lim", &[(RSPACE, "0.2em")]), mi("x")]));
        assert_eq!(
            run(vec![int, lim, mi("f")]),
            "math(msubsup(mo[0em] mi mi) mspace[0.16666666666666666em] msub(mo[0em] mi) mspace[0.2em] mi)"
        );
        // Without limits an operator stays as is, a standalone large one gets the explicit space.
        let sin = mo("sin", &[(RSPACE, "0.16666666666666666em")]);
        assert_eq!(run(vec![sin, mi("x")]), "math(mo[0.16666666666666666em] mi)");
        assert_eq!(run(vec![mo("∑", &[]), mi("f")]), "math(mo[0.16666666666666666em] mi)");
        // Already spaced (Typst wrote `rspace="0em"`), a base that is not an operator, a space in other units.
        let spaced = node(el(mathml::msubsup, &[], vec![mo("∑", &[(RSPACE, "0em")]), mi("a"), mi("b")]));
        assert_eq!(run(vec![spaced, mi("f")]), "math(msubsup(mo[0em] mi mi) mi)");
        let plain = node(el(mathml::msub, &[], vec![mi("x"), mi("i")]));
        assert_eq!(run(vec![plain]), "math(msub(mi mi))");
        let px = node(el(mathml::msub, &[], vec![mo("lim", &[(RSPACE, "3px")]), mi("x")]));
        assert_eq!(run(vec![px]), "math(msub(mo[3px] mi))");
        // A construct that is a child of `mfrac` (a fixed number of children): no extra element.
        let alone = node(el(mathml::msub, &[], vec![mo("∑", &[]), mi("i")]));
        let frac = node(el(mathml::mfrac, &[], vec![alone, mi("n")]));
        assert_eq!(run(vec![frac]), "math(mfrac(msub(mo[0.16666666666666666em] mi) mi))");
        // A construct in a nested row: the space goes into the same row.
        let nested =
            node(el(mathml::mrow, &[], vec![node(el(mathml::msub, &[], vec![mo("∑", &[]), mi("i")])), mi("x")]));
        assert_eq!(run(vec![nested]), "math(mrow(msub(mo[0em] mi) mspace[0.16666666666666666em] mi))");
    }
}
