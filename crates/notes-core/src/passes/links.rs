//! Ссылки между заметками: `a.k-link` из `#see` получают адрес от
//! [`LinkResolver`](crate::render::LinkResolver); ссылки на несуществующие
//! заметки помечаются `k-link-broken`. Ссылки копятся в
//! [`Context::out_links`] без повторов.

use typst_html::{HtmlElement, attr, tag};

use super::{Context, TreePass};
use crate::render::{DATA_ANCHOR, DATA_TARGET, LinkRef, has_class};

pub const PASS: TreePass = TreePass { name: "ссылки", visit };

fn visit(ctx: &mut Context<'_>, el: &mut HtmlElement) {
    if !(el.tag == tag::a && has_class(el, "k-link")) {
        return;
    }
    let Some(target) = el.attrs.get(*DATA_TARGET).map(ToString::to_string) else { return };
    let anchor = el.attrs.get(*DATA_ANCHOR).map(ToString::to_string);
    if let Some(href) = ctx.links.href(&target, anchor.as_deref()) {
        el.attrs.push(attr::href, href);
    } else {
        if let Some(class) = el.attrs.get_mut(attr::class) {
            class.push_str(" k-link-broken");
        }
        el.attrs.push(attr::title, format!("нет заметки «{target}»"));
    }
    let link = LinkRef { target, anchor };
    if !ctx.out_links.contains(&link) {
        ctx.out_links.push(link);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::test_util::{ctx, el, text};

    #[test]
    fn resolves_and_marks_broken() {
        let themes = vec![];
        let mut c = ctx(&themes);
        let link = |target: &str| {
            el(tag::a, &[(attr::class, "k-link"), (*DATA_TARGET, target), (*DATA_ANCHOR, "x")], vec![text("т")])
        };
        let mut ok = link("A");
        visit(&mut c, &mut ok);
        assert_eq!(ok.attrs.get(attr::href).unwrap(), "/n/A#x");
        let mut again = link("A");
        visit(&mut c, &mut again);
        let mut broken = link("Нет");
        visit(&mut c, &mut broken);
        assert_eq!(broken.attrs.get(attr::class).unwrap(), "k-link k-link-broken");
        assert_eq!(broken.attrs.get(attr::title).unwrap(), "нет заметки «Нет»");
        let targets: Vec<_> = c.out_links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, ["A", "Нет"], "без повторов");
    }
}
