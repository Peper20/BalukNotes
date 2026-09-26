//! Шрифты: для компилятора и для браузера.
//!
//! Шрифты оформления (Gentium Plus, JetBrains Mono — каталог `fonts/`) и
//! шрифты Typst (New Computer Modern Math и др.) встроены в бинарник и
//! **заслоняют** одноимённые системные: отрисовка одинакова на любой машине,
//! а ставить шрифты в систему не нужно. Системные — для всего остального.
//!
//! Браузер рисует текст и формулы сам, поэтому ему нужны те же файлы
//! шрифтов, что и Typst, — их отдаёт [`Fonts::web_face`]: в WOFF2 и по
//! частям (наборам знаков), см. [`crate::webfonts`].

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;

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

/// Семейство и начертание.
type WebKey = (String, WebVariant);

pub struct Fonts {
    store: FontStore,
    /// Шрифты для браузера: план частей и уже сжатые части.
    web: Mutex<HashMap<WebKey, Option<Arc<WebFace>>>>,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts").field("families", &self.store.book().families().count()).finish_non_exhaustive()
    }
}

/// Начертание для `@font-face`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Шрифт одного начертания для браузера: части по наборам знаков. Части
/// сжимаются при первом запросе (Gentium — ~0,1 с на часть, математический
/// шрифт целиком — ~2 с) и дальше отдаются из памяти.
pub struct WebFace {
    data: Bytes,
    pub chunks: Vec<crate::webfonts::Chunk>,
    files: Vec<OnceLock<Option<Arc<[u8]>>>>,
}

impl std::fmt::Debug for WebFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebFace").field("chunks", &self.chunks.len()).finish_non_exhaustive()
    }
}

impl WebFace {
    fn new(data: Bytes) -> Self {
        let chunks = crate::webfonts::plan(&data);
        let files = chunks.iter().map(|_| OnceLock::new()).collect();
        Self { data, chunks, files }
    }

    /// WOFF2 части `name` (`latin`, `cyrillic`, …, `all`).
    pub fn file(&self, name: &str) -> Option<Arc<[u8]>> {
        let i = self.chunks.iter().position(|c| c.name == name)?;
        self.files[i]
            .get_or_init(|| {
                let started = std::time::Instant::now();
                let file = crate::webfonts::chunk_woff2(&self.data, &self.chunks[i]).map(Arc::from);
                tracing::debug!(
                    chunk = name,
                    bytes = file.as_ref().map(|f: &Arc<[u8]>| f.len()),
                    ms = started.elapsed().as_millis(),
                    "шрифт для браузера"
                );
                file
            })
            .clone()
    }
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
        Self { store, web: Mutex::new(HashMap::new()) }
    }

    /// Шрифт семейства `family` этого начертания для браузера (см. [`Self::web_font`]
    /// — когда его нет). План частей строится один раз.
    pub fn web_face(&self, family: &str, variant: WebVariant) -> Option<Arc<WebFace>> {
        // Под замком целиком: сервер при запуске сжимает части в фоне, и
        // запрос страницы должен попасть в тот же кэш, а не начать заново.
        let mut web = self.web.lock();
        web.entry((family.to_owned(), variant))
            .or_insert_with(|| self.web_font(family, variant).map(|data| Arc::new(WebFace::new(data))))
            .clone()
    }

    /// Сжать все части шрифтов `families` заранее (в фоне при запуске
    /// сервера: первая страница не ждёт сжатия).
    pub fn warm_web(&self, families: &[&str]) {
        for family in families {
            for v in WebVariant::ALL {
                if let Some(face) = self.web_face(family, v) {
                    for c in &face.chunks {
                        face.file(&c.name);
                    }
                }
            }
        }
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
    fn web_font(&self, family: &str, variant: WebVariant) -> Option<Bytes> {
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
        if data.get(..4)? == b"ttcf" {
            return None;
        }
        Some(data)
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
                let face = fonts.web_face(family, v).unwrap_or_else(|| panic!("{family} {}", v.slug()));
                let names: Vec<_> = face.chunks.iter().map(|c| c.name.as_str()).collect();
                assert_eq!(&names[..2], ["latin", "cyrillic"], "{family} {}: по наборам знаков", v.slug());
            }
        }
        // Математический шрифт не режется (нет подмножеств для MATH и CFF).
        let math = fonts.web_face("New Computer Modern Math", WebVariant::ALL[0]).unwrap();
        assert_eq!(math.chunks.len(), 1);
        assert!(
            Arc::ptr_eq(&math, &fonts.web_face("New Computer Modern Math", WebVariant::ALL[0]).unwrap()),
            "план — один раз"
        );
    }
}
