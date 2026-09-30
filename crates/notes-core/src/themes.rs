//! Темы оформления: цвета из `baluk/theme.typ` → CSS-переменные.
//!
//! Источник правды — словари тем в Typst. `css.typ` выгружает их вместе с
//! производными цветами (фон врезки и т. п.) в `metadata <k-css>`, здесь они
//! превращаются в блоки `:root[data-theme="…"] { --k-…: … }`. Новая тема в
//! `theme.typ` появляется в приложении без правок Rust и CSS.
//!
//! Там же — основные шрифты тем ([`ThemeSet::web_fonts`]): их сервер
//! отдаёт браузеру (WOFF2 по частям, `notes-core::webfonts`).
//! Тема со своим шрифтом не требует правки Rust. И языки словарей оформления
//! ([`ThemeSet::languages`], `<k-langs>`) — для проверки `lang:` в `notes check`.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};
use typst::foundations::Label;
use typst::introspection::{Introspector as _, MetadataElem};
use typst::utils::PicoStr;

use crate::world::{Compiler, LIB_DIR};
use crate::{Error, Result};

const CSS_FILE: &str = "css.typ";
const CSS_LABEL: &str = "k-css";
const LANGS_LABEL: &str = "k-langs";

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Theme {
    /// Имя в `theme.typ` — значение настройки и `data-theme`.
    pub name: String,
    /// Название для интерфейса.
    pub title: String,
    /// Тёмная ли тема (по светлоте фона) — для выбора «как в системе».
    pub dark: bool,
}

#[derive(Debug, Clone)]
pub struct ThemeSet {
    themes: Vec<Theme>,
    css: String,
    web_fonts: Vec<String>,
    /// Языки словарей оформления (`i18n.typ`); пусто — библиотека их не
    /// выгружает (старая), язык заметок не проверяется.
    languages: Vec<String>,
}

impl ThemeSet {
    pub fn load(compiler: &Compiler) -> Result<Self> {
        let main = Path::new(LIB_DIR).join(CSS_FILE);
        let compilation = compiler.compile_html(&main, &[], crate::world::Priority::User);
        let docs = compilation
            .docs
            .map_err(|errs| Error::Library(errs.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")))?;
        let (_, doc) = docs.into_iter().next().expect("одна компиляция без темы");
        let metadata = |name: &str| -> Result<Value> {
            let label = Label::new(PicoStr::intern(name)).expect("метка не пустая");
            let content = doc
                .introspector()
                .query_label(label)
                .map_err(|e| Error::Library(format!("{CSS_FILE}: нет <{name}>: {e}")))?;
            let meta =
                content.to_packed::<MetadataElem>().ok_or_else(|| Error::Library(format!("<{name}> — не metadata")))?;
            Ok(serde_json::to_value(&meta.value)?)
        };
        let mut set = Self::from_json(&metadata(CSS_LABEL)?)?;
        if let Ok(Value::Array(langs)) = metadata(LANGS_LABEL) {
            set.languages = langs.iter().filter_map(Value::as_str).map(str::to_owned).collect();
        }
        Ok(set)
    }

    /// `{тема: {title, colors: {переменная: цвет}, fonts: [семейство]}}` → темы и CSS.
    fn from_json(value: &Value) -> Result<Self> {
        let bad = |what: &str| Error::Library(format!("{CSS_FILE}: {what}"));
        let map = value.as_object().ok_or_else(|| bad("ожидался словарь тем"))?;
        if map.is_empty() {
            return Err(bad("нет ни одной темы"));
        }
        let mut themes = Vec::new();
        let mut css = String::from("/* Сгенерировано из baluk/theme.typ — не править. */\n");
        let mut web_fonts = BTreeSet::new();
        for (name, theme) in map {
            let title = theme.get("title").and_then(Value::as_str).unwrap_or(name);
            let colors: &Map<String, Value> =
                theme.get("colors").and_then(Value::as_object).ok_or_else(|| bad("цвета темы — словарь"))?;
            let bg = colors.get("bg").and_then(Value::as_str).ok_or_else(|| bad("у темы нет bg"))?;
            let dark = is_dark(bg);
            themes.push(Theme { name: name.clone(), title: title.to_owned(), dark });
            if let Some(fonts) = theme.get("fonts") {
                let fonts = fonts.as_array().ok_or_else(|| bad("шрифты темы — список"))?;
                for font in fonts {
                    web_fonts.insert(font.as_str().ok_or_else(|| bad("шрифт — строка"))?.to_owned());
                }
            }
            let _ = writeln!(css, ":root[data-theme=\"{name}\"] {{");
            for (var, color) in colors {
                let color = color.as_str().ok_or_else(|| bad("цвет — строка"))?;
                let _ = writeln!(css, "  --k-{var}: {color};");
            }
            // Полосы прокрутки, поля ввода и фон окна браузера — в тон теме.
            let _ = writeln!(css, "  color-scheme: {};\n}}", if dark { "dark" } else { "light" });
            // Рисунки: виден только вариант текущей темы (см. render).
            let _ = writeln!(
                css,
                ":root[data-theme=\"{name}\"] .k-frame-v[data-theme=\"{name}\"] {{ display: contents; }}"
            );
        }
        Ok(Self { themes, css, web_fonts: web_fonts.into_iter().collect(), languages: Vec::new() })
    }

    pub fn themes(&self) -> &[Theme] {
        &self.themes
    }

    pub fn names(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.name.clone()).collect()
    }

    pub fn css(&self) -> &str {
        &self.css
    }

    /// Основные шрифты всех тем — те, что нужны браузеру: по алфавиту, без
    /// повторов. Шрифт, которого нет среди доступных Typst, просто не отдаётся.
    pub fn web_fonts(&self) -> &[String] {
        &self.web_fonts
    }

    /// Языки, для которых в библиотеке есть слова оформления (`ru`, `en`).
    pub fn languages(&self) -> &[String] {
        &self.languages
    }
}

/// Тёмный ли цвет `#rrggbb[aa]` (относительная яркость < 0.5).
fn is_dark(hex: &str) -> bool {
    let h = hex.trim_start_matches('#');
    let channel = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255);
    let (r, g, b) = (f64::from(channel(0)), f64::from(channel(2)), f64::from(channel(4)));
    (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0 < 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_from_json() {
        let json = serde_json::json!({
            "light": {"title": "Светлая", "colors": {"bg": "#ffffff", "text": "#1b1b1b"}, "fonts": ["Serif", "Mono"]},
            "dark": {"title": "Тёмная", "colors": {"bg": "#16181e", "text": "#dde2ea"}, "fonts": ["Serif", "Math"]},
        });
        let set = ThemeSet::from_json(&json).unwrap();
        assert_eq!(set.names(), ["light", "dark"]);
        assert_eq!(set.themes()[1].title, "Тёмная");
        assert!(!set.themes()[0].dark);
        assert!(set.themes()[1].dark);
        assert!(set.css().contains(":root[data-theme=\"dark\"] {\n  --k-bg: #16181e;"));
        assert!(set.css().contains(".k-frame-v[data-theme=\"light\"] { display: contents; }"));
        assert!(set.css().contains("  --k-text: #dde2ea;\n  color-scheme: dark;\n}"));
        assert_eq!(set.web_fonts(), ["Math", "Mono", "Serif"], "по алфавиту, без повторов");
    }

    #[test]
    fn rejects_broken_themes() {
        assert!(ThemeSet::from_json(&serde_json::json!({})).is_err());
        assert!(ThemeSet::from_json(&serde_json::json!({"x": {"text": "#000"}})).is_err());
    }
}
