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
    raw_decimal_comma(node, out);
    latex_decimal_comma(node, out);
    smallcaps_call(node, out);
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
/// оформления будут английскими («Definition», «Fig.», «Chapter»).
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
        message: "для этого языка в библиотеке нет слов оформления: «Definition», «Fig.», «Chapter» будут английскими",
        hint: "дайте свои слова: words: (definition: \"…\", figure: \"…\", …) — ключи в baluk/i18n.typ",
    });
}

/// `$0,5$` — запятая между цифрами без пробелов: Typst напечатает «0, 5».
/// Десятичная дробь — `dc("0,5")` (`baluk/math.typ`). Не дробь — скобки,
/// в которых только числа через запятую: кортеж, отрезок, множество,
/// аргументы и индексы (`(3,6)`, `[0,1]`, `{1,2}`, `N(0,1)`, `x_(1,2)`,
/// `\{2,3\}`).
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
        message: "запятая между цифрами в формуле: Typst напечатает «0, 5», а не десятичную дробь",
        hint: "десятичная дробь — $dc(\"0,5\")$; перечисление — через пробел: $0, 5$",
    });
}

fn is_number(node: &LinkedNode) -> bool {
    node.kind() == SyntaxKind::MathText && node.leaf_text().bytes().all(|b| b.is_ascii_digit())
}

/// Первый сосед (вперёд или назад), который не число, не запятая и не
/// пробел; `None` — перечисление дошло до края группы.
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

/// `{,}` из LaTeX: в Typst фигурные скобки выводятся буквально.
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
            message: "{,} из LaTeX: в Typst скобки напечатаются как есть",
            hint: "десятичная дробь — $dc(\"0,5\")$",
        });
    }
}

/// `smallcaps(…)` у шрифтов оформления без кириллической капители: Typst
/// молча напечатает строчные.
fn smallcaps_call(node: &LinkedNode, out: &mut Vec<Lint>) {
    if let Some(call) = node.cast::<ast::FuncCall>()
        && matches!(call.callee(), ast::Expr::Ident(id) if id.as_str() == "smallcaps")
    {
        out.push(Lint {
            offset: node.offset(),
            message: "smallcaps не работает с кириллицей: у шрифтов нет капители, выйдут строчные",
            hint: "капитель из библиотеки — small-caps(…)",
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
    fn raw_decimal_comma_in_math() {
        let found = |text: &str| offsets(text).len();
        assert_eq!(found("$0,5$"), 1);
        assert_eq!(found("$x = 1,25 + y$ и $(0,5 + 1)$"), 2);
        assert_eq!(found("$dc(\"0,5\")$ $0, 5$ $a,b$ 0,5 в тексте"), 0, "не дробь");
        assert_eq!(found("$(3,6) + [0,1] + {1,2,3} + N(0,1) + x_(1,2) + A = \\{2,3\\}$"), 0, "перечисления");
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
        assert!(offsets("#see(\"A\")\\; дальше; и ещё").is_empty(), "\\; и ; в тексте — обычный текст");
        assert!(offsets("#{ let a = 1; a }").is_empty(), "; внутри кода — не в разметке");
    }
}
