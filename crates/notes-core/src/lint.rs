//! Проверки исходников заметок, которых нет в Typst: то, что компилируется
//! без ошибок, но выглядит не так, как задумано. Результат — предупреждения
//! в `NotePage::warnings` (видны в приложении и в `notes check`).

use typst::syntax::{LinkedNode, SyntaxKind};

/// Предупреждение в исходнике: место — байтовое смещение.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    pub offset: usize,
    pub message: &'static str,
    pub hint: &'static str,
}

/// Все предупреждения для текста одного файла.
pub fn lint(text: &str) -> Vec<Lint> {
    let root = typst::syntax::parse(text);
    let mut out = Vec::new();
    visit(&LinkedNode::new(&root), &mut out);
    out
}

/// `#see("A"); дальше` — в разметке `;` сразу после `#выражения` завершает
/// выражение и исчезает из текста. Для `#let x = 1;` это задумано, для
/// вызовов посреди фразы — потерянный знак препинания.
fn visit(node: &LinkedNode, out: &mut Vec<Lint>) {
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
    for child in node.children() {
        visit(&child, out);
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
        lint(text).iter().map(|l| l.offset).collect()
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
