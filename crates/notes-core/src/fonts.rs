//! Fonts: for the compiler and for the browser.
//!
//! The styling fonts (Gentium Plus, JetBrains Mono - the `fonts/` directory)
//! and the Typst fonts (New Computer Modern Math etc.) are embedded in the
//! binary and **shadow** system fonts of the same name: rendering is the
//! same on any machine, and nothing has to be installed. System fonts are
//! for everything else.
//!
//! The browser draws text and formulas itself, so it needs the same font
//! files as Typst; [`Fonts::web_face`] serves them, in WOFF2 and in chunks
//! (character sets), see [`crate::webfonts`].

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

/// The styling fonts from `fonts/`. In a debug build rust-embed reads them
/// from disk.
#[derive(RustEmbed)]
#[folder = "../../fonts/"]
#[include = "*.ttf"]
#[include = "*.otf"]
struct EmbeddedFonts;

/// The styling fonts embedded in the binary.
fn bundled_fonts() -> impl Iterator<Item = (Font, FontInfo)> {
    EmbeddedFonts::iter().filter_map(|name| EmbeddedFonts::get(&name)).flat_map(|file| {
        Font::iter(Bytes::new(file.data.into_owned())).map(|font| {
            let info = font.info().clone();
            (font, info)
        })
    })
}

/// Family and variant.
type WebKey = (String, WebVariant);

/// Fonts for Typst and the browser.
pub struct Fonts {
    store: FontStore,
    /// Fonts for the browser: the chunk plan and the compressed chunks.
    web: Mutex<HashMap<WebKey, Option<Arc<WebFace>>>>,
    /// Disk cache of compressed chunks (`<data>/cache/fonts`): the server
    /// does not compress them anew on every start.
    web_cache: Option<PathBuf>,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts").field("families", &self.store.book().families().count()).finish_non_exhaustive()
    }
}

/// A variant for `@font-face`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WebVariant {
    /// Italic.
    pub italic: bool,
    /// Bold.
    pub bold: bool,
}

impl WebVariant {
    /// All four variants.
    pub const ALL: [Self; 4] = [
        Self { italic: false, bold: false },
        Self { italic: true, bold: false },
        Self { italic: false, bold: true },
        Self { italic: true, bold: true },
    ];

    /// Name in the URL: `regular`, `italic`, `bold`, `bold-italic`.
    pub fn slug(self) -> &'static str {
        match (self.bold, self.italic) {
            (false, false) => "regular",
            (false, true) => "italic",
            (true, false) => "bold",
            (true, true) => "bold-italic",
        }
    }

    /// The variant by its name in the URL.
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

/// A font of one variant for the browser: chunks by character sets. Chunks
/// are compressed on the first request (Gentium ~0.1 s per chunk, the whole
/// math font ~2 s) and then served from memory; with a disk cache they come
/// from there (the file name is a hash of the font and the chunk,
/// [`crate::webfonts::chunk_key`]).
pub struct WebFace {
    data: Bytes,
    /// Chunks by character sets.
    pub chunks: Vec<crate::webfonts::Chunk>,
    files: Vec<OnceLock<Option<Arc<[u8]>>>>,
    /// Cache directory and the hash of the font file.
    cache: Option<(PathBuf, u64)>,
}

impl std::fmt::Debug for WebFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebFace").field("chunks", &self.chunks.len()).finish_non_exhaustive()
    }
}

impl WebFace {
    /// A CFF text font is first converted to TrueType
    /// ([`crate::webfonts::cff_to_truetype`]); a math font stays as is. The
    /// cache key is by the original file.
    fn new(data: Bytes, math: bool, cache: Option<&PathBuf>) -> Self {
        let cache = cache.map(|dir| (dir.clone(), crate::version::StableHasher::new().bytes(&data).finish()));
        let started = std::time::Instant::now();
        let data = match (!math).then(|| crate::webfonts::cff_to_truetype(&data)).flatten() {
            Some(truetype) => {
                tracing::debug!(ms = started.elapsed().as_millis(), bytes = truetype.len(), "font CFF -> TrueType");
                Bytes::new(truetype)
            }
            None => data,
        };
        let chunks = crate::webfonts::plan(&data);
        let files = chunks.iter().map(|_| OnceLock::new()).collect();
        Self { data, chunks, files, cache }
    }

    /// File of a chunk in the disk cache.
    fn cached_path(&self, i: usize) -> Option<PathBuf> {
        let (dir, hash) = self.cache.as_ref()?;
        Some(dir.join(format!("{}.woff2", crate::webfonts::chunk_key(*hash, &self.chunks[i]))))
    }

    /// WOFF2 of the chunk `name` (`latin`, `cyrillic`, ..., `all`).
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
                    tracing::warn!("font cache: {e}");
                }
                tracing::debug!(
                    chunk = name,
                    bytes = file.as_ref().map(|f: &Arc<[u8]>| f.len()),
                    ms = started.elapsed().as_millis(),
                    "font for the browser"
                );
                file
            })
            .clone()
    }
}

impl Fonts {
    /// Fonts from `extra_dirs` (given explicitly, they beat all others), the
    /// embedded ones (styling and Typst) and the system ones, except families
    /// already among the embedded.
    pub fn load(extra_dirs: &[PathBuf]) -> Self {
        let mut store = FontStore::new();
        for dir in extra_dirs {
            store.extend(fonts::scan(dir));
        }
        let embedded: Vec<_> = bundled_fonts().chain(fonts::embedded()).collect();
        let shadowed: HashSet<String> = embedded.iter().map(|(_, info)| info.family.to_lowercase()).collect();
        store.extend(embedded);
        // Otherwise a system font of the same family could win the selection
        // (Typst prefers a variable file to a static one).
        store.extend(fonts::system().filter(|(_, info)| !shadowed.contains(&info.family.to_lowercase())));
        Self { store, web: Mutex::new(HashMap::new()), web_cache: None }
    }

    /// Keeps compressed font chunks for the browser in the directory `dir`.
    #[must_use]
    pub fn with_web_cache(mut self, dir: Option<PathBuf>) -> Self {
        self.web_cache = dir;
        self
    }

    /// The font of `family` in this variant for the browser (see
    /// [`Self::web_font`] for when there is none). The chunk plan is built once.
    pub fn web_face(&self, family: &WebFamily, variant: WebVariant) -> Option<Arc<WebFace>> {
        // Locked as a whole: the server compresses chunks in the background
        // on start, and a page request must hit the same cache, not start anew.
        let mut web = self.web.lock();
        web.entry((family.name.clone(), variant))
            .or_insert_with(|| {
                let data = self.web_font(&family.name, variant)?;
                Some(Arc::new(WebFace::new(data, family.math, self.web_cache.as_ref())))
            })
            .clone()
    }

    /// `@font-face` for every chunk of the fonts `families` (the browser
    /// downloads only the chunks with the characters of the page,
    /// `unicode-range`). `base` is the path to the files (`/fonts/`): a chunk
    /// file is `{base}{family}/{variant}/{chunk}.woff2`.
    pub fn font_faces(&self, families: &[WebFamily], base: &str) -> String {
        let mut out =
            String::from("/* Styling fonts: the same files as Typst uses, in WOFF2 and by character sets. */\n");
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

    /// Compresses all chunks of the fonts `families` in advance (in the
    /// background at server start: the first page does not wait).
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

    /// Cleans the disk cache of chunks: files not in the chunk plan of the
    /// fonts `families` (an old font, a previous encoder), if they are older
    /// than `max_age` - an app build sharing the data directory (debug and
    /// release) uses its own chunks. Returns how many were removed.
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
                    Err(e) => tracing::warn!("font cache {}: not removed: {e}", path.display()),
                }
            }
        }
        removed
    }

    /// The font book for Typst.
    pub fn book(&self) -> &LazyHash<FontBook> {
        self.store.book()
    }

    /// Fingerprint of the font set (families and the number of variants) for
    /// the disk cache key: a font installed or removed may change rendering.
    pub fn fingerprint(&self) -> u64 {
        let mut h = crate::version::StableHasher::new();
        for (family, infos) in self.store.book().families() {
            h.str(family).u64(infos.count() as u64);
        }
        h.finish()
    }

    /// A font by its index in the book.
    pub fn font(&self, index: usize) -> Option<Font> {
        self.store.font(index)
    }

    /// The file of the `family` font in exactly this variant. `None` if there
    /// is no such variant (the browser synthesizes italic and bold itself, it
    /// should not get a regular file posing as bold) or the font is in a
    /// collection (`.ttc`) the browser cannot pick from.
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
            assert!(source.is::<Font>(), "{family}: embedded, not a system file");
        }
        for family in ["Gentium Plus", "JetBrains Mono"] {
            for v in WebVariant::ALL {
                let face = fonts.web_face(&text(family), v).unwrap_or_else(|| panic!("{family} {}", v.slug()));
                let names: Vec<_> = face.chunks.iter().map(|c| c.name.as_str()).collect();
                assert_eq!(&names[..2], ["latin", "cyrillic"], "{family} {}: by character sets", v.slug());
            }
        }
        // Fallback fonts of the themes are chunked too; the CFF text font
        // (New Computer Modern) is converted to TrueType.
        for family in ["DejaVu Sans Mono", "New Computer Modern"] {
            for v in WebVariant::ALL {
                let face = fonts.web_face(&text(family), v).unwrap_or_else(|| panic!("{family} {}", v.slug()));
                assert_eq!(face.chunks[0].name, "latin", "{family} {}", v.slug());
            }
        }
        let serif = fonts.web_face(&text("New Computer Modern"), WebVariant::ALL[0]).unwrap();
        let rest = serif.chunks.iter().find(|c| c.name == "rest").unwrap();
        assert!(rest.unicode_range.as_deref().unwrap().contains("U+2103"), "℃ is in the chunk rest");
        assert_eq!(&serif.file("rest").unwrap()[4..8], [0, 1, 0, 0], "TrueType");
        // The math font is not cut or rebuilt (CFF + MATH), but declared
        // only for its own characters.
        let math = WebFamily { name: "New Computer Modern Math".into(), math: true };
        let face = fonts.web_face(&math, WebVariant::ALL[0]).unwrap();
        assert_eq!(face.chunks.len(), 1);
        assert!(face.chunks[0].unicode_range.as_deref().unwrap().starts_with("U+20"));
        assert_eq!(&face.file("all").unwrap()[4..8], b"OTTO");
        assert!(Arc::ptr_eq(&face, &fonts.web_face(&math, WebVariant::ALL[0]).unwrap()), "the plan is built once");
    }

    #[test]
    fn web_chunks_are_cached_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let first = Fonts::load(&[]).with_web_cache(Some(dir.path().to_path_buf()));
        let face = first.web_face(&text("JetBrains Mono"), WebVariant::ALL[0]).unwrap();
        let latin = face.file("latin").unwrap();
        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(files.len(), 1);
        // A substituted cache file is read as is, so it is not compressed anew.
        let mut marked = fs::read(&files[0]).unwrap();
        assert_eq!(&marked[..], &latin[..]);
        marked.extend_from_slice(b"from-cache");
        fs::write(&files[0], &marked).unwrap();
        let second = Fonts::load(&[]).with_web_cache(Some(dir.path().to_path_buf()));
        let again = second.web_face(&text("JetBrains Mono"), WebVariant::ALL[0]).unwrap().file("latin").unwrap();
        assert!(again.ends_with(b"from-cache"));
        // A broken one (not WOFF2) is compressed anew.
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
        for name in ["old.woff2", "foreign-fresh.woff2"] {
            fs::write(dir.path().join(name), b"wOF2").unwrap();
        }
        fs::File::options().write(true).open(dir.path().join("old.woff2")).unwrap().set_modified(old).unwrap();
        fs::File::options().write(true).open(&current).unwrap().set_modified(old).unwrap();

        assert_eq!(fonts.prune_web(&[text("JetBrains Mono")], 14 * day), 1);
        assert!(current.exists(), "a chunk of the current font, even an old one");
        assert!(dir.path().join("foreign-fresh.woff2").exists(), "a foreign one, while fresh");
        assert!(!dir.path().join("old.woff2").exists());
    }
}
