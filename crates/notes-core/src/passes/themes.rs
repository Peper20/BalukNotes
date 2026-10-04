//! Joining themes. The text is the same in every theme; only figures differ
//! (SVG with baked-in colors). Every `div.k-frame` of the base document gets
//! one figure variant per theme, `div.k-frame-v[data-theme]`, and CSS shows the
//! variant of the current theme. The figure order is the same in every theme:
//! [`crate::render::render`] checks it before the passes.

use std::sync::LazyLock;

use ecow::EcoVec;
use typst_html::{HtmlAttr, HtmlElement, HtmlNode, attr, tag};

use super::{Context, TreePass};
use crate::render::{attr_name, has_class};

pub const PASS: TreePass = TreePass { name: "themes", visit };

static DATA_THEME: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-theme"));

/// div.k-frame > svg  ->  div.k-frame > div.k-frame-v[data-theme] per theme.
fn visit(ctx: &mut Context<'_>, el: &mut HtmlElement) {
    if !has_class(el, "k-frame") {
        return;
    }
    let Some(base) = el.children.iter().find_map(|c| match c {
        HtmlNode::Frame(f) => Some(f.clone()),
        _ => None,
    }) else {
        return;
    };
    let i = ctx.frame;
    ctx.frame += 1;
    let mut children = EcoVec::new();
    for (t, theme) in ctx.themes.iter().enumerate() {
        let frame = if t == 0 {
            base.clone()
        } else {
            // Only the base variant keeps ids and anchors inside the SVG,
            // otherwise the page would have repeated ids.
            let mut f = ctx.frames[t - 1][i].clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::test_util::{ctx, el, text};

    #[test]
    fn frame_without_svg_is_left_alone() {
        let themes = vec!["classic".to_owned(), "night".to_owned()];
        let mut c = ctx(&themes);
        let mut div = el(tag::div, &[(attr::class, "k-frame")], vec![text("нет рисунка")]);
        visit(&mut c, &mut div);
        assert_eq!(c.frame, 0);
        assert!(matches!(&div.children[0], HtmlNode::Text(t, _) if t == "нет рисунка"));
    }
}
