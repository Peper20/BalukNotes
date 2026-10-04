//! Sanitizing the page HTML before serializing (`docs/architecture.md`, HTML
//! security): only an allowlist of HTML, MathML and SVG tags and attributes seen in
//! the real output of the library, the passes and Typst stays. Scripts, frames,
//! forms, `on*` attributes, dangerous URLs and CSS (`javascript:`,
//! `expression(`, non-image `data:`), external `<use>` and SVG animations of
//! links are removed; `meta` stays only in the real `<head>` of Typst. The
//! counts of what was removed become a build warning ([`crate::pipeline`]).

use ecow::EcoVec;
use typst_html::{HtmlElement, HtmlNode, attr};

use super::{Context, TreePass};

pub const PASS: TreePass = TreePass { name: "sanitize", visit };

const DROP_TAGS: &[&str] = &[
    "script",
    "iframe",
    "object",
    "embed",
    "frame",
    "frameset",
    "base",
    "form",
    "input",
    "button",
    "textarea",
    "select",
    "option",
    "optgroup",
    "fieldset",
    "legend",
    "label",
    "datalist",
    "output",
    "foreignobject",
];

const URL_ATTRS: &[&str] = &["href", "src", "xlink:href", "action", "formaction"];

const REAL_HEAD_MARK: &str = "_notes-real-head";

const GLOBAL_ATTRS: &[&str] = &["id", "class", "title", "lang", "dir", "role", "tabindex", "hidden"];

const HTML_ATTRS: &[&str] = &[
    "alt",
    "width",
    "height",
    "loading",
    "decoding",
    "srcset",
    "sizes",
    "colspan",
    "rowspan",
    "scope",
    "open",
    "target",
    "rel",
    "download",
    "hreflang",
    "controls",
    "autoplay",
    "loop",
    "muted",
    "playsinline",
    "poster",
    "preload",
    "crossorigin",
    "referrerpolicy",
    "name",
    "value",
    "type",
    "charset",
    "content",
    "aria-hidden",
    "aria-label",
    "aria-live",
    "aria-current",
    "aria-controls",
    "aria-expanded",
    "aria-describedby",
    "aria-labelledby",
];

const MATHML_TAGS: &[&str] = &[
    "annotation",
    "annotation-xml",
    "maction",
    "math",
    "merror",
    "mfrac",
    "mi",
    "mmultiscripts",
    "mn",
    "mo",
    "mover",
    "mpadded",
    "mphantom",
    "mprescripts",
    "mroot",
    "mrow",
    "ms",
    "mspace",
    "msqrt",
    "mstyle",
    "msub",
    "msubsup",
    "msup",
    "mtable",
    "mtd",
    "mtext",
    "mtr",
    "munder",
    "munderover",
    "semantics",
];

const MATHML_ATTRS: &[&str] = &[
    "display",
    "displaystyle",
    "scriptlevel",
    "linethickness",
    "mathvariant",
    "form",
    "fence",
    "separator",
    "lspace",
    "rspace",
    "stretchy",
    "symmetric",
    "maxsize",
    "minsize",
    "largeop",
    "movablelimits",
    "width",
    "height",
    "accent",
    "accentunder",
    "columnspan",
    "rowspan",
];

const SVG_TAGS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "desc",
    "title",
    "symbol",
    "use",
    "path",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "rect",
    "text",
    "tspan",
    "textpath",
    "marker",
    "mask",
    "pattern",
    "clippath",
    "lineargradient",
    "radialgradient",
    "stop",
    "filter",
    "feblend",
    "fecolormatrix",
    "fecomponenttransfer",
    "fecomposite",
    "feconvolvematrix",
    "fediffuselighting",
    "fedisplacementmap",
    "fedistantlight",
    "fedropshadow",
    "feflood",
    "fefunca",
    "fefuncb",
    "fefuncg",
    "fefuncr",
    "fegaussianblur",
    "feimage",
    "femerge",
    "femergenode",
    "femorphology",
    "feoffset",
    "fepointlight",
    "fespecularlighting",
    "fespotlight",
    "fetile",
    "feturbulence",
    "animate",
    "animatetransform",
    "set",
];

const SVG_ATTRS: &[&str] = &[
    "viewbox",
    "width",
    "height",
    "x",
    "y",
    "x1",
    "x2",
    "y1",
    "y2",
    "cx",
    "cy",
    "dx",
    "dy",
    "r",
    "rx",
    "ry",
    "d",
    "points",
    "transform",
    "preserveaspectratio",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
    "clip-path",
    "clip-rule",
    "mask",
    "filter",
    "marker-start",
    "marker-mid",
    "marker-end",
    "stop-color",
    "stop-opacity",
    "offset",
    "gradientunits",
    "gradienttransform",
    "spreadmethod",
    "patternunits",
    "patterncontentunits",
    "text-anchor",
    "dominant-baseline",
    "font-size",
    "font-family",
    "font-style",
    "font-weight",
    "letter-spacing",
    "word-spacing",
    "textlength",
    "lengthadjust",
    "startoffset",
    "pathlength",
    "vector-effect",
    "shape-rendering",
    "attributeName",
    "attributename",
    "from",
    "to",
    "values",
    "dur",
    "repeatcount",
    "keytimes",
    "keysplines",
    "calcmode",
];

#[derive(Debug, Clone, Copy, Default)]
struct DropReason {
    bad_urls: usize,
}

fn visit(ctx: &mut Context<'_>, el: &mut HtmlElement) {
    sanitize_children(ctx, el);
    sanitize_attrs(ctx, el);
}

fn sanitize_children(ctx: &mut Context<'_>, el: &mut HtmlElement) {
    let parent_is_real_head = is_real_head(el);
    let parent_is_real_html_root = is_real_html_root(el);
    let mut kept = EcoVec::with_capacity(el.children.len());
    for child in &el.children {
        let HtmlNode::Element(child_el) = child else {
            kept.push(child.clone());
            continue;
        };
        let mut child = child_el.clone();
        if parent_is_real_html_root && tag_name(&child) == "head" {
            child.attrs.push(crate::render::attr_name(REAL_HEAD_MARK), "1");
        }
        if let Some(reason) = drop_reason(&child, parent_is_real_head) {
            ctx.removed_tags += 1;
            ctx.removed_urls += reason.bad_urls;
        } else {
            kept.push(HtmlNode::Element(child));
        }
    }
    el.children = kept;
}

fn sanitize_attrs(ctx: &mut Context<'_>, el: &mut HtmlElement) {
    let tag = tag_name(el);
    let mut out = EcoVec::new();
    for (attr, value) in &el.attrs.0 {
        let name = attr.resolve().as_str().to_ascii_lowercase();
        if name == REAL_HEAD_MARK {
            continue;
        }
        if !is_allowed_attr(&tag, &name) {
            ctx.removed_attrs += 1;
            continue;
        }
        if URL_ATTRS.contains(&name.as_str()) {
            if !is_allowed_url(value) {
                ctx.removed_attrs += 1;
                ctx.removed_urls += 1;
                continue;
            }
            if tag == "use" && !is_local_svg_ref(value) {
                ctx.removed_attrs += 1;
                ctx.removed_urls += 1;
                continue;
            }
        }
        if name == "style" {
            let (safe, bad_urls) = is_safe_style(value);
            if !safe {
                ctx.removed_attrs += 1;
                ctx.removed_urls += bad_urls;
                continue;
            }
        }
        out.push((*attr, value.clone()));
    }
    el.attrs.0 = out;
}

fn drop_reason(el: &HtmlElement, parent_is_real_head: bool) -> Option<DropReason> {
    let tag = tag_name(el);
    if tag == "meta" && (!parent_is_real_head || !is_typst_head_meta(el)) {
        return Some(DropReason::default());
    }
    if tag == "link" {
        return Some(DropReason::default());
    }
    if !is_allowed_tag(&tag) || DROP_TAGS.contains(&tag.as_str()) {
        return Some(DropReason::default());
    }
    if matches!(tag.as_str(), "animate" | "set" | "animatetransform") && targets_dangerous_attr(el) {
        return Some(DropReason::default());
    }
    if tag == "use" {
        let href = el.attrs.get(attr::href).or_else(|| el.attrs.get(crate::render::attr_name("xlink:href")));
        if href.is_some_and(|href| !is_local_svg_ref(href)) {
            return Some(DropReason { bad_urls: 1 });
        }
    }
    if tag == "style" {
        let css = style_text(el);
        let (safe, bad_urls) = is_safe_style(&css);
        if !safe {
            return Some(DropReason { bad_urls });
        }
    }
    None
}

fn is_real_html_root(el: &HtmlElement) -> bool {
    tag_name(el) == "html" && el.parent.is_none()
}

fn is_real_head(el: &HtmlElement) -> bool {
    el.tag.resolve().as_str().eq_ignore_ascii_case("head")
        && el.attrs.get(crate::render::attr_name(REAL_HEAD_MARK)).is_some()
}

fn is_typst_head_meta(el: &HtmlElement) -> bool {
    let mut charset = None;
    let mut name = None;
    let mut content = None;

    for (attr, value) in &el.attrs.0 {
        let key = attr.resolve().as_str().to_ascii_lowercase();
        match key.as_str() {
            REAL_HEAD_MARK => {}
            "charset" => {
                if charset.replace(value.as_str()).is_some() {
                    return false;
                }
            }
            "name" => {
                if name.replace(value.as_str()).is_some() {
                    return false;
                }
            }
            "content" => {
                if content.replace(value.as_str()).is_some() {
                    return false;
                }
            }
            _ => return false,
        }
    }

    match (charset, name, content) {
        (Some(charset), None, None) => normalize_token(charset) == "utf-8",
        (None, Some(name), Some(content)) => match normalize_token(name).as_str() {
            "viewport" => content == "width=device-width, initial-scale=1",
            "description" | "authors" | "keywords" => true,
            _ => false,
        },
        _ => false,
    }
}

fn tag_name(el: &HtmlElement) -> String {
    el.tag.resolve().as_str().to_ascii_lowercase()
}

fn is_allowed_tag(tag: &str) -> bool {
    MATHML_TAGS.contains(&tag) || SVG_TAGS.contains(&tag) || ALLOWED_HTML_TAGS.contains(&tag)
}

const ALLOWED_HTML_TAGS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "bdi",
    "bdo",
    "blockquote",
    "body",
    "br",
    "canvas",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "main",
    "map",
    "mark",
    "meta",
    "menu",
    "meter",
    "nav",
    "link",
    "noscript",
    "ol",
    "p",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "search",
    "section",
    "slot",
    "small",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
];

fn is_allowed_attr(tag: &str, name: &str) -> bool {
    if name.starts_with("on") {
        return false;
    }
    if GLOBAL_ATTRS.contains(&name)
        || name.starts_with("data-")
        || name.starts_with("aria-")
        || name == "style"
        || URL_ATTRS.contains(&name)
        || name == "xmlns"
        || name.starts_with("xmlns:")
        || name.starts_with("xml:")
    {
        return true;
    }
    if MATHML_TAGS.contains(&tag) {
        return MATHML_ATTRS.contains(&name);
    }
    if SVG_TAGS.contains(&tag) {
        return SVG_ATTRS.contains(&name);
    }
    HTML_ATTRS.contains(&name)
}

fn targets_dangerous_attr(el: &HtmlElement) -> bool {
    let target = el.attrs.get(crate::render::attr_name("attributeName"));
    let Some(target) = target else { return false };
    let normalized = normalize_token(target);
    normalized == "href" || normalized == "xlink:href" || normalized.starts_with("on")
}

fn style_text(el: &HtmlElement) -> String {
    let mut out = String::new();
    for child in &el.children {
        if let HtmlNode::Text(text, _) = child {
            out.push_str(text);
        }
    }
    out
}

fn is_safe_style(style: &str) -> (bool, usize) {
    let decoded = decode_entities(style);
    let compact = normalize_token(&decoded);
    if compact.contains("expression(") {
        return (false, 0);
    }
    let mut bad_urls = 0;
    let mut rest = compact.as_str();
    while let Some(start) = rest.find("url(") {
        let tail = &rest[start + 4..];
        let Some(end) = tail.find(')') else {
            return (false, bad_urls.max(1));
        };
        let url = tail[..end].trim_matches(|c| c == '\'' || c == '"');
        if !is_allowed_url(url) {
            bad_urls += 1;
        }
        rest = &tail[end + 1..];
    }
    (bad_urls == 0, bad_urls)
}

fn is_allowed_url(value: &str) -> bool {
    let normalized = normalize_token(value);
    if normalized.is_empty() || normalized.starts_with('#') {
        return true;
    }
    let Some(scheme) = url_scheme(&normalized) else {
        return true;
    };
    match scheme {
        "http" | "https" | "mailto" => true,
        "data" => {
            let rest = &normalized[scheme.len() + 1..];
            let media = rest.split([',', ';']).next().unwrap_or_default();
            media.starts_with("image/")
        }
        _ => false,
    }
}

fn url_scheme(url: &str) -> Option<&str> {
    for (idx, ch) in url.char_indices() {
        match ch {
            ':' => return Some(&url[..idx]),
            '/' | '?' | '#' => return None,
            _ => {}
        }
    }
    None
}

fn is_local_svg_ref(value: &str) -> bool {
    let normalized = normalize_token(value);
    normalized.starts_with('#') && normalized.len() > 1
}

fn normalize_token(value: &str) -> String {
    decode_entities(value)
        .chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_control())
        .flat_map(char::to_lowercase)
        .collect()
}

fn decode_entities(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch != '&' {
            out.push(ch);
            continue;
        }
        let mut end = None;
        for (j, next) in chars.clone() {
            if next == ';' {
                end = Some(j);
                break;
            }
            if next == '&' || j.saturating_sub(i) > 16 {
                break;
            }
        }
        let Some(end) = end else {
            out.push(ch);
            continue;
        };
        let entity = &value[i + 1..end];
        if let Some(decoded) = decode_entity(entity) {
            out.push(decoded);
            while let Some((j, _)) = chars.peek().copied() {
                if j <= end {
                    chars.next();
                } else {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    if let Some(hex) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
        let code = u32::from_str_radix(hex, 16).ok()?;
        return char::from_u32(code);
    }
    if let Some(dec) = entity.strip_prefix('#') {
        let code = dec.parse::<u32>().ok()?;
        return char::from_u32(code);
    }
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "colon" => Some(':'),
        "tab" | "Tab" => Some('\t'),
        "newline" | "NewLine" => Some('\n'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use typst_html::{HtmlAttr, HtmlElement, HtmlNode, HtmlTag, attr, tag};

    use super::*;
    use crate::passes::run_tree;
    use crate::passes::test_util::{ctx, el, text};

    fn tag(name: &str) -> HtmlTag {
        HtmlTag::intern(name).unwrap()
    }

    fn attr(name: &str) -> HtmlAttr {
        HtmlAttr::intern(name).unwrap()
    }

    #[test]
    fn drops_forbidden_tags_and_event_attrs() {
        let themes = vec![];
        let mut context = ctx(&themes);
        let mut root = el(
            tag::div,
            &[],
            vec![
                HtmlNode::Element(el(tag("script"), &[], vec![text("alert(1)")])),
                HtmlNode::Element(el(tag::img, &[(attr::src, "/x"), (attr("onerror"), "boom")], vec![])),
            ],
        );

        run_tree(&mut root, &mut context, &[PASS]);

        assert!(
            root.children.iter().all(|n| !matches!(n, HtmlNode::Element(e) if tag_name(e) == "script")),
            "script is removed"
        );
        let img = root.children.iter().find_map(|n| match n {
            HtmlNode::Element(e) if e.tag == tag::img => Some(e),
            _ => None,
        });
        assert!(img.is_some(), "img stays");
        assert!(img.unwrap().attrs.get(attr("onerror")).is_none(), "the event attribute is removed");
        assert_eq!(context.removed_tags, 1);
        assert_eq!(context.removed_attrs, 1);
    }

    #[test]
    fn enforces_url_scheme_allowlist() {
        let themes = vec![];
        let mut context = ctx(&themes);
        let mut root = el(
            tag::div,
            &[],
            vec![
                HtmlNode::Element(el(tag::a, &[(attr::href, " JaVa\nScRiPt:alert(1)")], vec![text("x")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "JAVASCRIPT:alert(1)")], vec![text("x")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "jav&#x61;script:alert(1)")], vec![text("x")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "file:///etc/passwd")], vec![text("x")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "ftp://evil")], vec![text("x")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "custom+scheme://evil")], vec![text("x")])),
                HtmlNode::Element(el(tag::img, &[(attr::src, "data:text/html,boom")], vec![])),
                HtmlNode::Element(el(tag::img, &[(attr::src, "data:image/png;base64,AA==")], vec![])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "vbscript:msgbox(1)")], vec![text("x")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "https://example.com")], vec![text("ok")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "http://example.com")], vec![text("ok")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "mailto:test@example.com")], vec![text("ok")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "/notes/A")], vec![text("ok")])),
                HtmlNode::Element(el(tag::a, &[(attr::href, "#local")], vec![text("ok")])),
            ],
        );

        run_tree(&mut root, &mut context, &[PASS]);

        for child in &root.children {
            if let HtmlNode::Element(el) = child
                && el.tag == tag::a
            {
                if text_of(el) == "x" {
                    assert!(el.attrs.get(attr::href).is_none());
                } else {
                    assert!(el.attrs.get(attr::href).is_some());
                }
            }
        }
        let imgs: Vec<&HtmlElement> = root
            .children
            .iter()
            .filter_map(|n| match n {
                HtmlNode::Element(e) if e.tag == tag::img => Some(e),
                _ => None,
            })
            .collect();
        assert!(imgs[0].attrs.get(attr::src).is_none(), "non-image data: is removed");
        assert!(imgs[1].attrs.get(attr::src).is_some(), "image data: stays");
        assert!(context.removed_urls >= 8);
    }

    #[test]
    fn keeps_real_head_meta_and_drops_fake_head_meta_and_link() {
        let themes = vec![];
        let mut context = ctx(&themes);
        let mut root = el(
            tag::html,
            &[],
            vec![
                HtmlNode::Element(el(
                    tag::head,
                    &[],
                    vec![
                        HtmlNode::Element(el(tag::meta, &[(attr("charset"), "utf-8")], vec![])),
                        HtmlNode::Element(el(
                            tag::meta,
                            &[(attr("name"), "viewport"), (attr("content"), "width=device-width, initial-scale=1")],
                            vec![],
                        )),
                    ],
                )),
                HtmlNode::Element(el(
                    tag::body,
                    &[],
                    vec![HtmlNode::Element(el(
                        tag::head,
                        &[],
                        vec![
                            HtmlNode::Element(el(
                                tag::meta,
                                &[(attr("http-equiv"), "refresh"), (attr("content"), "0;url=https://evil")],
                                vec![],
                            )),
                            HtmlNode::Element(el(
                                tag::link,
                                &[(attr("rel"), "stylesheet"), (attr::href, "https://evil/style.css")],
                                vec![],
                            )),
                        ],
                    ))],
                )),
            ],
        );

        run_tree(&mut root, &mut context, &[PASS]);

        let [HtmlNode::Element(head), HtmlNode::Element(body)] = &root.children[..] else {
            panic!("expected html(head, body)");
        };
        let real_head_meta: Vec<&HtmlElement> = head
            .children
            .iter()
            .filter_map(|child| match child {
                HtmlNode::Element(el) if el.tag == tag::meta => Some(el),
                _ => None,
            })
            .collect();
        assert_eq!(real_head_meta.len(), 2, "the real head must stay unchanged");
        assert_eq!(real_head_meta[0].attrs.get(attr("charset")).unwrap(), "utf-8");
        assert_eq!(real_head_meta[1].attrs.get(attr("name")).unwrap(), "viewport");

        let fake_head = body.children.iter().find_map(|child| match child {
            HtmlNode::Element(el) if el.tag == tag::head => Some(el),
            _ => None,
        });
        assert!(fake_head.is_some(), "the fake <head> itself stays");
        let fake_head = fake_head.unwrap();
        assert!(
            fake_head
                .children
                .iter()
                .all(|child| { !matches!(child, HtmlNode::Element(el) if el.tag == tag::meta || el.tag == tag::link) }),
            "meta/link inside a fake head are removed"
        );
    }

    #[test]
    fn drops_svg_foreign_object_external_use_and_dangerous_animation_targets() {
        let themes = vec![];
        let mut context = ctx(&themes);
        let mut root = el(
            tag("svg"),
            &[(attr("xmlns"), "http://www.w3.org/2000/svg")],
            vec![
                HtmlNode::Element(el(tag("foreignObject"), &[], vec![text("x")])),
                HtmlNode::Element(el(tag("use"), &[(attr::href, "https://evil/x.svg#id")], vec![])),
                HtmlNode::Element(el(tag("use"), &[(attr::href, "#inside")], vec![])),
                HtmlNode::Element(el(tag("animate"), &[(attr("attributeName"), "href")], vec![])),
                HtmlNode::Element(el(tag("set"), &[(attr("attributeName"), "onclick")], vec![])),
            ],
        );

        run_tree(&mut root, &mut context, &[PASS]);

        let mut names = Vec::new();
        for child in &root.children {
            if let HtmlNode::Element(e) = child {
                names.push(tag_name(e));
            }
        }
        assert!(names.contains(&"use".to_string()), "a local use stays");
        assert_eq!(names.iter().filter(|n| n.as_str() == "use").count(), 1, "an external use is removed");
        assert!(!names.iter().any(|n| n == "foreignobject"));
        assert!(!names.iter().any(|n| n == "animate" || n == "set"));
        assert!(context.removed_tags >= 4);
        assert!(context.removed_urls >= 1);
    }

    #[test]
    fn removes_unsafe_style_attr_and_style_tag() {
        let themes = vec![];
        let mut context = ctx(&themes);
        let mut root = el(
            tag::div,
            &[],
            vec![
                HtmlNode::Element(el(
                    tag::span,
                    &[(attr::style, "background:url(javascript:alert(1))")],
                    vec![text("a")],
                )),
                HtmlNode::Element(el(tag("style"), &[], vec![text("p{width:expression(alert(1))}")])),
                HtmlNode::Element(el(tag::span, &[(attr::style, "color:var(--k-fg)")], vec![text("ok")])),
            ],
        );

        run_tree(&mut root, &mut context, &[PASS]);

        let spans: Vec<&HtmlElement> = root
            .children
            .iter()
            .filter_map(|n| match n {
                HtmlNode::Element(e) if e.tag == tag::span => Some(e),
                _ => None,
            })
            .collect();
        assert!(spans[0].attrs.get(attr::style).is_none(), "a dangerous style is removed");
        assert_eq!(spans[1].attrs.get(attr::style).unwrap(), "color:var(--k-fg)");
        assert!(
            root.children.iter().all(|n| !matches!(n, HtmlNode::Element(e) if tag_name(e) == "style")),
            "a dangerous <style> is removed"
        );
    }

    fn text_of(el: &HtmlElement) -> String {
        let mut out = String::new();
        for child in &el.children {
            if let HtmlNode::Text(text, _) = child {
                out.push_str(text);
            }
        }
        out
    }
}
