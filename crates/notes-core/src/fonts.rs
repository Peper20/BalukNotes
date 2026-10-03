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
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;

use rust_embed::RustEmbed;
use typst::foundations::Bytes;
use typst::text::{Font, FontBook, FontInfo, FontStyle, FontVariant, FontWeight};
use typst::utils::LazyHash;
use typst_kit::fonts::{self, FontPath, FontStore};

use crate::themes::WebFamily;

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
    /// Кэш сжатых частей на диске (`<данные>/cache/fonts`): сервер не
    /// сжимает их при каждом запуске заново.
    web_cache: Option<PathBuf>,
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
/// шрифт целиком — ~2 с) и дальше отдаются из памяти; с кэшем на диске —
/// берутся оттуда (имя файла — хэш шрифта и части, [`crate::webfonts::chunk_key`]).
pub struct WebFace {
    data: Bytes,
    pub chunks: Vec<crate::webfonts::Chunk>,
    files: Vec<OnceLock<Option<Arc<[u8]>>>>,
    /// Каталог кэша и хэш файла шрифта.
    cache: Option<(PathBuf, u64)>,
}

impl std::fmt::Debug for WebFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebFace").field("chunks", &self.chunks.len()).finish_non_exhaustive()
    }
}

impl WebFace {
    /// Текстовый шрифт CFF — сначала в TrueType ([`crate::webfonts::cff_to_truetype`]);
    /// шрифт формул — как есть. Метка кэша — по исходному файлу.
    fn new(data: Bytes, math: bool, cache: Option<&PathBuf>) -> Self {
        let cache = cache.map(|dir| (dir.clone(), crate::version::StableHasher::new().bytes(&data).finish()));
        let started = std::time::Instant::now();
        let data = match (!math).then(|| crate::webfonts::cff_to_truetype(&data)).flatten() {
            Some(truetype) => {
                tracing::debug!(ms = started.elapsed().as_millis(), bytes = truetype.len(), "шрифт CFF -> TrueType");
                Bytes::new(truetype)
            }
            None => data,
        };
        let chunks = crate::webfonts::plan(&data);
        let files = chunks.iter().map(|_| OnceLock::new()).collect();
        Self { data, chunks, files, cache }
    }

    /// Файл части в кэше на диске.
    fn cached_path(&self, i: usize) -> Option<PathBuf> {
        let (dir, hash) = self.cache.as_ref()?;
        Some(dir.join(format!("{}.woff2", crate::webfonts::chunk_key(*hash, &self.chunks[i]))))
    }

    /// WOFF2 части `name` (`latin`, `cyrillic`, …, `all`).
    pub fn file(&self, name: &str) -> Option<Arc<[u8]>> {
        let i = self.chunks.iter().position(|c| c.name == name)?;
        self.files[i]
            .get_or_init(|| {
                let cached = self.cached_path(i);
                if let Some(data) = cached.as_ref().and_then(|p| fs::read(p).ok()).filter(|d| d.starts_with(b"wOF2")) {
                    return Some(Arc::from(data));
                }
                let started = std::time::Instant::now();
                let file = crate::webfonts::chunk_woff2(&self.data, &self.chunks[i]).map(Arc::from);
                if let (Some(path), Some(data)) = (&cached, &file)
                    && let Err(e) = crate::fsutil::write_atomic(path, data)
                {
                    tracing::warn!("кэш шрифтов: {e}");
                }
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
        Self { store, web: Mutex::new(HashMap::new()), web_cache: None }
    }

    /// Хранить сжатые части шрифтов для браузера в каталоге `dir`.
    #[must_use]
    pub fn with_web_cache(mut self, dir: Option<PathBuf>) -> Self {
        self.web_cache = dir;
        self
    }

    /// Шрифт семейства `family` этого начертания для браузера (см. [`Self::web_font`]
    /// — когда его нет). План частей строится один раз.
    pub fn web_face(&self, family: &WebFamily, variant: WebVariant) -> Option<Arc<WebFace>> {
        // Под замком целиком: сервер при запуске сжимает части в фоне, и
        // запрос страницы должен попасть в тот же кэш, а не начать заново.
        let mut web = self.web.lock();
        web.entry((family.name.clone(), variant))
            .or_insert_with(|| {
                let data = self.web_font(&family.name, variant)?;
                Some(Arc::new(WebFace::new(data, family.math, self.web_cache.as_ref())))
            })
            .clone()
    }

    /// `@font-face` на каждую часть шрифтов `families` (браузер качает только
    /// части со знаками страницы — `unicode-range`). `base` — путь к файлам
    /// (`/fonts/`): файл части — `{base}{семейство}/{начертание}/{часть}.woff2`.
    pub fn font_faces(&self, families: &[WebFamily], base: &str) -> String {
        let mut out = String::from("/* Шрифты оформления: те же файлы, что у Typst, в WOFF2 и по наборам знаков. */\n");
        for web in families {
            let family = web.name.as_str();
            for v in WebVariant::ALL {
                let Some(face) = self.web_face(web, v) else { continue };
                for chunk in &face.chunks {
                    let url = format!("{base}{}/{}/{}.woff2", family.replace(' ', "%20"), v.slug(), chunk.name);
                    let range =
                        chunk.unicode_range.as_ref().map(|r| format!(" unicode-range: {r};")).unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "@font-face {{ font-family: \"{family}\"; src: url(\"{url}\") format(\"woff2\"); font-style: {}; font-weight: {}; font-display: swap;{range} }}",
                        if v.italic { "italic" } else { "normal" },
                        if v.bold { 700 } else { 400 },
                    );
                }
            }
        }
        out
    }

    /// Сжать все части шрифтов `families` заранее (в фоне при запуске
    /// сервера: первая страница не ждёт сжатия).
    pub fn warm_web(&self, families: &[WebFamily]) {
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

    /// Чистка кэша частей на диске: файлы, которых нет в плане частей
    /// шрифтов `families` (старый шрифт, прежний кодировщик), — если они
    /// старше `max_age`: сборка приложения, делящая каталог данных (отладочная
    /// и релизная), пользуется своими частями. Сколько удалено.
    pub fn prune_web(&self, families: &[WebFamily], max_age: std::time::Duration) -> usize {
        let Some(dir) = &self.web_cache else { return 0 };
        let mut keep = HashSet::new();
        for family in families {
            for v in WebVariant::ALL {
                let Some(face) = self.web_face(family, v) else { continue };
                keep.extend((0..face.chunks.len()).filter_map(|i| face.cached_path(i)));
            }
        }
        let now = std::time::SystemTime::now();
        let mut removed = 0;
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| now.duration_since(t).is_ok_and(|age| age > max_age));
            if path.extension().is_some_and(|e| e == "woff2") && !keep.contains(&path) && old {
                match fs::remove_file(&path) {
                    Ok(()) => removed += 1,
                    Err(e) => tracing::warn!("кэш шрифтов {}: не удалён: {e}", path.display()),
                }
            }
        }
        removed
    }

    pub fn book(&self) -> &LazyHash<FontBook> {
        self.store.book()
    }

    /// Отпечаток набора шрифтов (семейства и число начертаний) — для метки
    /// кэша на диске: поставили или убрали шрифт — отрисовка могла
    /// измениться.
    pub fn fingerprint(&self) -> u64 {
        let mut h = crate::version::StableHasher::new();
        for (family, infos) in self.store.book().families() {
            h.str(family).u64(infos.count() as u64);
        }
        h.finish()
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

    fn text(name: &str) -> WebFamily {
        WebFamily { name: name.into(), math: false }
    }

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
                let face = fonts.web_face(&text(family), v).unwrap_or_else(|| panic!("{family} {}", v.slug()));
                let names: Vec<_> = face.chunks.iter().map(|c| c.name.as_str()).collect();
                assert_eq!(&names[..2], ["latin", "cyrillic"], "{family} {}: по наборам знаков", v.slug());
            }
        }
        // Запасные шрифты тем — тоже частями; текстовый CFF (New Computer
        // Modern) — переведённый в TrueType.
        for family in ["DejaVu Sans Mono", "New Computer Modern"] {
            for v in WebVariant::ALL {
                let face = fonts.web_face(&text(family), v).unwrap_or_else(|| panic!("{family} {}", v.slug()));
                assert_eq!(face.chunks[0].name, "latin", "{family} {}", v.slug());
            }
        }
        let serif = fonts.web_face(&text("New Computer Modern"), WebVariant::ALL[0]).unwrap();
        let rest = serif.chunks.iter().find(|c| c.name == "rest").unwrap();
        assert!(rest.unicode_range.as_deref().unwrap().contains("U+2103"), "℃ — в части rest");
        assert_eq!(&serif.file("rest").unwrap()[4..8], [0, 1, 0, 0], "TrueType");
        // Шрифт формул не режется и не перестраивается (CFF + MATH), но
        // объявлен только для своих знаков.
        let math = WebFamily { name: "New Computer Modern Math".into(), math: true };
        let face = fonts.web_face(&math, WebVariant::ALL[0]).unwrap();
        assert_eq!(face.chunks.len(), 1);
        assert!(face.chunks[0].unicode_range.as_deref().unwrap().starts_with("U+20"));
        assert_eq!(&face.file("all").unwrap()[4..8], b"OTTO");
        assert!(Arc::ptr_eq(&face, &fonts.web_face(&math, WebVariant::ALL[0]).unwrap()), "план — один раз");
    }

    #[test]
    fn web_chunks_are_cached_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let first = Fonts::load(&[]).with_web_cache(Some(dir.path().to_path_buf()));
        let face = first.web_face(&text("JetBrains Mono"), WebVariant::ALL[0]).unwrap();
        let latin = face.file("latin").unwrap();
        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(files.len(), 1);
        // Подменённый файл кэша читается как есть — значит, не сжимается заново.
        let mut marked = fs::read(&files[0]).unwrap();
        assert_eq!(&marked[..], &latin[..]);
        marked.extend_from_slice(b"from-cache");
        fs::write(&files[0], &marked).unwrap();
        let second = Fonts::load(&[]).with_web_cache(Some(dir.path().to_path_buf()));
        let again = second.web_face(&text("JetBrains Mono"), WebVariant::ALL[0]).unwrap().file("latin").unwrap();
        assert!(again.ends_with(b"from-cache"));
        // Испорченный (не WOFF2) — сжимается заново.
        fs::write(&files[0], b"junk").unwrap();
        let third = Fonts::load(&[]).with_web_cache(Some(dir.path().to_path_buf()));
        assert_eq!(
            &third.web_face(&text("JetBrains Mono"), WebVariant::ALL[0]).unwrap().file("latin").unwrap()[..],
            &latin[..]
        );
    }

    #[test]
    fn prune_keeps_current_and_recent_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let fonts = Fonts::load(&[]).with_web_cache(Some(dir.path().to_path_buf()));
        let face = fonts.web_face(&text("JetBrains Mono"), WebVariant::ALL[0]).unwrap();
        face.file("latin").unwrap();
        let current = face.cached_path(0).unwrap();
        let day = std::time::Duration::from_hours(24);
        let old = std::time::SystemTime::now() - 30 * day;
        for name in ["старая.woff2", "чужая-свежая.woff2"] {
            fs::write(dir.path().join(name), b"wOF2").unwrap();
        }
        fs::File::options().write(true).open(dir.path().join("старая.woff2")).unwrap().set_modified(old).unwrap();
        fs::File::options().write(true).open(&current).unwrap().set_modified(old).unwrap();

        assert_eq!(fonts.prune_web(&[text("JetBrains Mono")], 14 * day), 1);
        assert!(current.exists(), "часть нынешнего шрифта — даже старая");
        assert!(dir.path().join("чужая-свежая.woff2").exists(), "чужая — пока свежая");
        assert!(!dir.path().join("старая.woff2").exists());
    }
}
