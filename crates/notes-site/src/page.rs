//! Страница сайта целиком: шапка со стилями и скриптом, панель, оглавление,
//! заметка.

use crate::markup::escape;

pub(crate) struct Page<'a> {
    /// Путь к корню сайта (`../` на каждый уровень папок).
    pub up: &'a str,
    /// `note`, `book` или `index` — `data-kind` у `<html>`.
    pub kind: &'a str,
    pub title: &'a str,
    /// `<style>` страницы (цвета рисунков по темам).
    pub styles: &'a str,
    pub body: &'a str,
    /// Темы для скрипта сайта (`window.K_THEMES`), JSON.
    pub themes: &'a str,
}

impl Page<'_> {
    /// Страница целиком; `toc` — оглавление (идёт до `<main>`).
    pub(crate) fn html(&self, toc: &str) -> String {
        let Page { up, kind, styles, body, themes, .. } = self;
        format!(
            r#"<!DOCTYPE html><html lang="ru" data-kind="{kind}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{title}</title><link rel="stylesheet" href="{up}assets/fonts.css"><link rel="stylesheet" href="{up}assets/baluk.css"><link rel="stylesheet" href="{up}assets/themes.css">{styles}<script>window.K_THEMES = {themes};</script><script src="{up}assets/static.js"></script></head><body class="k-static"><nav class="k-toolbar"><a class="k-toolbar-home" href="{up}index.html">Все заметки</a><button id="k-theme" type="button" hidden></button></nav>{toc}<main class="k-note">{body}</main></body></html>"#,
            title = escape(self.title),
        )
    }
}
