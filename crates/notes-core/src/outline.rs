//! Содержание исходника без компиляции: название и теги из шаблона,
//! разделы и их текст — для быстрого перехода, поиска и тегов.
//!
//! Как и ссылки ([`crate::graph`]), берётся из синтаксического дерева Typst:
//! весь индекс строится за миллисекунды. Текст — приблизительный: формулы
//! пропускаются, вычисляемое содержимое (`#let x = …; #x`) не раскрывается.
//! Для поиска этого достаточно; точный текст есть только у собранной страницы.

use typst::syntax::ast::AstNode as _;
use typst::syntax::{SyntaxKind, SyntaxNode, ast};

/// Ссылка на заметку из `baluk/links.typ`.
const LINK_FN: &str = "see";
/// Шаблоны baluk, у которых берём `название` и `теги`.
const TEMPLATES: &[&str] = &["note", "book"];
/// Именованные строковые аргументы, которые видны читателю (подписи блоков).
const VISIBLE_ARGS: &[&str] = &["title", "label", "caption", "description", "subtitle"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    pub title: Option<String>,
    pub tags: Vec<String>,
    /// Первый раздел — текст до первого заголовка (без заголовка).
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// Текст заголовка; `None` — начало файла до первого заголовка.
    pub heading: Option<String>,
    /// Уровень `=` (1 — `=`).
    pub level: usize,
    /// Метка заголовка `= Раздел <метка>` — она станет его `id`.
    pub label: Option<String>,
    pub text: String,
}

pub fn parse_outline(source: &str) -> Outline {
    let root = typst::syntax::parse(source);
    let mut w = Walker::default();
    w.walk(&root, SyntaxKind::Markup);
    w.finish_section();
    w.out.sections.retain(|s| s.heading.is_some() || !s.text.is_empty());
    w.out
}

#[derive(Default)]
struct Walker {
    out: Outline,
    text: String,
    heading: Option<String>,
    level: usize,
    label: Option<String>,
}

impl Walker {
    fn walk(&mut self, node: &SyntaxNode, parent: SyntaxKind) {
        match node.kind() {
            // Кавычка — как написана (`'` не становится `"`).
            SyntaxKind::Text | SyntaxKind::SmartQuote => self.text.push_str(node.leaf_text()),
            SyntaxKind::Space
            | SyntaxKind::Linebreak
            | SyntaxKind::Parbreak
            | SyntaxKind::RawTrimmed
            | SyntaxKind::Equation => {
                self.space();
            }
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
                    // Строка именованного аргумента — только если он виден читателю.
                    self.walk(child, if visible { SyntaxKind::Args } else { SyntaxKind::Named });
                }
            }
            SyntaxKind::Heading => self.heading(node),
            SyntaxKind::FuncCall if self.link(node) => {}
            // `= Раздел <метка>`: метка — сосед заголовка сразу после него.
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
        let mut inner = Walker::default();
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
            text,
        });
    }

    /// `#see("Сеть/SSH")` — надпись как на странице (`baluk/links.typ`):
    /// своя подпись, иначе якорь, иначе последний сегмент пути.
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
        if let Some(body) = body {
            self.walk(body.body().to_untyped(), SyntaxKind::Markup);
        } else if let Some(label) = anchor.or_else(|| target.map(|t| t.rsplit('/').next().unwrap_or(&t).into())) {
            self.text.push_str(&label);
        }
        true
    }

    /// `#show: note.with(title: […], tags: (…))`.
    fn template(&mut self, node: &SyntaxNode) {
        let Some(rule) = node.cast::<ast::ShowRule>() else { return };
        let ast::Expr::FuncCall(call) = rule.transform() else { return };
        let ast::Expr::FieldAccess(access) = call.callee() else { return };
        let ast::Expr::Ident(name) = access.target() else { return };
        if !TEMPLATES.contains(&name.as_str()) || access.field().as_str() != "with" {
            return;
        }
        for arg in call.args().items() {
            let ast::Arg::Named(n) = arg else { continue };
            match (n.name().as_str(), n.expr()) {
                ("title", ast::Expr::ContentBlock(block)) => {
                    let mut inner = Walker::default();
                    inner.walk(block.body().to_untyped(), SyntaxKind::Markup);
                    self.out.title = Some(normalize(&inner.text)).filter(|t| !t.is_empty());
                }
                ("tags", ast::Expr::Array(array)) => {
                    self.out.tags = array
                        .items()
                        .filter_map(|item| match item {
                            ast::ArrayItem::Pos(ast::Expr::Str(s)) => Some(s.get().to_string()),
                            _ => None,
                        })
                        .collect();
                }
                _ => {}
            }
        }
    }
}

/// Пробелы схлопнуты, по краям — без пробелов.
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
            assert!(!s1.contains(noise) && !o.sections[0].text.contains(noise), "«{noise}» в тексте: {s1}");
        }
        let s2 = &o.sections[2].text;
        assert!(s2.contains("→ межсетевой экран; дальше."), "своя подпись ссылки, без пути: {s2}");
        assert!(!s2.contains("Сеть/UFW"), "{s2}");
        let o = parse_outline(r#"О ключах — #see("Сеть/SSH"). Порт — #see("Сеть/SSH", anchor: "Смена порта")."#);
        assert_eq!(o.sections[0].text, "О ключах — SSH. Порт — Смена порта.");
        assert!(!s2.contains("комментарий"));
        assert!(s2.contains("ssh-keygen ssh-copy-id host"), "строки кода — через пробел: {s2}");
    }

    #[test]
    fn plain_typst_without_template() {
        let o = parse_outline("= Заголовок\nТекст.\n");
        assert_eq!(o.title, None);
        assert_eq!(o.sections.len(), 1);
        assert_eq!(o.sections[0].heading.as_deref(), Some("Заголовок"));
    }
}
