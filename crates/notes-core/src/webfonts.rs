//! Шрифты для браузера: WOFF2 и части по наборам символов.
//!
//! Файлы шрифтов оформления большие (Gentium Plus — ~0,9 МБ на начертание:
//! латиница, кириллица, греческий, МФА…), а странице обычно нужны латиница и
//! кириллица. Поэтому шрифт режется на части (`latin`, `cyrillic`, …) — у
//! каждой свой `@font-face` с `unicode-range`, и браузер качает только те
//! части, чьи знаки есть на странице. Каждая часть — WOFF2 (brotli).
//!
//! Комбинируемые знаки (U+0300–036F, например ударение U+0301) входят в
//! каждую часть: иначе знак и буква попали бы в разные шрифты, и знак встал
//! бы мимо буквы.
//!
//! Режутся только шрифты TrueType (`glyf`) без таблицы `MATH`: подмножество
//! строит `fontcull` (порт hb-subset), а он не умеет CFF и `MATH`. Такие
//! шрифты (New Computer Modern Math) отдаются целиком — но тоже в WOFF2.
//!
//! Кодировщик WOFF2 здесь свой и простой: таблицы без преобразований (так
//! можно — версия преобразования «null»), общий поток brotli. Преобразование
//! `glyf` дало бы ещё ~10 %, но для него нужен разбор контуров.

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;

/// Части шрифта: имя и диапазоны. Знак попадает в первую подходящую часть;
/// что не подошло никуда — в часть `rest`. Наборы — как у Google Fonts.
const CHUNKS: &[(&str, &[(u32, u32)])] = &[
    (
        "latin",
        &[
            (0x0000, 0x00FF),
            (0x0131, 0x0131),
            (0x0152, 0x0153),
            (0x02BB, 0x02BC),
            (0x02C6, 0x02C6),
            (0x02DA, 0x02DA),
            (0x02DC, 0x02DC),
            (0x2000, 0x206F),
            (0x20AC, 0x20AC),
            (0x2122, 0x2122),
            (0x2190, 0x2193),
            (0x2212, 0x2215),
            (0xFEFF, 0xFEFF),
            (0xFFFD, 0xFFFD),
        ],
    ),
    ("cyrillic", &[(0x0400, 0x045F), (0x0490, 0x0491), (0x04B0, 0x04B1), (0x2116, 0x2116)]),
    ("cyrillic-ext", &[(0x0460, 0x052F), (0x1C80, 0x1C8F), (0x2DE0, 0x2DFF), (0xA640, 0xA69F)]),
    ("greek", &[(0x0370, 0x03FF), (0x1F00, 0x1FFF)]),
    ("latin-ext", &[(0x0100, 0x02FF), (0x1D00, 0x1DBF), (0x1E00, 0x1EFF), (0x2C60, 0x2C7F), (0xA720, 0xA7FF)]),
];

/// Комбинируемые знаки — в каждой части.
const MARKS: (u32, u32) = (0x0300, 0x036F);

/// Часть шрифта для браузера.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Имя в URL и в имени файла: `latin`, `cyrillic`, …; `all` — шрифт целиком.
    pub name: String,
    /// Значение `unicode-range` для `@font-face`; у целого шрифта — нет.
    pub unicode_range: Option<String>,
    /// Знаки части (с комбинируемыми); у целого шрифта — пусто.
    codepoints: Vec<u32>,
}

/// Как отдавать шрифт: части по наборам знаков или (если резать нельзя или
/// незачем) один файл `all`.
pub fn plan(font: &[u8]) -> Vec<Chunk> {
    let whole = || vec![Chunk { name: "all".into(), unicode_range: None, codepoints: Vec::new() }];
    let Some(tables) = table_records(font) else { return whole() };
    let has = |tag: &[u8; 4]| tables.iter().any(|t| &t.tag == tag);
    if !has(b"glyf") || has(b"MATH") {
        return whole();
    }
    let Ok(face) = ttf_parser::Face::parse(font, 0) else { return whole() };
    let mut all = BTreeSet::new();
    if let Some(cmap) = face.tables().cmap {
        for sub in cmap.subtables.into_iter().filter(ttf_parser::cmap::Subtable::is_unicode) {
            sub.codepoints(|c| {
                all.insert(c);
            });
        }
    }
    let marks: Vec<u32> = all.range(MARKS.0..=MARKS.1).copied().collect();
    let mut left: BTreeSet<u32> = all.iter().copied().filter(|c| !(MARKS.0..=MARKS.1).contains(c)).collect();
    let mut out = Vec::new();
    let mut push = |name: &str, own: Vec<u32>| {
        if own.is_empty() {
            return;
        }
        let mut codepoints = own;
        codepoints.extend(&marks);
        codepoints.sort_unstable();
        codepoints.dedup();
        out.push(Chunk { name: name.into(), unicode_range: Some(css_ranges(&codepoints)), codepoints });
    };
    for (name, ranges) in CHUNKS {
        let own: Vec<u32> = left.iter().copied().filter(|c| ranges.iter().any(|(a, b)| (a..=b).contains(&c))).collect();
        for c in &own {
            left.remove(c);
        }
        push(name, own);
    }
    push("rest", left.into_iter().collect());
    if out.len() < 2 {
        return whole();
    }
    out
}

/// Файл части: подмножество шрифта в WOFF2. `None` — не вышло (тогда
/// отдавать шрифт целиком).
pub fn chunk_woff2(font: &[u8], chunk: &Chunk) -> Option<Vec<u8>> {
    if chunk.codepoints.is_empty() {
        return woff2(font);
    }
    let chars: HashSet<char> = chunk.codepoints.iter().filter_map(|&c| char::from_u32(c)).collect();
    let subset = fontcull::subset_font_data(font, &chars, &[]).ok()?;
    woff2(&subset)
}

/// `U+0-FF, U+131, U+400-45F` — диапазоны подряд идущих знаков.
fn css_ranges(sorted: &[u32]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            i += 1;
            end = sorted[i];
        }
        if !out.is_empty() {
            out.push_str(", ");
        }
        if start == end {
            let _ = write!(out, "U+{start:X}");
        } else {
            let _ = write!(out, "U+{start:X}-{end:X}");
        }
        i += 1;
    }
    out
}

// ── WOFF2 ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct TableRecord {
    tag: [u8; 4],
    offset: usize,
    length: usize,
}

fn be_u16(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn be_u32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

/// Таблицы sfnt (TrueType или CFF); коллекции (`ttcf`) — нет.
fn table_records(font: &[u8]) -> Option<Vec<TableRecord>> {
    let flavor = be_u32(font, 0)?;
    if flavor != 0x0001_0000 && &font[..4] != b"OTTO" && &font[..4] != b"true" {
        return None;
    }
    let count = usize::from(be_u16(font, 4)?);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = 12 + 16 * i;
        let tag: [u8; 4] = font.get(at..at + 4)?.try_into().ok()?;
        let offset = be_u32(font, at + 8)? as usize;
        let length = be_u32(font, at + 12)? as usize;
        font.get(offset..offset.checked_add(length)?)?;
        out.push(TableRecord { tag, offset, length });
    }
    Some(out)
}

/// Теги с номером в WOFF2 (спецификация, 5.1): номер вместо четырёх байт.
const KNOWN_TAGS: [&[u8; 4]; 63] = [
    b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post", b"cvt ", b"fpgm", b"glyf", b"loca",
    b"prep", b"CFF ", b"VORG", b"EBDT", b"EBLC", b"gasp", b"hdmx", b"kern", b"LTSH", b"PCLT", b"VDMX", b"vhea",
    b"vmtx", b"BASE", b"GDEF", b"GPOS", b"GSUB", b"EBSC", b"JSTF", b"MATH", b"CBDT", b"CBLC", b"COLR", b"CPAL",
    b"SVG ", b"sbix", b"acnt", b"avar", b"bdat", b"bloc", b"bsln", b"cvar", b"fdsc", b"feat", b"fmtx", b"fvar",
    b"gvar", b"hsty", b"just", b"lcar", b"mort", b"morx", b"opbd", b"prop", b"trak", b"Zapf", b"Silf", b"Glat",
    b"Gloc", b"Feat", b"Sill",
];

/// UIntBase128: по 7 бит, старшие вперёд, без ведущих нулей.
fn base128(out: &mut Vec<u8>, mut v: u32) {
    let mut bytes = [0u8; 5];
    let mut n = 0;
    loop {
        bytes[n] = (v & 0x7F) as u8;
        n += 1;
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    for i in (0..n).rev() {
        out.push(bytes[i] | if i > 0 { 0x80 } else { 0 });
    }
}

/// Шрифт TrueType/CFF → WOFF2 без преобразований таблиц. `None` — не sfnt.
pub fn woff2(font: &[u8]) -> Option<Vec<u8>> {
    let mut tables = table_records(font)?;
    // Порядок в каталоге = порядок в потоке; `loca` — после `glyf` (по тегам так и есть).
    tables.sort_by_key(|t| t.tag);
    let num_tables = u16::try_from(tables.len()).ok()?;

    let mut stream = Vec::with_capacity(font.len());
    let mut sfnt_size = 12 + 16 * tables.len();
    let mut directory = Vec::new();
    for t in &tables {
        let null_transform = if &t.tag == b"glyf" || &t.tag == b"loca" { 3 << 6 } else { 0 };
        if let Some(i) = KNOWN_TAGS.iter().position(|k| **k == t.tag) {
            directory.push(u8::try_from(i).ok()? | null_transform);
        } else {
            directory.push(63 | null_transform);
            directory.extend_from_slice(&t.tag);
        }
        base128(&mut directory, u32::try_from(t.length).ok()?);
        stream.extend_from_slice(&font[t.offset..t.offset + t.length]);
        sfnt_size += t.length.next_multiple_of(4);
    }

    let mut compressed = Vec::new();
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        lgwin: 22,
        mode: brotli::enc::backward_references::BrotliEncoderMode::BROTLI_MODE_FONT,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut stream.as_slice(), &mut compressed, &params).ok()?;

    let length = (48 + directory.len() + compressed.len()).next_multiple_of(4);
    let mut out = Vec::with_capacity(length);
    out.extend_from_slice(b"wOF2");
    out.extend_from_slice(&font[..4]); // flavor
    out.extend_from_slice(&u32::try_from(length).ok()?.to_be_bytes());
    out.extend_from_slice(&num_tables.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(&u32::try_from(sfnt_size).ok()?.to_be_bytes());
    out.extend_from_slice(&u32::try_from(compressed.len()).ok()?.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    out.extend_from_slice(&[0; 20]); // метаданных и частных данных нет
    out.extend_from_slice(&directory);
    out.extend_from_slice(&compressed);
    out.resize(length, 0);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENTIUM: &[u8] = include_bytes!("../../../fonts/GentiumPlus-Regular.ttf");

    /// Обратно из WOFF2 (только без преобразований): тег → данные таблицы.
    fn decode(w: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(&w[..4], b"wOF2");
        assert_eq!(be_u32(w, 8).unwrap() as usize, w.len());
        assert_eq!(w.len() % 4, 0);
        let n = usize::from(be_u16(w, 12).unwrap());
        let compressed_len = be_u32(w, 20).unwrap() as usize;
        let mut at = 48;
        let mut entries = Vec::new();
        for _ in 0..n {
            let flags = w[at];
            at += 1;
            let tag = if flags & 63 == 63 {
                at += 4;
                w[at - 4..at].try_into().unwrap()
            } else {
                *KNOWN_TAGS[usize::from(flags & 63)]
            };
            let mut len = 0u32;
            loop {
                let b = w[at];
                at += 1;
                len = (len << 7) | u32::from(b & 0x7F);
                if b & 0x80 == 0 {
                    break;
                }
            }
            let transform = flags >> 6;
            assert_eq!(transform, if &tag == b"glyf" || &tag == b"loca" { 3 } else { 0 }, "без преобразований");
            entries.push((tag, len as usize));
        }
        let mut stream = Vec::new();
        brotli::BrotliDecompress(&mut &w[at..at + compressed_len], &mut stream).unwrap();
        let mut pos = 0;
        entries
            .into_iter()
            .map(|(tag, len)| {
                pos += len;
                (tag, stream[pos - len..pos].to_vec())
            })
            .collect()
    }

    #[test]
    fn woff2_keeps_every_table() {
        let w = woff2(GENTIUM).unwrap();
        assert!(w.len() < GENTIUM.len() / 2, "сжатие: {} из {}", w.len(), GENTIUM.len());
        let tables = decode(&w);
        let original = table_records(GENTIUM).unwrap();
        assert_eq!(tables.len(), original.len());
        for t in original {
            let (_, data) = tables.iter().find(|(tag, _)| *tag == t.tag).unwrap();
            assert_eq!(data.as_slice(), &GENTIUM[t.offset..t.offset + t.length]);
        }
    }

    #[test]
    fn base128_encoding() {
        let enc = |v| {
            let mut out = Vec::new();
            base128(&mut out, v);
            out
        };
        assert_eq!(enc(0), [0]);
        assert_eq!(enc(127), [127]);
        assert_eq!(enc(128), [0x81, 0]);
        assert_eq!(enc(63_000), [0x83, 0xEC, 0x18]);
    }

    #[test]
    fn ranges_css() {
        assert_eq!(css_ranges(&[0x20, 0x21, 0x22, 0x131, 0x400, 0x401]), "U+20-22, U+131, U+400-401");
    }

    #[test]
    fn gentium_splits_into_scripts() {
        let chunks = plan(GENTIUM);
        let names: Vec<_> = chunks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(&names[..3], ["latin", "cyrillic", "cyrillic-ext"]);
        let has = |name: &str, c: u32| chunks.iter().find(|x| x.name == name).unwrap().codepoints.contains(&c);
        assert!(has("latin", 'a' as u32) && has("latin", '«' as u32) && has("latin", '—' as u32));
        assert!(has("cyrillic", 'ё' as u32) && !has("latin", 'ё' as u32));
        // ударение — в каждой части
        assert!(chunks.iter().all(|c| c.codepoints.contains(&0x301)));
        assert!(chunks[1].unicode_range.as_deref().unwrap().contains("U+300"));

        let cyr = chunk_woff2(GENTIUM, &chunks[1]).unwrap();
        assert!(cyr.len() < 60_000, "кириллица: {} байт", cyr.len());
        assert!(decode(&cyr).iter().any(|(tag, _)| tag == b"GPOS"), "позиционирование знаков осталось");
    }

    #[test]
    fn math_and_cff_fonts_stay_whole() {
        let math = typst_assets_math();
        let chunks = plan(&math);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "all");
        let w = chunk_woff2(&math, &chunks[0]).unwrap();
        assert_eq!(&w[4..8], b"OTTO");
        assert_eq!(decode(&w).len(), table_records(&math).unwrap().len());
    }

    fn typst_assets_math() -> Vec<u8> {
        typst_assets::fonts()
            .find(|f| f.starts_with(b"OTTO") && f.windows(4).any(|w| w == b"MATH"))
            .expect("математический шрифт в typst-assets")
            .to_vec()
    }
}
