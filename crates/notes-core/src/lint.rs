//! Checks of note sources that Typst does not make: things that compile
//! without errors but do not look as intended. The result is warnings in
//! `NotePage::warnings` (shown in the app and in `notes check`).

use typst::syntax::ast;
use typst::syntax::{LinkedNode, SyntaxKind};

/// A warning in a source: the place is a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    /// Byte offset in the source.
    pub offset: usize,
    /// What is wrong.
    pub message: &'static str,
    /// How to fix it.
    pub hint: &'static str,
}

/// All warnings for the text of one file. `languages` are the languages of
/// the library's styling dictionaries (`ThemeSet::languages`); empty - the
/// language is not checked.
pub fn lint(text: &str, languages: &[String]) -> Vec<Lint> {
    let root = typst::syntax::parse(text);
    let mut out = Vec::new();
    visit(&LinkedNode::new(&root), languages, &mut out);
    out
}

fn visit(node: &LinkedNode, languages: &[String], out: &mut Vec<Lint>) {
    swallowed_semicolon(node, out);
    lang_without_words(node, languages, out);
    raw_decimal_comma(node, out);
    latex_decimal_comma(node, out);
    smallcaps_call(node, out);
    for child in node.children() {
        visit(&child, languages, out);
    }
}

/// `#see("A"); and on`: in markup `;` right after `#expression` ends the
/// expression and vanishes from the text. For `#let x = 1;` this is
/// intended, for calls in the middle of a sentence it is a lost punctuation
/// mark.
fn swallowed_semicolon(node: &LinkedNode, out: &mut Vec<Lint>) {
    if node.kind() == SyntaxKind::Semicolon
        && node.parent_kind() == Some(SyntaxKind::Markup)
        && node.prev_sibling().is_some_and(|prev| !is_statement(prev.kind()))
    {
        out.push(Lint {
            offset: node.offset(),
            message: "\";\" after #expression does not get into the text: Typst takes it as the end of the expression",
            hint: "if it is punctuation, write \\;",
        });
    }
}

/// `note.with(lang: "de")` (or `book`, or a call without `.with`) with a
/// language the library has no styling words for, and without own `words:`:
/// the styling words will be English ("Definition", "Fig.", "Chapter").
fn lang_without_words(node: &LinkedNode, languages: &[String], out: &mut Vec<Lint>) {
    if languages.is_empty() {
        return;
    }
    let Some(call) = node.cast::<ast::FuncCall>() else { return };
    let is_template = |name: &str| name == "note" || name == "book";
    let template = match call.callee() {
        ast::Expr::Ident(id) => is_template(&id),
        ast::Expr::FieldAccess(access) => {
            access.field().as_str() == "with" && matches!(access.target(), ast::Expr::Ident(id) if is_template(&id))
        }
        _ => false,
    };
    if !template {
        return;
    }
    let mut lang = None;
    for arg in call.args().items() {
        let ast::Arg::Named(named) = arg else { continue };
        match named.name().as_str() {
            "words" => return,
            "lang" => {
                if let ast::Expr::Str(s) = named.expr() {
                    lang = Some(s.get());
                }
            }
            _ => {}
        }
    }
    let Some(lang) = lang else { return };
    if languages.iter().any(|l| *l == lang.as_str()) {
        return;
    }
    // The place is the `lang:` argument.
    let is_lang = |n: &LinkedNode| n.cast::<ast::Named>().is_some_and(|named| named.name().as_str() == "lang");
    let offset = node
        .children()
        .find(|c| c.kind() == SyntaxKind::Args)
        .and_then(|args| args.children().find(is_lang))
        .map_or(node.offset(), |n| n.offset());
    out.push(Lint {
        offset,
        message: "the library has no styling words for this language: \"Definition\", \"Fig.\", \"Chapter\" will be English",
        hint: "give your own words: words: (definition: \"...\", figure: \"...\", ...), keys in baluk/i18n.typ",
    });
}

/// `$0,5$`: a comma between digits without spaces; Typst prints "0, 5".
/// A decimal fraction is `dc("0,5")` (`baluk/math.typ`). Not a fraction:
/// brackets holding only comma-separated numbers - a tuple, an interval, a
/// set, arguments and indices (`(3,6)`, `[0,1]`, `{1,2}`, `N(0,1)`,
/// `x_(1,2)`, `\{2,3\}`).
fn raw_decimal_comma(node: &LinkedNode, out: &mut Vec<Lint>) {
    if node.kind() != SyntaxKind::MathText || node.leaf_text() != "," {
        return;
    }
    let (Some(prev), Some(next)) = (node.prev_sibling_with_trivia(), node.next_sibling_with_trivia()) else {
        return;
    };
    if !is_number(&prev) || !is_number(&next) {
        return;
    }
    let (left, right) = (list_edge(node, false), list_edge(node, true));
    let escaped_set =
        left.as_ref().is_some_and(|n| n.leaf_text() == "\\{") && right.as_ref().is_some_and(|n| n.leaf_text() == "\\}");
    let whole_group = left.is_none()
        && right.is_none()
        && node.parent().and_then(LinkedNode::parent).is_some_and(|g| g.kind() != SyntaxKind::Equation);
    if escaped_set || whole_group {
        return;
    }
    out.push(Lint {
        offset: prev.offset(),
        message: "a comma between digits in a formula: Typst prints \"0, 5\", not a decimal fraction",
        hint: "a decimal fraction is $dc(\"0,5\")$; for a list put a space: $0, 5$",
    });
}

fn is_number(node: &LinkedNode) -> bool {
    node.kind() == SyntaxKind::MathText && node.leaf_text().bytes().all(|b| b.is_ascii_digit())
}

/// The first neighbour (forward or back) that is not a number, a comma or
/// a space; `None` if the list reaches the edge of the group.
fn list_edge<'a>(node: &LinkedNode<'a>, forward: bool) -> Option<LinkedNode<'a>> {
    let mut current = node.clone();
    loop {
        let step = if forward { current.next_sibling_with_trivia() } else { current.prev_sibling_with_trivia() };
        match step {
            Some(n) if is_number(&n) || n.leaf_text() == "," || n.kind() == SyntaxKind::Space => current = n,
            other => return other,
        }
    }
}

/// `{,}` from LaTeX: Typst prints curly braces literally.
fn latex_decimal_comma(node: &LinkedNode, out: &mut Vec<Lint>) {
    if node.kind() != SyntaxKind::MathDelimited {
        return;
    }
    let children: Vec<LinkedNode> = node.children().collect();
    let inner_is_comma = |n: &LinkedNode| {
        let items: Vec<LinkedNode> = n.children().collect();
        items.len() == 1 && items[0].leaf_text() == ","
    };
    if let [open, inner, _close] = children.as_slice()
        && open.leaf_text() == "{"
        && inner_is_comma(inner)
    {
        out.push(Lint {
            offset: node.offset(),
            message: "{,} from LaTeX: Typst prints the braces as they are",
            hint: "a decimal fraction is $dc(\"0,5\")$",
        });
    }
}

/// `smallcaps(...)` with styling fonts that have no Cyrillic small caps:
/// Typst silently prints lowercase.
fn smallcaps_call(node: &LinkedNode, out: &mut Vec<Lint>) {
    if let Some(call) = node.cast::<ast::FuncCall>()
        && matches!(call.callee(), ast::Expr::Ident(id) if id.as_str() == "smallcaps")
    {
        out.push(Lint {
            offset: node.offset(),
            message: "smallcaps does not work with Cyrillic: the fonts have no small caps, you get lowercase",
            hint: "small caps from the library: small-caps(...)",
        });
    }
}

fn is_statement(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LetBinding
            | SyntaxKind::SetRule
            | SyntaxKind::ShowRule
            | SyntaxKind::ModuleImport
            | SyntaxKind::ModuleInclude
            | SyntaxKind::Contextual
            | SyntaxKind::Conditional
            | SyntaxKind::WhileLoop
            | SyntaxKind::ForLoop
            | SyntaxKind::LoopBreak
            | SyntaxKind::LoopContinue
            | SyntaxKind::FuncReturn
    )
}

/// Line and column (from one, the column in characters) of a byte offset.
pub fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offsets(text: &str) -> Vec<usize> {
        lint(text, &[]).iter().map(|l| l.offset).collect()
    }

    fn lang_lints(text: &str) -> Vec<(usize, usize)> {
        let languages = ["ru".to_owned(), "en".to_owned()];
        lint(text, &languages).iter().map(|l| line_column(text, l.offset)).collect()
    }

    #[test]
    fn lang_without_dictionary() {
        assert_eq!(lang_lints("#show: note.with(title: [A], lang: \"de\")\n"), vec![(1, 30)]);
        assert_eq!(lang_lints("#show: book.with(lang: \"uk\")\n").len(), 1);
        assert_eq!(lang_lints("#show: doc => note(lang: \"de\", doc)\n").len(), 1);
        assert!(lang_lints("#show: note.with(lang: \"en\")\n").is_empty(), "has a dictionary");
        assert!(lang_lints("#show: note.with(lang: \"de\", words: (figure: \"Abb.\"))\n").is_empty(), "own words");
        assert!(lang_lints("#show: note.with(title: [A])\n").is_empty(), "default language");
        assert!(lang_lints("#set text(lang: \"de\")\n#other.with(lang: \"de\")\n").is_empty(), "not a template");
        assert!(lint("#show: note.with(lang: \"de\")", &[]).is_empty(), "unknown languages: no check");
    }

    #[test]
    fn swallowed_semicolon_after_call() {
        let text = "Ссылка #see(\"A\"); дальше\n- пункт #see(\"B\")[текст];\n";
        let found = offsets(text);
        assert_eq!(found.len(), 2);
        assert_eq!(line_column(text, found[0]), (1, 17));
        assert_eq!(line_column(text, found[1]).0, 2);
    }

    #[test]
    fn raw_decimal_comma_in_math() {
        let found = |text: &str| offsets(text).len();
        assert_eq!(found("$0,5$"), 1);
        assert_eq!(found("$x = 1,25 + y$ и $(0,5 + 1)$"), 2);
        assert_eq!(found("$dc(\"0,5\")$ $0, 5$ $a,b$ 0,5 в тексте"), 0, "not a fraction");
        assert_eq!(found("$(3,6) + [0,1] + {1,2,3} + N(0,1) + x_(1,2) + A = \\{2,3\\}$"), 0, "lists");
        assert_eq!(found("// $0,5$ в комментарии\n"), 0);
    }

    #[test]
    fn latex_comma_and_smallcaps() {
        assert_eq!(offsets("$0{,}5$").len(), 1);
        assert_eq!(offsets("#smallcaps[Имя] и #small-caps[Имя]").len(), 1);
        assert_eq!(offsets("#{ smallcaps[a] }").len(), 1);
    }

    #[test]
    fn statements_and_escapes_are_fine() {
        assert!(offsets("#let x = 1;\n#set text(red);\n#import \"a.typ\": *;\n").is_empty());
        assert!(offsets("#let x = 1; текст").is_empty());
        assert!(offsets("#see(\"A\")\\; дальше; и ещё").is_empty(), "\\; and ; in text are plain text");
        assert!(offsets("#{ let a = 1; a }").is_empty(), "; inside code is not in markup");
    }
}
