//! The sanitizer on real pages: the fixture renders the same with and without
//! it, and malicious HTML is removed and reported in one warning.

use std::sync::Arc;

use notes_core::LibrarySource;
use notes_core::fonts::Fonts;
use notes_core::passes;
use notes_core::pipeline::encode;
use notes_core::render::{LinkResolver, render_with};
use notes_core::themes::ThemeSet;
use notes_core::vault::{NoteId, Vault};
use notes_core::version::Versions;
use notes_core::world::{Compiler, Priority};

use crate::common::{compile, repo};

#[test]
fn vault_pages_are_identical_with_and_without_sanitizer() {
    let vault = Vault::open(repo().join("tests/vault")).unwrap();
    let versions = Versions::new(vault.storage().clone());
    let fonts = Arc::new(Fonts::load(&[]));
    let compiler = Compiler::new(versions, LibrarySource::Dir(repo().join("baluk")), fonts);
    let themes = ThemeSet::load(&compiler).unwrap();
    let links = VaultLinks { vault: &vault };

    for entry in vault.entries().unwrap() {
        let build = compiler.compile_html(&entry.main, &themes.names(), Priority::User);
        let Ok(docs) = build.docs else { continue };
        let clean = render_with(docs.clone(), &links, passes::TREE).unwrap();
        let no_sanitize = render_with(docs, &links, &passes::TREE[1..]).unwrap();

        assert_eq!(clean.body, no_sanitize.body, "sanitizing changed the body of {}", entry.id);
        assert_eq!(clean.styles, no_sanitize.styles, "sanitizing changed the styles of {}", entry.id);
        assert_eq!(clean.headings, no_sanitize.headings, "sanitizing changed the headings of {}", entry.id);
        assert_eq!(clean.links, no_sanitize.links, "sanitizing changed the links of {}", entry.id);
        assert_eq!(clean.tags, no_sanitize.tags, "sanitizing changed the tags of {}", entry.id);
        assert_eq!(clean.sanitizer, None, "nothing should be removed from the fixture: {}", entry.id);
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

    assert!(page.errors.is_empty(), "the build must pass");
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
        assert!(!body.contains(banned), "a dangerous fragment is left in the output: {banned}");
    }

    assert!(body.contains("href=\"#inside\""), "a local SVG use must stay");
    assert!(body.contains("data:image/png"), "data:image must stay");

    let sanitizer_warnings: Vec<_> =
        page.warnings.iter().filter(|w| w.message.starts_with("HTML sanitized:")).collect();
    assert_eq!(sanitizer_warnings.len(), 1, "one sanitizer warning is expected");
    assert!(!sanitizer_warnings[0].message.contains('\n'), "the warning must be one line");
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
