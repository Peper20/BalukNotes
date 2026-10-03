use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use notes_core::LibrarySource;
use notes_core::fonts::Fonts;
use notes_core::passes::{self, Context, TreePass};
use notes_core::pipeline::encode;
use notes_core::render::{LinkResolver, Rendered};
use notes_core::themes::ThemeSet;
use notes_core::vault::{NoteId, Vault};
use notes_core::version::Versions;
use notes_core::world::{Compiler, Priority};
use typst::model::Document as _;
use typst_html::{HtmlDocument, HtmlElement, HtmlFrame, HtmlNode, HtmlOptions, attr};

use crate::common::{compile, repo};

#[test]
fn vault_pages_are_identical_with_and_without_sanitizer() {
    let vault = Vault::open(repo().join("tests/vault")).unwrap();
    let versions = Versions::new(vault.storage().clone());
    let fonts = Arc::new(Fonts::load(&[]));
    let compiler = Compiler::new(versions, LibrarySource::Dir(repo().join("baluk")), fonts);
    let themes = ThemeSet::load(&compiler).unwrap();

    for entry in vault.entries().unwrap() {
        let build = compiler.compile_html(&entry.main, &themes.names(), Priority::User);
        let Ok(docs) = build.docs else { continue };
        let clean = render_with_tree_passes(docs.clone(), &vault, passes::TREE).unwrap();
        let no_sanitize = render_with_tree_passes(docs, &vault, &passes::TREE[1..]).unwrap();

        assert_eq!(clean.body, no_sanitize.body, "санитизация изменила body для {}", entry.id);
        assert_eq!(clean.styles, no_sanitize.styles, "санитизация изменила styles для {}", entry.id);
        assert_eq!(clean.headings, no_sanitize.headings, "санитизация изменила headings для {}", entry.id);
        assert_eq!(clean.links, no_sanitize.links, "санитизация изменила links для {}", entry.id);
        assert_eq!(clean.tags, no_sanitize.tags, "санитизация изменила tags для {}", entry.id);
        assert_eq!(clean.sanitizer, None, "для фикстуры не должно быть удалений: {}", entry.id);
    }
}

#[test]
fn malicious_html_is_neutralized_and_reported() {
    let page = compile(
        r##"
#context if target() == "html" {
  html.elem("script", "x")
  html.elem("iframe", "x")
  html.elem("object", "x")
  html.elem("embed", "x")
  html.elem("frame", "x")
  html.elem("base", attrs: (href: "https://evil/"))
  html.elem("meta", attrs: (name: "x", content: "y"))
  html.elem("link", attrs: (rel: "stylesheet", href: "https://evil/style.css"))
  html.elem("form", html.elem("input"))

  html.elem("a", attrs: (href: " JaVa\nScRiPt:alert(1)"), "js")
  html.elem("a", attrs: (href: "jav&#x61;script:alert(1)"), "encoded")
  html.elem("a", attrs: (href: "vbscript:msgbox(1)"), "vb")
  html.elem("img", attrs: (src: "data:text/html,boom", onerror: "alert(1)"))
  html.elem("img", attrs: (src: "data:image/png;base64,AA=="))

  html.elem("svg", attrs: (xmlns: "http://www.w3.org/2000/svg"), {
    html.elem("foreignObject", "x")
    html.elem("use", attrs: (href: "https://evil/x.svg#id"))
    html.elem("use", attrs: (href: "#inside"))
    html.elem("animate", attrs: ("attributeName": "href", from: "#a", to: "#b"))
    html.elem("set", attrs: ("attributeName": "onclick", to: "x"))
  })

  html.elem("div", attrs: (style: "background:url(javascript:alert(1))"), "x")
  html.elem("style", "p{width:expression(alert(1))}")
}
"##,
    );

    assert!(page.errors.is_empty(), "сборка должна пройти");
    let rendered = page.rendered.as_ref().unwrap();
    let body = &rendered.body;

    for banned in [
        "<script",
        "<iframe",
        "<object",
        "<embed",
        "<frame",
        "<base",
        "<meta",
        "<link",
        "<form",
        "<input",
        "foreignObject",
        "javascript:",
        "vbscript:",
        "onerror",
        "https://evil/x.svg#id",
    ] {
        assert!(!body.contains(banned), "в output остался опасный фрагмент: {banned}");
    }

    assert!(body.contains("href=\"#inside\""), "локальный SVG use должен остаться");
    assert!(body.contains("data:image/png"), "data:image должно оставаться");

    let sanitizer_warnings: Vec<_> = page.warnings.iter().filter(|w| w.message.starts_with("очищено HTML:")).collect();
    assert_eq!(sanitizer_warnings.len(), 1, "ожидается одно предупреждение санитайзера");
    assert!(!sanitizer_warnings[0].message.contains('\n'), "предупреждение должно быть в одну строку");
}

fn render_with_tree_passes(
    docs: Vec<(String, HtmlDocument)>,
    vault: &Vault,
    tree_passes: &[TreePass],
) -> Result<Rendered, String> {
    let mut docs = docs.into_iter();
    let (base_theme, mut base) = docs.next().ok_or("нет ни одного документа")?;

    let mut themes = vec![base_theme];
    let mut frames: Vec<Vec<HtmlFrame>> = Vec::new();
    for (theme, doc) in docs {
        frames.push(collect_frames(doc.root()));
        themes.push(theme);
    }
    let base_count = count_frames(base.root());
    if let Some((i, f)) = frames.iter().enumerate().find(|(_, f)| f.len() != base_count) {
        return Err(format!(
            "в теме «{}» рисунков {}, а в «{}» — {base_count}: рисунок зависит от темы не только цветом",
            themes[i + 1],
            f.len(),
            themes[0],
        ));
    }

    let mut ids = HashSet::new();
    walk(base.root(), &mut |el| {
        if let Some(id) = el.attrs.get(attr::id) {
            ids.insert(id.to_string());
        }
    });

    let links = VaultLinks { vault };
    let mut ctx = Context::new(&themes, frames, ids, &links);
    passes::run_tree(base.root_mut(), &mut ctx, tree_passes);
    let Context { headings, out_links, tags, removed_tags, removed_attrs, removed_urls, .. } = ctx;

    let title = base.info().title.as_ref().map(ToString::to_string);
    let html = typst_html::html(&base, &HtmlOptions::default())
        .map_err(|errs| errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; "))?;
    let (styles, body) = split_html(&html);
    let sanitizer =
        (removed_tags + removed_attrs + removed_urls > 0).then_some((removed_tags, removed_attrs, removed_urls));
    let mut page = Rendered { title, styles, body, headings, links: out_links, tags, sanitizer };
    passes::run_text(&mut page, passes::TEXT);
    Ok(page)
}

fn has_class(el: &HtmlElement, class: &str) -> bool {
    el.attrs.get(attr::class).is_some_and(|c| c.split_whitespace().any(|c| c == class))
}

fn walk(el: &HtmlElement, f: &mut dyn FnMut(&HtmlElement)) {
    f(el);
    for child in &el.children {
        if let HtmlNode::Element(e) = child {
            walk(e, f);
        }
    }
}

fn count_frames(root: &HtmlElement) -> usize {
    let mut count = 0;
    walk(root, &mut |el| count += usize::from(has_class(el, "k-frame")));
    count
}

fn collect_frames(root: &HtmlElement) -> Vec<HtmlFrame> {
    let mut out = Vec::new();
    walk(root, &mut |el| {
        if has_class(el, "k-frame")
            && let Some(frame) = el.children.iter().find_map(|child| match child {
                HtmlNode::Frame(frame) => Some(frame.clone()),
                _ => None,
            })
        {
            out.push(frame);
        }
    });
    out
}

fn split_html(html: &str) -> (String, String) {
    let between = |open: &str, close: &str| {
        let start = html.find(open).map(|i| i + open.len())?;
        let end = html[start..].find(close)? + start;
        Some(&html[start..end])
    };
    let head = between("<head>", "</head>").unwrap_or_default();
    let mut styles = String::new();
    let mut rest = head;
    while let Some(start) = rest.find("<style>") {
        let Some(len) = rest[start..].find("</style>") else { break };
        let end = start + len + "</style>".len();
        styles.push_str(&rest[start..end]);
        rest = &rest[end..];
    }
    let body = between("<body>", "</body>").unwrap_or(html);
    (styles, body.to_owned())
}

struct VaultLinks<'a> {
    vault: &'a Vault,
}

impl LinkResolver for VaultLinks<'_> {
    fn href(&self, target: &str, anchor: Option<&str>) -> Option<String> {
        let id = NoteId::new(target).ok()?;
        self.vault.entry(&id).ok()?;
        let fragment = anchor.map(|a| format!("#{}", encode(&notes_core::render::slug(a)))).unwrap_or_default();
        Some(format!("/n/{}{fragment}", encode(id.as_str())))
    }
}

#[allow(dead_code)]
fn _exists(path: &Path) -> bool {
    path.exists()
}
