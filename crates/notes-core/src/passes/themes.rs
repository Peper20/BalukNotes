//! Склейка тем. Текст одинаков во всех темах, различаются только рисунки
//! (SVG с вшитыми цветами). Каждый `div.k-frame` базового документа
//! получает по варианту рисунка на тему: `div.k-frame-v[data-theme]`, CSS
//! показывает вариант текущей темы. Порядок рисунков во всех темах
//! одинаковый — это проверяет [`crate::render::render`] до проходов.

use std::sync::LazyLock;

use ecow::EcoVec;
use typst_html::{HtmlAttr, HtmlElement, HtmlNode, attr, tag};

use super::{Context, TreePass};
use crate::render::{attr_name, has_class};

pub const PASS: TreePass = TreePass { name: "темы", visit };

static DATA_THEME: LazyLock<HtmlAttr> = LazyLock::new(|| attr_name("data-theme"));

/// div.k-frame > svg  →  div.k-frame > div.k-frame-v[data-theme] × темы.
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
            // id и якоря внутри SVG есть только у базового варианта,
            // иначе на странице окажутся повторяющиеся id.
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
