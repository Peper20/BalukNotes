//! Проверки исходников заметок, которых нет в Typst: то, что компилируется
//! без ошибок, но выглядит не так, как задумано. Результат — предупреждения
//! в `NotePage::warnings` (видны в приложении и в `notes check`).

use typst::syntax::ast;
use typst::syntax::{LinkedNode, SyntaxKind};

/// Предупреждение в исходнике: место — байтовое смещение.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    pub offset: usize,
    pub message: &'static str,
    pub hint: &'static str,
}

/// Все предупреждения для текста одного файла. `languages` — языки словарей
/// оформления библиотеки (`ThemeSet::languages`); пусто — язык не проверяется.
pub fn lint(text: &str, languages: &[String]) -> Vec<Lint> {
    let root = typst::syntax::parse(text);
    let mut out = Vec::new();
    visit(&LinkedNode::new(&root), languages, &mut out);
    out
}

fn visit(node: &LinkedNode, languages: &[String], out: &mut Vec<Lint>) {
    swallowed_semicolon(node, out);
    lang_without_words(node, languages, out);
    for child in node.children() {
        visit(&child, languages, out);
    }
}

/// `#see("A"); дальше` — в разметке `;` сразу после `#выражения` завершает
/// выражение и исчезает из текста. Для `#let x = 1;` это задумано, для
/// вызовов посреди фразы — потерянный знак препинания.
fn swallowed_semicolon(node: &LinkedNode, out: &mut Vec<Lint>) {
    if node.kind() == SyntaxKind::Semicolon
        && node.parent_kind() == Some(SyntaxKind::Markup)
        && node.prev_sibling().is_some_and(|prev| !is_statement(prev.kind()))
    {
        out.push(Lint {
            offset: node.offset(),
            message: "«;» после #выражения не попадёт в текст: Typst считает его концом выражения",
            hint: "если это знак препинания, напишите \\;",
        });
    }
}

/// `note.with(lang: "de")` (или `book`, или вызов без `.with`) с языком, для
/// которого в библиотеке нет слов оформления, и без своих `words:` — слова
/// «Определение», «Рис.», «Глава» останутся русскими.
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
    // Место — аргумент `lang:`.
    let is_lang = |n: &LinkedNode| n.cast::<ast::Named>().is_some_and(|named| named.name().as_str() == "lang");
    let offset = node
        .children()
        .find(|c| c.kind() == SyntaxKind::Args)
        .and_then(|args| args.children().find(is_lang))
        .map_or(node.offset(), |n| n.offset());
    out.push(Lint {
        offset,
        message: "для этого языка в библиотеке нет слов оформления: «Определение», «Рис.», «Глава» останутся русскими",
        hint: "дайте свои слова: words: (definition: \"…\", figure: \"…\", …) — ключи в baluk/i18n.typ",
    });
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

/// Строка и столбец (с единицы, столбец — в символах) по байтовому смещению.
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
        assert!(lang_lints("#show: note.with(lang: \"en\")\n").is_empty(), "есть словарь");
        assert!(lang_lints("#show: note.with(lang: \"de\", words: (figure: \"Abb.\"))\n").is_empty(), "свои слова");
        assert!(lang_lints("#show: note.with(title: [A])\n").is_empty(), "язык по умолчанию");
        assert!(lang_lints("#set text(lang: \"de\")\n#other.with(lang: \"de\")\n").is_empty(), "не шаблон");
        assert!(lint("#show: note.with(lang: \"de\")", &[]).is_empty(), "языки неизвестны — не проверяем");
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
    fn statements_and_escapes_are_fine() {
        assert!(offsets("#let x = 1;\n#set text(red);\n#import \"a.typ\": *;\n").is_empty());
        assert!(offsets("#let x = 1; текст").is_empty());
        assert!(offsets("#see(\"A\")\\; дальше; и ещё").is_empty(), "\\; и ; в тексте — обычный текст");
        assert!(offsets("#{ let a = 1; a }").is_empty(), "; внутри кода — не в разметке");
    }
}
