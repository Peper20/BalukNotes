//! The outline of a source without compiling: the title and tags from the
//! template, sections and their text - for quick navigation, search and
//! tags. A book chapter with its own properties
//! (`#show: chapter.with(title: [...], tags: (...))`) is a first-level
//! section with its own tags.
//!
//! Like links ([`crate::graph`]), it comes from the Typst syntax tree: the
//! whole index is built in milliseconds. The text is approximate: formulas
//! in the text are skipped, and in the title and headings they are kept as
//! source without `$` (`Ряд $sum 1/n^2$` -> "Ряд sum 1/n^2", user's
//! decision: the formula does not vanish in the tree, the tabs and the
//! graph); computed content (`#let x = ...; #x`) is not expanded. That is
//! enough for search; only a built page has the exact text.

use typst::syntax::ast::AstNode as _;
use typst::syntax::{SyntaxKind, SyntaxNode, ast};

/// Link to a note from `baluk/links.typ`.
const LINK_FN: &str = "see";
/// baluk templates we take `title` and `tags` from.
const TEMPLATES: &[&str] = &["note", "book"];
/// A book chapter: its `title` is a first-level heading, `tags` are its own tags.
const CHAPTER: &str = "chapter";
/// Named string arguments the reader sees (block captions).
const VISIBLE_ARGS: &[&str] = &["title", "label", "caption", "description", "subtitle"];

/// The outline of one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    /// Title from the template.
    pub title: Option<String>,
    /// Template tags; of a book - of the root (`main.typ`), all chapters inherit them.
    pub tags: Vec<String>,
    /// The first section is the text before the first heading (no heading).
    pub sections: Vec<Section>,
}

/// A section: a heading and the text up to the next one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// Heading text; `None` - the start of the file before the first heading.
    pub heading: Option<String>,
    /// The level of `=` (1 - `=`).
    pub level: usize,
    /// Heading label `= Раздел <label>`: it becomes its `id`.
    pub label: Option<String>,
    /// Own tags of a book chapter (`chapter.with(tags: ...)`); empty for other sections.
    pub tags: Vec<String>,
    /// Plain text of the section.
    pub text: String,
    /// `#see` links in the section text (no repeats): whose they are on the
    /// graph with book chapters. The full list of a note's links is in the
    /// index ([`crate::graph`]).
    pub links: Vec<crate::render::LinkRef>,
}

impl Outline {
    /// Tags of the note or book: of the root, then of the chapters (may repeat).
    pub fn all_tags(&self) -> impl Iterator<Item = &str> {
        self.tags.iter().chain(self.sections.iter().flat_map(|s| &s.tags)).map(String::as_str)
    }

    /// Whether the note or book has the tag: at the root or in at least one chapter.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.all_tags().any(|t| t == tag)
    }
}

/// Parses the outline of a source.
pub fn parse_outline(source: &str) -> Outline {
    let root = typst::syntax::parse(source);
    let mut w = Walker::default();
    w.walk(&root, SyntaxKind::Markup);
    w.finish_section();
    w.out.sections.retain(|s| s.heading.is_some() || !s.text.is_empty() || !s.links.is_empty());
    w.out
}

#[derive(Default)]
struct Walker {
    out: Outline,
    text: String,
    heading: Option<String>,
    level: usize,
    label: Option<String>,
    tags: Vec<String>,
    links: Vec<crate::render::LinkRef>,
    /// Title or heading: a formula as source, not skipped.
    formulas: bool,
}

impl Walker {
    /// For the title and a heading: formulas as source.
    fn inline() -> Self {
        Self { formulas: true, ..Self::default() }
    }

    fn walk(&mut self, node: &SyntaxNode, parent: SyntaxKind) {
        match node.kind() {
            // A quote as written (`'` does not turn into `"`).
            SyntaxKind::Text | SyntaxKind::SmartQuote => self.text.push_str(node.leaf_text()),
            SyntaxKind::Space | SyntaxKind::Linebreak | SyntaxKind::Parbreak | SyntaxKind::RawTrimmed => {
                self.space();
            }
            SyntaxKind::Equation if self.formulas => {
                let source = node.full_text();
                self.text.push_str(source.trim_matches('$').trim());
            }
            SyntaxKind::Equation => self.space(),
            SyntaxKind::Shorthand => {
                if let Some(s) = node.cast::<ast::Shorthand>() {
                    self.text.push(s.get());
                }
            }
            SyntaxKind::Escape => {
                if let Some(e) = node.cast::<ast::Escape>() {
                    self.text.push(e.get());
                }
            }
            SyntaxKind::Str => {
                if parent == SyntaxKind::Args
                    && let Some(s) = node.cast::<ast::Str>()
                {
                    self.space();
                    self.text.push_str(&s.get());
                    self.space();
                }
            }
            SyntaxKind::Named => {
                let named = node.cast::<ast::Named>();
                let visible = named.is_some_and(|n| VISIBLE_ARGS.contains(&n.name().as_str()));
                for child in node.children() {
                    // The string of a named argument only if the reader sees it.
                    self.walk(child, if visible { SyntaxKind::Args } else { SyntaxKind::Named });
                }
            }
            SyntaxKind::Heading => self.heading(node),
            SyntaxKind::FuncCall if self.link(node) => {}
            // `= Раздел <label>`: the label is the heading's neighbour right after it.
            SyntaxKind::Label if self.heading.is_some() && self.label.is_none() && self.text.trim().is_empty() => {
                self.label = node.cast::<ast::Label>().map(|l| l.get().to_owned());
            }
            SyntaxKind::ShowRule => self.template(node),
            SyntaxKind::ModuleImport
            | SyntaxKind::ModuleInclude
            | SyntaxKind::LetBinding
            | SyntaxKind::SetRule
            | SyntaxKind::Label
            | SyntaxKind::Ref
            | SyntaxKind::Keyed
            | SyntaxKind::RawLang
            | SyntaxKind::LineComment
            | SyntaxKind::BlockComment => {}
            kind => {
                for child in node.children() {
                    self.walk(child, kind);
                }
            }
        }
    }

    fn space(&mut self) {
        if !self.text.is_empty() && !self.text.ends_with(' ') {
            self.text.push(' ');
        }
    }

    fn heading(&mut self, node: &SyntaxNode) {
        let Some(h) = node.cast::<ast::Heading>() else { return };
        self.finish_section();
        let mut inner = Walker::inline();
        inner.walk(h.body().to_untyped(), SyntaxKind::Markup);
        self.heading = Some(normalize(&inner.text));
        self.level = h.depth().get();
    }

    fn finish_section(&mut self) {
        let text = normalize(&std::mem::take(&mut self.text));
        self.out.sections.push(Section {
            heading: self.heading.take(),
            level: self.level,
            label: self.label.take(),
            tags: std::mem::take(&mut self.tags),
            text,
            links: std::mem::take(&mut self.links),
        });
    }

    /// `#see("Сеть/SSH")`: the caption as on the page (`baluk/links.typ`) -
    /// own text, otherwise the anchor, otherwise the last path segment.
    fn link(&mut self, node: &SyntaxNode) -> bool {
        let Some(call) = node.cast::<ast::FuncCall>() else { return false };
        let ast::Expr::Ident(name) = call.callee() else { return false };
        if name.as_str() != LINK_FN {
            return false;
        }
        let mut target = None;
        let mut anchor = None;
        let mut body = None;
        for arg in call.args().items() {
            match arg {
                ast::Arg::Pos(ast::Expr::Str(s)) if target.is_none() => target = Some(s.get()),
                ast::Arg::Pos(ast::Expr::ContentBlock(b)) => body = Some(b),
                ast::Arg::Named(n) if n.name().as_str() == "anchor" => {
                    if let ast::Expr::Str(s) = n.expr() {
                        anchor = Some(s.get());
                    }
                }
                _ => {}
            }
        }
        if let Some(target) = &target {
            let link =
                crate::render::LinkRef { target: target.to_string(), anchor: anchor.as_ref().map(ToString::to_string) };
            if !self.links.contains(&link) {
                self.links.push(link);
            }
        }
        if let Some(body) = body {
            self.walk(body.body().to_untyped(), SyntaxKind::Markup);
        } else if let Some(label) = anchor.or_else(|| target.map(|t| t.rsplit('/').next().unwrap_or(&t).into())) {
            self.text.push_str(&label);
        }
        true
    }

    /// `#show: note.with(title: [...], tags: (...))`; a chapter is
    /// `#show: chapter.with(title: [...], tags: (...), label: "...")`.
    fn template(&mut self, node: &SyntaxNode) {
        let Some(rule) = node.cast::<ast::ShowRule>() else { return };
        let ast::Expr::FuncCall(call) = rule.transform() else { return };
        let ast::Expr::FieldAccess(access) = call.callee() else { return };
        let ast::Expr::Ident(name) = access.target() else { return };
        let chapter = name.as_str() == CHAPTER;
        if !(chapter || TEMPLATES.contains(&name.as_str())) || access.field().as_str() != "with" {
            return;
        }
        let mut title = None;
        let mut tags = Vec::new();
        let mut label = None;
        for arg in call.args().items() {
            let ast::Arg::Named(n) = arg else { continue };
            match (n.name().as_str(), n.expr()) {
                ("title", ast::Expr::ContentBlock(block)) => {
                    let mut inner = Walker::inline();
                    inner.walk(block.body().to_untyped(), SyntaxKind::Markup);
                    title = Some(normalize(&inner.text));
                }
                ("tags", ast::Expr::Array(array)) => {
                    tags = array
                        .items()
                        .filter_map(|item| match item {
                            ast::ArrayItem::Pos(ast::Expr::Str(s)) => Some(s.get().to_string()),
                            _ => None,
                        })
                        .collect();
                }
                ("label", ast::Expr::Str(s)) => label = Some(s.get().to_string()),
                _ => {}
            }
        }
        if chapter {
            // The template sets the chapter heading itself, as `= Название <label>`.
            self.finish_section();
            self.heading = Some(title.unwrap_or_default());
            self.level = 1;
            self.label = label;
            self.tags = tags;
        } else {
            self.out.title = title.filter(|t| !t.is_empty());
            self.out.tags = tags;
        }
    }
}

/// Whitespace collapsed, none at the edges.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_tags_sections() {
        let o = parse_outline(
            r#"#import "/_baluk/lib.typ": *
#show: note.with(title: [SSH и *ключи*], tags: ("сеть", "безопасность"))
#let x = "не текст"

#lead[Протокол для входа на удалённую машину.]

= Вход по ключу <ключ>
Команда `ssh-keygen -t ed25519` создаёт пару -- ключей. Формула $x^2$ пропущена.
#definition(title: "ключа", lang: "служебное")[*Ключ* — файл.]
#data-table(highlight: ("1,*": "line"), [ячейка])

== Смена порта
// комментарий не текст
Порт~22 → #see("Сеть/UFW")[межсетевой экран]\; дальше.
```sh
ssh-keygen
ssh-copy-id host
```
"#,
        );
        assert_eq!(o.title.as_deref(), Some("SSH и ключи"));
        assert_eq!(o.tags, ["сеть", "безопасность"]);
        let heads: Vec<_> = o.sections.iter().map(|s| (s.heading.as_deref(), s.level, s.label.as_deref())).collect();
        assert_eq!(heads, [(None, 0, None), (Some("Вход по ключу"), 1, Some("ключ")), (Some("Смена порта"), 2, None)]);
        assert_eq!(o.sections[0].text, "Протокол для входа на удалённую машину.");
        let s1 = &o.sections[1].text;
        assert!(s1.contains("ssh-keygen -t ed25519"), "{s1}");
        assert!(s1.contains("пару – ключей"), "{s1}");
        assert!(s1.contains("ключа") && s1.contains("Ключ — файл."), "{s1}");
        assert!(s1.contains("ячейка"), "{s1}");
        for noise in ["служебное", "line", "1,*", "x^2", "не текст", "_baluk"] {
            assert!(!s1.contains(noise) && !o.sections[0].text.contains(noise), "\"{noise}\" in the text: {s1}");
        }
        let s2 = &o.sections[2].text;
        assert!(s2.contains("→ межсетевой экран; дальше."), "own link caption, no path: {s2}");
        assert!(!s2.contains("Сеть/UFW"), "{s2}");
        let o = parse_outline(r#"О ключах — #see("Сеть/SSH"). Порт — #see("Сеть/SSH", anchor: "Смена порта")."#);
        assert_eq!(o.sections[0].text, "О ключах — SSH. Порт — Смена порта.");
        assert!(!s2.contains("комментарий"));
        assert!(s2.contains("ssh-keygen ssh-copy-id host"), "code lines joined by a space: {s2}");
    }

    #[test]
    fn book_chapter_with_own_tags() {
        let o = parse_outline(
            r#"#import "/_baluk/lib.typ": *
#show: chapter.with(title: [Двойные *интегралы*], tags: ("интегралы",), label: "гл-2")
#lead[Зачем.]
== Пределы
Текст.
"#,
        );
        assert_eq!(o.title, None, "a chapter title is not the book title");
        assert!(o.tags.is_empty(), "chapter tags are not root tags");
        let heads: Vec<_> =
            o.sections.iter().map(|s| (s.heading.as_deref(), s.level, s.label.as_deref(), s.tags.clone())).collect();
        assert_eq!(
            heads,
            [
                (Some("Двойные интегралы"), 1, Some("гл-2"), vec!["интегралы".to_owned()]),
                (Some("Пределы"), 2, None, vec![])
            ]
        );
        assert_eq!(o.sections[0].text, "Зачем.");
        assert!(o.has_tag("интегралы") && !o.has_tag("пределы"));
    }

    #[test]
    fn formulas_in_title_and_headings_as_source() {
        let o = parse_outline(
            "#show: note.with(title: [Ряд $sum 1/n^2$])\n= Норма $norm(x)$ и $ y $\nТекст $a + b$ после.\n",
        );
        assert_eq!(o.title.as_deref(), Some("Ряд sum 1/n^2"));
        assert_eq!(o.sections[0].heading.as_deref(), Some("Норма norm(x) и y"));
        // In the section text the formula is still skipped.
        assert_eq!(o.sections[0].text, "Текст после.");
    }

    #[test]
    fn plain_typst_without_template() {
        let o = parse_outline("= Заголовок\nТекст.\n");
        assert_eq!(o.title, None);
        assert_eq!(o.sections.len(), 1);
        assert_eq!(o.sections[0].heading.as_deref(), Some("Заголовок"));
    }
}
