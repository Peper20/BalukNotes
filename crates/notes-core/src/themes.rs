//! Themes: colors from `baluk/theme.typ` -> CSS variables.
//!
//! The source of truth is the theme dictionaries in Typst. `css.typ` exports
//! them together with derived colors (callout background etc.) to
//! `metadata <k-css>`, here they become blocks
//! `:root[data-theme="..."] { --k-...: ... }`. A new theme in `theme.typ`
//! shows up in the app without changes to Rust or CSS.
//!
//! Also the theme fonts, main and fallback ([`ThemeSet::web_fonts`]): the
//! server sends them to the browser (WOFF2 in chunks, `notes-core::webfonts`).
//! A theme with its own font needs no Rust change. And the languages of the
//! styling dictionaries ([`ThemeSet::languages`], `<k-langs>`) - for the
//! `lang:` check in `notes check`.

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

/// A theme font for the browser.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WebFamily {
    /// Family: "Gentium Plus".
    pub name: String,
    /// Math font (`font.math` of the theme): sent as is, with the `MATH` table.
    pub math: bool,
}

/// A theme for the settings and the client.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Theme {
    /// Name in `theme.typ`: the setting value and `data-theme`.
    pub name: String,
    /// Title for the interface.
    pub title: String,
    /// Whether the theme is dark (by background lightness), for the theme "auto".
    pub dark: bool,
}

/// The themes of the styling library: list, CSS, fonts and languages.
#[derive(Debug, Clone)]
pub struct ThemeSet {
    themes: Vec<Theme>,
    css: String,
    web_fonts: Vec<WebFamily>,
    /// Languages of the styling dictionaries (`i18n.typ`); empty if the
    /// library does not export them (an old one): note languages go unchecked.
    languages: Vec<String>,
}

impl ThemeSet {
    /// Builds `css.typ` of the library and reads the themes from it.
    pub fn load(compiler: &Compiler) -> Result<Self> {
        let main = Path::new(LIB_DIR).join(CSS_FILE);
        let compilation = compiler.compile_html(&main, &[], crate::world::Priority::User);
        let docs = compilation
            .docs
            .map_err(|errs| Error::Library(errs.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")))?;
        let Some((_, doc)) = docs.into_iter().next() else {
            return Err(Error::Library(format!("{CSS_FILE}: no document")));
        };
        let metadata = |name: &str| -> Result<Value> {
            let label = Label::new(PicoStr::intern(name))
                .ok_or_else(|| Error::Library(format!("{CSS_FILE}: empty label name")))?;
            let content = doc
                .introspector()
                .query_label(label)
                .map_err(|e| Error::Library(format!("{CSS_FILE}: no <{name}>: {e}")))?;
            let meta = content
                .to_packed::<MetadataElem>()
                .ok_or_else(|| Error::Library(format!("<{name}> is not metadata")))?;
            Ok(serde_json::to_value(&meta.value)?)
        };
        let mut set = Self::from_json(&metadata(CSS_LABEL)?)?;
        if let Ok(Value::Array(langs)) = metadata(LANGS_LABEL) {
            set.languages = langs.iter().filter_map(Value::as_str).map(str::to_owned).collect();
        }
        Ok(set)
    }

    /// `{theme: {title, colors: {variable: color}, fonts: [family]}}` -> themes and CSS.
    fn from_json(value: &Value) -> Result<Self> {
        let bad = |what: &str| Error::Library(format!("{CSS_FILE}: {what}"));
        let map = value.as_object().ok_or_else(|| bad("expected a dictionary of themes"))?;
        if map.is_empty() {
            return Err(bad("no themes"));
        }
        let mut themes = Vec::new();
        let mut css = String::from("/* Generated from baluk/theme.typ, do not edit. */\n");
        let mut web_fonts = BTreeSet::new();
        for (name, theme) in map {
            let title = theme.get("title").and_then(Value::as_str).unwrap_or(name);
            let colors: &Map<String, Value> = theme
                .get("colors")
                .and_then(Value::as_object)
                .ok_or_else(|| bad("theme colors must be a dictionary"))?;
            let bg = colors.get("bg").and_then(Value::as_str).ok_or_else(|| bad("the theme has no bg"))?;
            let dark = is_dark(bg);
            themes.push(Theme { name: name.clone(), title: title.to_owned(), dark });
            let names = |key: &str| -> Result<Vec<String>> {
                let Some(fonts) = theme.get(key) else { return Ok(Vec::new()) };
                let fonts = fonts.as_array().ok_or_else(|| bad("theme fonts must be a list"))?;
                fonts
                    .iter()
                    .map(|f| f.as_str().map(str::to_owned).ok_or_else(|| bad("a font must be a string")))
                    .collect()
            };
            let math = names("math-fonts")?;
            for name in names("fonts")? {
                web_fonts.insert(WebFamily { math: math.contains(&name), name });
            }
            let _ = writeln!(css, ":root[data-theme=\"{name}\"] {{");
            for (var, color) in colors {
                let color = color.as_str().ok_or_else(|| bad("a color must be a string"))?;
                let _ = writeln!(css, "  --k-{var}: {color};");
            }
            // Scrollbars, inputs and the browser window background follow the theme.
            let _ = writeln!(css, "  color-scheme: {};\n}}", if dark { "dark" } else { "light" });
            // Figures: only the variant of the current theme is visible (see render).
            let _ = writeln!(
                css,
                ":root[data-theme=\"{name}\"] .k-frame-v[data-theme=\"{name}\"] {{ display: contents; }}"
            );
        }
        Ok(Self { themes, css, web_fonts: web_fonts.into_iter().collect(), languages: Vec::new() })
    }

    /// The themes in the order of `theme.typ`.
    pub fn themes(&self) -> &[Theme] {
        &self.themes
    }

    /// Theme names.
    pub fn names(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.name.clone()).collect()
    }

    /// CSS of all themes.
    pub fn css(&self) -> &str {
        &self.css
    }

    /// Fonts of all themes, main and fallback, that the browser needs:
    /// alphabetical, without repeats. A font Typst does not have is simply
    /// not sent.
    pub fn web_fonts(&self) -> &[WebFamily] {
        &self.web_fonts
    }

    /// Languages the library has styling words for (`ru`, `en`).
    pub fn languages(&self) -> &[String] {
        &self.languages
    }
}

/// Whether the color `#rrggbb[aa]` is dark (relative luminance < 0.5).
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
            "dark": {"title": "Тёмная", "colors": {"bg": "#16181e", "text": "#dde2ea"}, "fonts": ["Serif", "Math"], "math-fonts": ["Math"]},
        });
        let set = ThemeSet::from_json(&json).unwrap();
        assert_eq!(set.names(), ["light", "dark"]);
        assert_eq!(set.themes()[1].title, "Тёмная");
        assert!(!set.themes()[0].dark);
        assert!(set.themes()[1].dark);
        assert!(set.css().contains(":root[data-theme=\"dark\"] {\n  --k-bg: #16181e;"));
        assert!(set.css().contains(".k-frame-v[data-theme=\"light\"] { display: contents; }"));
        assert!(set.css().contains("  --k-text: #dde2ea;\n  color-scheme: dark;\n}"));
        let fonts: Vec<_> = set.web_fonts().iter().map(|f| (f.name.as_str(), f.math)).collect();
        assert_eq!(fonts, [("Math", true), ("Mono", false), ("Serif", false)], "alphabetical, no repeats");
    }

    #[test]
    fn rejects_broken_themes() {
        assert!(ThemeSet::from_json(&serde_json::json!({})).is_err());
        assert!(ThemeSet::from_json(&serde_json::json!({"x": {"text": "#000"}})).is_err());
    }
}
