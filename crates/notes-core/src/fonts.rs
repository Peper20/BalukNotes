//! Шрифты: для компилятора и для браузера.
//!
//! Шрифты оформления (Gentium Plus, JetBrains Mono — каталог `fonts/`) и
//! шрифты Typst (New Computer Modern Math и др.) встроены в бинарник и
//! **заслоняют** одноимённые системные: отрисовка одинакова на любой машине,
//! а ставить шрифты в систему не нужно. Системные — для всего остального.
//!
//! Браузер рисует текст и формулы сам, поэтому ему нужны те же файлы
//! шрифтов, что и Typst, — их отдаёт [`Fonts::web_font`].

use std::any::Any;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use rust_embed::RustEmbed;
use typst::foundations::Bytes;
use typst::text::{Font, FontBook, FontInfo, FontStyle, FontVariant, FontWeight};
use typst::utils::LazyHash;
use typst_kit::fonts::{self, FontPath, FontStore};

/// Шрифты оформления из `fonts/`. В отладочной сборке rust-embed читает их
/// с диска.
#[derive(RustEmbed)]
#[folder = "../../fonts/"]
#[include = "*.ttf"]
#[include = "*.otf"]
struct EmbeddedFonts;

/// Шрифты оформления, встроенные в бинарник.
fn bundled_fonts() -> impl Iterator<Item = (Font, FontInfo)> {
    EmbeddedFonts::iter().filter_map(|name| EmbeddedFonts::get(&name)).flat_map(|file| {
        Font::iter(Bytes::new(file.data.into_owned())).map(|font| {
            let info = font.info().clone();
            (font, info)
        })
    })
}

pub struct Fonts {
    store: FontStore,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts").field("families", &self.store.book().families().count()).finish()
    }
}

/// Начертание для `@font-face`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebVariant {
    pub italic: bool,
    pub bold: bool,
}

impl WebVariant {
    pub const ALL: [Self; 4] = [
        Self { italic: false, bold: false },
        Self { italic: true, bold: false },
        Self { italic: false, bold: true },
        Self { italic: true, bold: true },
    ];

    /// Имя в URL: `regular`, `italic`, `bold`, `bold-italic`.
    pub fn slug(self) -> &'static str {
        match (self.bold, self.italic) {
            (false, false) => "regular",
            (false, true) => "italic",
            (true, false) => "bold",
            (true, true) => "bold-italic",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.slug() == slug)
    }

    fn typst(self) -> FontVariant {
        FontVariant {
            style: if self.italic { FontStyle::Italic } else { FontStyle::Normal },
            weight: if self.bold { FontWeight::BOLD } else { FontWeight::REGULAR },
            ..FontVariant::default()
        }
    }
}

/// Файл шрифта для браузера.
#[derive(Debug, Clone)]
pub struct WebFont {
    pub data: Bytes,
    /// MIME-тип: `font/ttf` или `font/otf`.
    pub mime: &'static str,
}

impl Fonts {
    /// Шрифты из `extra_dirs` (явно указанные — главнее всех), встроенные
    /// (оформления и Typst) и системные — кроме семейств, которые уже есть
    /// среди встроенных.
    pub fn load(extra_dirs: &[PathBuf]) -> Self {
        let mut store = FontStore::new();
        for dir in extra_dirs {
            store.extend(fonts::scan(dir));
        }
        let embedded: Vec<_> = bundled_fonts().chain(fonts::embedded()).collect();
        let shadowed: HashSet<String> = embedded.iter().map(|(_, info)| info.family.to_lowercase()).collect();
        store.extend(embedded);
        // Иначе системный шрифт того же семейства мог бы победить при выборе
        // (Typst предпочитает вариативный файл статическому).
        store.extend(fonts::system().filter(|(_, info)| !shadowed.contains(&info.family.to_lowercase())));
        Self { store }
    }

    pub fn book(&self) -> &LazyHash<FontBook> {
        self.store.book()
    }

    pub fn font(&self, index: usize) -> Option<Font> {
        self.store.font(index)
    }

    /// Файл шрифта семейства `family` именно этого начертания. `None`, если
    /// такого начертания нет (браузер достроит курсив и жирный сам — не
    /// нужно отдавать ему обычный файл под видом жирного) или шрифт лежит в
    /// коллекции (`.ttc`), из которой браузер не умеет выбирать.
    pub fn web_font(&self, family: &str, variant: WebVariant) -> Option<WebFont> {
        let book = self.store.book();
        let index = book.select(&family.to_lowercase(), variant.typst())?;
        let found = book.info(index)?.variant;
        let italic = found.style != FontStyle::Normal;
        let bold = found.weight >= FontWeight::SEMIBOLD;
        if (italic, bold) != (variant.italic, variant.bold) {
            return None;
        }
        let source: &dyn Any = self.store.source(index)?;
        let data = if let Some(path) = source.downcast_ref::<FontPath>() {
            if path.index != 0 {
                return None;
            }
            Bytes::new(fs::read(&path.path).ok()?)
        } else {
            let font = source.downcast_ref::<Font>()?;
            if font.index() != 0 {
                return None;
            }
            font.data().clone()
        };
        let mime = match data.get(..4)? {
            b"ttcf" => return None,
            b"OTTO" => "font/otf",
            _ => "font/ttf",
        };
        Some(WebFont { data, mime })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_are_embedded_and_win_over_system() {
        let fonts = Fonts::load(&[]);
        for family in ["Gentium Plus", "JetBrains Mono", "New Computer Modern Math"] {
            let index = fonts.book().select(&family.to_lowercase(), FontVariant::default()).expect(family);
            let source: &dyn Any = fonts.store.source(index).unwrap();
            assert!(source.is::<Font>(), "{family}: встроенный, не системный файл");
        }
        for family in ["Gentium Plus", "JetBrains Mono"] {
            for v in WebVariant::ALL {
                let font = fonts.web_font(family, v).unwrap_or_else(|| panic!("{family} {}", v.slug()));
                assert_eq!(font.mime, "font/ttf");
            }
        }
    }
}
