//! Проходы после кэша: сырая страница → страница под настройки читателя.
//!
//! Сырая отрисовка ([`crate::render`], проходы [`crate::passes`]) лежит в
//! кэше; эти проходы зависят от настроек ([`FigureOptions`]) и выполняются
//! при выдаче страницы — смена настроек не перекомпилирует заметку. Модуль
//! (и его проходы) — в `AFTER_CACHE` (`build.rs`): их правка кэш на диске
//! не сбрасывает.
//!
//! Проходы: рисунки ([`crate::figures`]), общие части кадров
//! ([`crate::frames`]). Новый проход — одна строка в [`FINISH`]; время — в
//! логе («проход»).

use crate::figures::{self, FigureOptions};
use crate::frames;
use crate::passes::timed;
use crate::render::Rendered;

/// Проходы по порядку.
pub const FINISH: &[FinishPass] =
    &[FinishPass { name: "рисунки", run: figures_pass }, FinishPass { name: "кадры", run: frames_pass }];

/// Проход по готовой странице.
#[derive(Debug, Clone, Copy)]
pub struct FinishPass {
    pub name: &'static str,
    pub run: fn(&mut Rendered, &Settings<'_>),
}

/// Что знают проходы: темы (порядок вариантов рисунков) и настройки.
#[derive(Debug, Clone, Copy)]
pub struct Settings<'a> {
    pub themes: &'a [String],
    pub opts: FigureOptions,
}

/// Страница под настройки: копия сырой после всех проходов.
pub fn finish(raw: &Rendered, settings: &Settings<'_>, passes: &[FinishPass]) -> Rendered {
    let mut page = raw.clone();
    for pass in passes {
        timed(pass.name, || (pass.run)(&mut page, settings));
    }
    page
}

/// Рисунки: общие глифы, один SVG на темы, округление ([`crate::figures`]).
fn figures_pass(page: &mut Rendered, s: &Settings<'_>) {
    let o = figures::optimize(&page.body, s.themes, s.opts);
    tracing::debug!(
        before = page.body.len(),
        after = o.body.len(),
        figures = o.stats.figures,
        merged = o.stats.merged,
        glyphs = o.stats.glyphs,
        colors = o.stats.colors,
        "рисунки"
    );
    page.body = o.body;
    page.styles.push_str(&o.styles);
}

/// Кадры: общие части в `<defs>` ([`crate::frames`]). После рисунков:
/// темы уже склеены и координаты округлены.
fn frames_pass(page: &mut Rendered, _: &Settings<'_>) {
    let (body, stats) = frames::share(&page.body);
    if stats.groups > 0 {
        tracing::debug!(
            groups = stats.groups,
            shared = stats.shared,
            before = stats.before,
            after = stats.after,
            "кадры"
        );
    }
    page.body = body;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upper(page: &mut Rendered, s: &Settings<'_>) {
        page.body = format!("{}|{}", page.body.to_uppercase(), s.opts.key());
    }

    #[test]
    fn passes_run_in_order_on_a_copy() {
        let raw = Rendered {
            title: None,
            styles: "<style>a{}</style>".into(),
            body: "<p>x</p>".into(),
            headings: vec![],
            links: vec![],
            tags: vec![],
        };
        let themes = vec!["classic".to_owned()];
        let settings = Settings { themes: &themes, opts: FigureOptions::default() };
        let passes = [FinishPass { name: "верх", run: upper }, FinishPass { name: "рисунки", run: figures_pass }];
        let page = finish(&raw, &settings, &passes);
        assert_eq!(page.body, "<P>X</P>|p2");
        assert_eq!(page.styles, raw.styles, "без рисунков стилей не добавилось");
        assert_eq!(raw.body, "<p>x</p>", "сырая не тронута");
    }
}
