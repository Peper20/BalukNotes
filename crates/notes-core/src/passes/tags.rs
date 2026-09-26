//! Теги заметки: элементы `ul.k-tags` шаблона → [`Context::tags`].

use typst_html::{HtmlElement, HtmlNode, tag};

use super::{Context, TreePass};
use crate::render::{has_class, text_of};

pub const PASS: TreePass = TreePass { name: "теги", visit };

fn visit(ctx: &mut Context<'_>, el: &mut HtmlElement) {
    if !(el.tag == tag::ul && has_class(el, "k-tags")) {
        return;
    }
    ctx.tags.extend(el.children.iter().filter_map(|c| match c {
        HtmlNode::Element(li) => Some(text_of(li, None)),
        _ => None,
    }));
}

#[cfg(test)]
mod tests {
    use typst_html::attr;

    use super::*;
    use crate::passes::test_util::{ctx, el, node, text};

    #[test]
    fn collects_tags() {
        let themes = vec![];
        let mut c = ctx(&themes);
        let li = |t: &str| node(el(tag::li, &[], vec![text(t)]));
        let mut ul = el(tag::ul, &[(attr::class, "k-tags")], vec![li("сеть"), text(" "), li("ssh ключи")]);
        visit(&mut c, &mut ul);
        let mut other = el(tag::ul, &[], vec![li("не тег")]);
        visit(&mut c, &mut other);
        assert_eq!(c.tags, ["сеть", "ssh ключи"]);
    }
}
