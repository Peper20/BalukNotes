//! Цвета кода. В HTML подсветка идёт опорными цветами (см.
//! `baluk/code.typ`), здесь они становятся CSS-переменными темы.

use super::TextPass;
use crate::render::Rendered;

pub const PASS: TextPass = TextPass { name: "цвета кода", run };

/// Опорные цвета подсветки кода (`baluk/code.typ`) → переменные CSS.
const CODE_COLORS: [(&str, &str); 8] = [
    ("#010100", "text"),
    ("#010101", "key"),
    ("#010102", "type"),
    ("#010103", "string"),
    ("#010104", "number"),
    ("#010105", "comment"),
    ("#010106", "function"),
    ("#010107", "hl"),
];

fn run(page: &mut Rendered) {
    page.body = replace_code_colors(&page.body);
}

fn replace_code_colors(html: &str) -> String {
    let mut out = html.to_owned();
    for (hex, name) in CODE_COLORS {
        // Только в CSS-свойствах (style="color: …"): в SVG опорных цветов нет,
        // но и случайное совпадение в тексте не трогаем.
        out = out.replace(&format!("color: {hex}"), &format!("color: var(--k-code-{name})"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_colors_become_variables() {
        let got = replace_code_colors(r##"<span style="color: #010101">fn</span> fill="#010101""##);
        assert_eq!(got, r##"<span style="color: var(--k-code-key)">fn</span> fill="#010101""##);
    }
}
