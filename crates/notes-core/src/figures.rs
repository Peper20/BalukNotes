//! Рисунки на странице: меньше байт при том же виде.
//!
//! Работает с готовым HTML (после [`crate::render`]): SVG рисунков существует
//! только как текст, который пишет `typst-svg`. Разметка у него простая и
//! предсказуемая — тег за тегом, значения атрибутов в `"…"`, `<` и `>`
//! внутри значений экранированы, — поэтому хватает разбора по тегам.
//!
//! 1. **Общие глифы.** Каждый SVG несёт в `<defs>` копии глифов своих
//!    подписей; их id — хэш глифа, одинаковый во всех рисунках. Глифы
//!    переезжают в один скрытый `<svg class="k-glyphs">` в начале страницы.
//! 2. **Один SVG на все темы.** Варианты рисунка по темам (`div.k-frame-v`)
//!    обычно различаются только цветами. Тогда остаётся один SVG, а каждый
//!    различающийся цвет становится переменной: `style="fill: var(--kf3)"`,
//!    значения по темам — в `<style>` страницы. Если варианты различаются
//!    чем-то ещё (другая геометрия, градиенты) — остаются варианты.
//! 3. **Точность.** Typst пишет координаты с 9 знаками после запятой
//!    (в пунктах). Округление до сотых — ошибка 0,005 pt, глазу не видна.
//!    Координаты путей в SVG относительные, поэтому округляются абсолютные
//!    точки, а разности считаются из округлённых: ошибка не копится вдоль
//!    кривой.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::version::StableHasher;

/// Настройки обработки рисунков.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct FigureOptions {
    /// Знаков после запятой в координатах; `None` — как написал Typst.
    pub precision: Option<u8>,
}

impl Default for FigureOptions {
    fn default() -> Self {
        Self { precision: Some(2) }
    }
}

impl FigureOptions {
    /// Короткая метка для версии страницы: `p2`, `full`.
    pub fn key(&self) -> String {
        self.precision.map_or_else(|| "full".into(), |p| format!("p{p}"))
    }
}

/// Атрибут тега (по номеру) и его значения по темам.
type ColorDiff = (usize, Vec<String>);

/// Результат обработки страницы.
#[derive(Debug)]
pub struct Optimized {
    pub body: String,
    /// `<style>` со значениями цветовых переменных по темам (или пусто).
    pub styles: String,
    pub stats: Stats,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Stats {
    pub figures: usize,
    /// Рисунков, у которых темы слились в один SVG.
    pub merged: usize,
    pub glyphs: usize,
    pub colors: usize,
}

/// Атрибуты с цветом: только они могут различаться между темами.
const COLOR_ATTRS: &[&str] = &["fill", "stroke", "stop-color"];

/// Атрибуты с координатами и размерами, которые округляются. Цвета
/// (`oklab(95.1% -0.003 …)`), `offset` градиентов и id не трогаем.
const ROUND_ATTRS: &[&str] = &[
    "x",
    "y",
    "width",
    "height",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "fr",
    "fx",
    "fy",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
];

/// Коэффициенты поворота и масштаба в `matrix(…)`/`scale(…)` округляются не
/// грубее: ошибка в них умножается на размер рисунка.
const SCALE_DIGITS: u8 = 4;
/// Глифы — мелкие фигуры, им нужно не меньше сотых.
const GLYPH_DIGITS: u8 = 2;

const VARIANT_OPEN: &str = r#"<div class="k-frame-v" data-theme=""#;

/// Обрабатывает рисунки в `<body>` страницы. `themes` — темы в порядке
/// вариантов (первая — базовая), `scope` — значение `data-k-figs`, под
/// которым живут цветовые переменные этой страницы.
pub fn optimize(body: &str, themes: &[String], opts: FigureOptions) -> Optimized {
    let mut page = Page { opts, glyphs: Vec::new(), glyph_ids: HashMap::new(), colors: HashMap::new() };
    let mut stats = Stats::default();
    let mut out = String::with_capacity(body.len() / 2);
    let mut rest = body;
    while let Some(start) = rest.find(VARIANT_OPEN) {
        out.push_str(&rest[..start]);
        let (variants, len) = read_variants(&rest[start..]);
        if len == 0 {
            // Вариант без `</svg></div>` (пустой рисунок) — не наш, оставляем как есть.
            out.push_str(VARIANT_OPEN);
            rest = &rest[start + VARIANT_OPEN.len()..];
            continue;
        }
        let group = &rest[start..start + len];
        rest = &rest[start + len..];
        stats.figures += 1;
        let names: Vec<&str> = variants.iter().map(|v| v.0).collect();
        if names != themes.iter().map(String::as_str).collect::<Vec<_>>() {
            // Не та раскладка тем — не трогаем.
            out.push_str(group);
            continue;
        }
        let svgs: Vec<Vec<&str>> = variants.iter().map(|v| page.hoist_glyphs(tokens(v.1))).collect();
        if let Some(merged) = page.merge(&svgs) {
            stats.merged += 1;
            out.push_str(&merged);
        } else {
            for ((theme, _), svg) in variants.iter().zip(&svgs) {
                let _ = write!(out, r#"{VARIANT_OPEN}{theme}">{}</div>"#, page.emit(svg));
            }
        }
    }
    out.push_str(rest);

    stats.glyphs = page.glyphs.len();
    stats.colors = page.colors.len();
    let mut sprite = String::new();
    if !page.glyphs.is_empty() {
        sprite.push_str(
            r#"<svg class="k-glyphs" aria-hidden="true" width="0" height="0" style="position: absolute" xmlns="http://www.w3.org/2000/svg"><defs>"#,
        );
        for g in &page.glyphs {
            sprite.push_str(g);
        }
        sprite.push_str("</defs></svg>");
    }

    let (body, styles) = if page.colors.is_empty() {
        (sprite + &out, String::new())
    } else {
        let scope = page.scope();
        (sprite + &scoped(&out, &scope), page.css(themes, &scope))
    };
    Optimized { body, styles, stats }
}

/// Варианты одного рисунка, идущие подряд: `[(тема, svg)]` и их общая длина.
fn read_variants(s: &str) -> (Vec<(&str, &str)>, usize) {
    let mut out = Vec::new();
    let mut pos = 0;
    while s[pos..].starts_with(VARIANT_OPEN) {
        let theme_start = pos + VARIANT_OPEN.len();
        let Some(theme_len) = s[theme_start..].find("\">") else { break };
        let svg_start = theme_start + theme_len + 2;
        let Some(svg_len) = s[svg_start..].find("</svg></div>") else { break };
        let svg_end = svg_start + svg_len + "</svg>".len();
        out.push((&s[theme_start..theme_start + theme_len], &s[svg_start..svg_end]));
        pos = svg_end + "</div>".len();
    }
    (out, pos)
}

/// SVG → теги и текст между ними.
pub(crate) fn tokens(svg: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = svg;
    while !rest.is_empty() {
        let end = if rest.starts_with('<') {
            rest.find('>').map_or(rest.len(), |i| i + 1)
        } else {
            rest.find('<').unwrap_or(rest.len())
        };
        out.push(&rest[..end]);
        rest = &rest[end..];
    }
    out
}

struct Page {
    opts: FigureOptions,
    /// Глифы по порядку появления (уже округлённые).
    glyphs: Vec<String>,
    /// id глифа → исходный текст `<symbol>…</symbol>`.
    glyph_ids: HashMap<String, String>,
    /// Значения цвета по темам → номер переменной.
    colors: HashMap<Vec<String>, usize>,
}

impl Page {
    /// Забирает глифы из `<defs>` в общий набор. `<defs>`, где есть что-то
    /// кроме глифов, или глиф с тем же id, но другим содержимым — остаются.
    fn hoist_glyphs<'a>(&mut self, toks: Vec<&'a str>) -> Vec<&'a str> {
        let Some(open) = toks.iter().position(|t| *t == "<defs>") else { return toks };
        let Some(len) = toks[open..].iter().position(|t| *t == "</defs>") else { return toks };
        let close = open + len;
        let mut symbols = Vec::new();
        let mut i = open + 1;
        while i < close {
            if !toks[i].starts_with("<symbol ") {
                return toks;
            }
            let Some(len) = toks[i..close].iter().position(|t| *t == "</symbol>") else { return toks };
            let Some(id) = attr(toks[i], "id") else { return toks };
            symbols.push((id, toks[i..=i + len].concat()));
            i += len + 1;
        }
        if symbols.iter().any(|(id, text)| self.glyph_ids.get(id.as_ref()).is_some_and(|known| known != text)) {
            return toks;
        }
        for (id, text) in symbols {
            if !self.glyph_ids.contains_key(id.as_ref()) {
                let digits = self.opts.precision.map(|p| p.max(GLYPH_DIGITS));
                let rounded = tokens(&text).iter().map(|t| round_tag(t, digits)).collect::<String>();
                self.glyphs.push(rounded);
                self.glyph_ids.insert(id.into_owned(), text);
            }
        }
        let mut out = toks;
        out.drain(open..=close);
        out
    }

    /// Один SVG вместо вариантов, если они различаются только цветами.
    fn merge(&mut self, svgs: &[Vec<&str>]) -> Option<String> {
        let base = &svgs[0];
        if svgs.iter().any(|s| s.len() != base.len()) {
            return None;
        }
        // Сначала — проверить весь рисунок, и только потом заводить
        // переменные: отвергнутый рисунок не должен оставить их в таблице.
        let mut plan: Vec<Option<(Tag, Vec<ColorDiff>)>> = Vec::with_capacity(base.len());
        for (i, tok) in base.iter().enumerate() {
            if svgs.iter().all(|s| s[i] == *tok) {
                plan.push(None);
                continue;
            }
            let tags: Option<Vec<Tag>> = svgs.iter().map(|s| Tag::parse(s[i])).collect();
            let tags = tags?;
            if tags.iter().any(|t| t.name != tags[0].name || t.attrs.len() != tags[0].attrs.len()) {
                return None;
            }
            let mut differing = Vec::new();
            for (a, (name, value)) in tags[0].attrs.iter().enumerate() {
                let values: Vec<&str> = tags.iter().map(|t| t.attrs[a].1.as_ref()).collect();
                if tags.iter().any(|t| t.attrs[a].0 != *name) {
                    return None;
                }
                if values.iter().all(|v| v == value) {
                    continue;
                }
                if !COLOR_ATTRS.contains(name) || values.iter().any(|v| v.contains("url(")) {
                    return None;
                }
                differing.push((a, values.iter().map(ToString::to_string).collect()));
            }
            let tag = tags.into_iter().next()?;
            plan.push(Some((tag, differing)));
        }

        let mut out = String::new();
        let digits = self.opts.precision;
        for (i, (tok, step)) in base.iter().zip(plan).enumerate() {
            let Some((mut tag, differing)) = step else {
                out.push_str(&if i == 0 { Cow::Borrowed(*tok) } else { round_tag(tok, digits) });
                continue;
            };
            let mut style = String::new();
            for (a, values) in &differing {
                let n = self.color_var(values.clone());
                let _ = write!(style, "{}: var(--kf{n}); ", tag.attrs[*a].0);
            }
            let removed: Vec<usize> = differing.iter().map(|(a, _)| *a).collect();
            let mut a = 0;
            tag.attrs.retain(|_| {
                a += 1;
                !removed.contains(&(a - 1))
            });
            if let Some((_, existing)) = tag.attrs.iter_mut().find(|(n, _)| *n == "style") {
                *existing = Cow::Owned(format!("{style}{existing}"));
            } else {
                tag.attrs.push(("style", Cow::Owned(style.trim_end().trim_end_matches(';').to_owned())));
            }
            if i != 0 {
                tag.round(digits);
            }
            out.push_str(&tag.to_string());
        }
        Some(out)
    }

    /// Текст SVG варианта (без слияния), с округлением.
    fn emit(&self, svg: &[&str]) -> String {
        let digits = self.opts.precision;
        svg.iter().enumerate().map(|(i, t)| if i == 0 { Cow::Borrowed(*t) } else { round_tag(t, digits) }).collect()
    }

    fn color_var(&mut self, values: Vec<String>) -> usize {
        let next = self.colors.len();
        *self.colors.entry(values).or_insert(next)
    }

    /// Метка страницы для переменных: хэш таблицы цветов (стабильный —
    /// одинаков в любой сборке и на любой машине).
    fn scope(&self) -> String {
        let mut table: Vec<_> = self.colors.iter().collect();
        table.sort_by_key(|(_, n)| **n);
        let mut h = StableHasher::new();
        for (values, n) in table {
            h.u64(*n as u64).u64(values.len() as u64);
            for v in values {
                h.str(v);
            }
        }
        format!("{:08x}", h.finish() & 0xffff_ffff)
    }

    fn css(&self, themes: &[String], scope: &str) -> String {
        let mut table: Vec<_> = self.colors.iter().collect();
        table.sort_by_key(|(_, n)| **n);
        let mut css = String::from("<style>");
        for (t, theme) in themes.iter().enumerate() {
            let _ = write!(css, r#":root[data-theme="{theme}"] [data-k-figs="{scope}"] {{ "#);
            for (values, n) in &table {
                let _ = write!(css, "--kf{n}: {}; ", values[t]);
            }
            css.push_str("}\n");
        }
        css.push_str("</style>");
        css
    }
}

/// Помечает страницу меткой переменных: на `<article class="k-doc">`
/// (шаблон baluk) или обёрткой вокруг всего.
fn scoped(body: &str, scope: &str) -> String {
    const ARTICLE: &str = r#"<article class="k-doc""#;
    if let Some(i) = body.find(ARTICLE) {
        let at = i + ARTICLE.len();
        format!(r#"{} data-k-figs="{scope}"{}"#, &body[..at], &body[at..])
    } else {
        format!(r#"<div data-k-figs="{scope}">{body}</div>"#)
    }
}

/// Открывающий тег с атрибутами.
#[derive(Debug)]
struct Tag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, Cow<'a, str>)>,
    self_closing: bool,
}

impl<'a> Tag<'a> {
    /// `<name a="…" b="…">` или `<name …/>`; закрывающие теги и прочее — `None`.
    fn parse(raw: &'a str) -> Option<Self> {
        let inner = raw.strip_prefix('<')?.strip_suffix('>')?;
        if inner.starts_with(['/', '!', '?']) {
            return None;
        }
        let (inner, self_closing) = match inner.strip_suffix('/') {
            Some(i) => (i, true),
            None => (inner, false),
        };
        let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
        let name = &inner[..name_end];
        let mut attrs = Vec::new();
        let mut rest = inner[name_end..].trim_start();
        while !rest.is_empty() {
            let eq = rest.find("=\"")?;
            let key = &rest[..eq];
            let value_start = eq + 2;
            let value_len = rest[value_start..].find('"')?;
            attrs.push((key, Cow::Borrowed(&rest[value_start..value_start + value_len])));
            rest = rest[value_start + value_len + 1..].trim_start();
        }
        Some(Self { name, attrs, self_closing })
    }

    fn round(&mut self, digits: Option<u8>) {
        let Some(p) = digits else { return };
        for (name, value) in &mut self.attrs {
            let rounded = match *name {
                "d" => round_path(value, p),
                "transform" | "patternTransform" => round_transform(value, p),
                n if ROUND_ATTRS.contains(&n) => round_numbers(value, p),
                _ => continue,
            };
            if let Some(r) = rounded {
                *value = Cow::Owned(r);
            }
        }
    }
}

impl std::fmt::Display for Tag<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{}", self.name)?;
        for (k, v) in &self.attrs {
            write!(f, r#" {k}="{v}""#)?;
        }
        f.write_str(if self.self_closing { "/>" } else { ">" })
    }
}

/// Значение атрибута из сырого тега.
fn attr<'a>(raw: &'a str, name: &str) -> Option<Cow<'a, str>> {
    Tag::parse(raw)?.attrs.into_iter().find(|(k, _)| *k == name).map(|(_, v)| v)
}

/// Округляет координаты в теге; не тег или нечего округлять — как есть.
fn round_tag(raw: &str, digits: Option<u8>) -> Cow<'_, str> {
    if digits.is_none() || !raw.starts_with('<') || raw.starts_with("</") {
        return Cow::Borrowed(raw);
    }
    match Tag::parse(raw) {
        Some(mut tag) => {
            tag.round(digits);
            Cow::Owned(tag.to_string())
        }
        None => Cow::Borrowed(raw),
    }
}

/// Число в целых долях `10^-p`.
fn to_grid(v: f64, p: u8) -> i64 {
    #[expect(clippy::cast_possible_truncation, reason = "coordinates of a figure are far below i64::MAX in grid units")]
    let n = (v * 10f64.powi(i32::from(p))).round() as i64;
    n
}

/// `1234`, p = 2 → `12.34`; лишние нули и точка отбрасываются.
fn fmt_grid(n: i64, p: u8) -> String {
    if p == 0 {
        return n.to_string();
    }
    let sign = if n < 0 { "-" } else { "" };
    let abs = n.unsigned_abs();
    let scale = 10u64.pow(u32::from(p));
    let (int, frac) = (abs / scale, abs % scale);
    if frac == 0 {
        return format!("{sign}{int}");
    }
    let frac = format!("{frac:0width$}", width = usize::from(p));
    format!("{sign}{int}.{}", frac.trim_end_matches('0'))
}

fn round_num(v: f64, p: u8) -> String {
    fmt_grid(to_grid(v, p), p)
}

/// Разбивает строку на числа и всё остальное.
fn split_numbers(s: &str) -> Vec<Result<f64, &str>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut other = 0;
    while i < bytes.len() {
        let starts_number = bytes[i].is_ascii_digit()
            || (matches!(bytes[i], b'-' | b'.') && bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit() || *b == b'.'));
        if !starts_number {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
            j += 1;
        }
        if j < bytes.len() && matches!(bytes[j], b'e' | b'E') {
            let mut k = j + 1;
            if k < bytes.len() && matches!(bytes[k], b'-' | b'+') {
                k += 1;
            }
            if k < bytes.len() && bytes[k].is_ascii_digit() {
                j = k;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
            }
        }
        if let Ok(v) = s[i..j].parse::<f64>() {
            if other < i {
                out.push(Err(&s[other..i]));
            }
            out.push(Ok(v));
            other = j;
        }
        i = j;
    }
    if other < s.len() {
        out.push(Err(&s[other..]));
    }
    out
}

/// Все числа строки — до `p` знаков.
fn round_numbers(s: &str, p: u8) -> Option<String> {
    let parts = split_numbers(s);
    parts.iter().any(Result::is_ok).then(|| {
        parts
            .iter()
            .map(|part| match part {
                Ok(v) => Cow::Owned(round_num(*v, p)),
                Err(t) => Cow::Borrowed(*t),
            })
            .collect()
    })
}

/// `matrix(a b c d e f)`: поворот и масштаб (a–d) — не грубее
/// [`SCALE_DIGITS`], сдвиг — до `p`. `scale(…)` — не грубее [`SCALE_DIGITS`].
fn round_transform(s: &str, p: u8) -> Option<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let close = rest[open..].find(')')? + open;
        let name = rest[..open].trim();
        out.push_str(&rest[..=open]);
        let args = &rest[open + 1..close];
        let digits = |k: usize| match name {
            "matrix" if k < 4 => p.max(SCALE_DIGITS),
            "scale" => p.max(SCALE_DIGITS),
            _ => p,
        };
        let mut k = 0;
        for part in split_numbers(args) {
            match part {
                Ok(v) => {
                    out.push_str(&round_num(v, digits(k)));
                    k += 1;
                }
                Err(t) => out.push_str(t),
            }
        }
        out.push(')');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Путь `typst-svg`: `M 0 0` и дальше относительные `m l h v c q a Z`.
/// Точки округляются в абсолютных координатах, разности — между
/// округлёнными, поэтому ошибка не больше полушага сетки в любой точке.
/// Незнакомая команда или неожиданное число аргументов — `None`.
fn round_path(d: &str, p: u8) -> Option<String> {
    let mut toks = Vec::new();
    for part in split_numbers(d) {
        match part {
            Ok(v) => toks.push(PathTok::Num(v)),
            Err(text) => {
                for ch in text.chars().filter(|c| !c.is_whitespace() && *c != ',') {
                    if !ch.is_ascii_alphabetic() {
                        return None;
                    }
                    toks.push(PathTok::Cmd(ch));
                }
            }
        }
    }
    let mut pen = Pen { p, out: String::with_capacity(d.len() / 2), ..Pen::default() };
    let mut i = 0;
    while i < toks.len() {
        let PathTok::Cmd(cmd) = toks[i] else { return None };
        let args: Vec<f64> =
            toks[i + 1..].iter().map_while(|t| if let PathTok::Num(v) = t { Some(*v) } else { None }).collect();
        i += 1 + args.len();
        pen.command(cmd, &args)?;
    }
    Some(pen.out)
}

#[derive(Debug, Clone, Copy)]
enum PathTok {
    Cmd(char),
    Num(f64),
}

/// Перо: точная позиция, округлённая (в долях сетки) и начало подпути.
#[derive(Debug, Default)]
struct Pen {
    p: u8,
    out: String,
    x: f64,
    y: f64,
    gx: i64,
    gy: i64,
    start: (f64, f64, i64, i64),
}

impl Pen {
    fn command(&mut self, cmd: char, a: &[f64]) -> Option<()> {
        if !self.out.is_empty() && cmd != 'Z' && cmd != 'z' {
            self.out.push(' ');
        }
        match (cmd, a.len()) {
            ('M', 2) => {
                (self.x, self.y) = (a[0], a[1]);
                (self.gx, self.gy) = (to_grid(a[0], self.p), to_grid(a[1], self.p));
                let _ = write!(self.out, "M {} {}", fmt_grid(self.gx, self.p), fmt_grid(self.gy, self.p));
                self.start = (self.x, self.y, self.gx, self.gy);
            }
            ('m' | 'l', 2) => {
                self.out.push(cmd);
                self.point(a[0], a[1], true);
                if cmd == 'm' {
                    self.start = (self.x, self.y, self.gx, self.gy);
                }
            }
            ('h', 1) => {
                let gx = to_grid(self.x + a[0], self.p);
                let _ = write!(self.out, "h {}", fmt_grid(gx - self.gx, self.p));
                self.x += a[0];
                self.gx = gx;
            }
            ('v', 1) => {
                let gy = to_grid(self.y + a[0], self.p);
                let _ = write!(self.out, "v {}", fmt_grid(gy - self.gy, self.p));
                self.y += a[0];
                self.gy = gy;
            }
            ('c', 6) | ('q', 4) => {
                self.out.push(cmd);
                let n = a.len();
                for k in (0..n - 2).step_by(2) {
                    self.point(a[k], a[k + 1], false);
                }
                self.point(a[n - 2], a[n - 1], true);
            }
            ('a', 7) => {
                let p = self.p;
                let _ = write!(
                    self.out,
                    "a {} {} {} {} {}",
                    round_num(a[0], p),
                    round_num(a[1], p),
                    round_num(a[2], p),
                    round_num(a[3], 0),
                    round_num(a[4], 0),
                );
                self.point(a[5], a[6], true);
            }
            ('Z' | 'z', 0) => {
                self.out.push('Z');
                (self.x, self.y, self.gx, self.gy) = self.start;
            }
            _ => return None,
        }
        Some(())
    }

    /// Точка относительно текущей: пишет разность округлённых; `advance` —
    /// перо переходит в неё (конец отрезка, а не опорная точка кривой).
    fn point(&mut self, dx: f64, dy: f64, advance: bool) {
        let (ax, ay) = (to_grid(self.x + dx, self.p), to_grid(self.y + dy, self.p));
        let _ = write!(self.out, " {} {}", fmt_grid(ax - self.gx, self.p), fmt_grid(ay - self.gy, self.p));
        if advance {
            (self.x, self.y, self.gx, self.gy) = (self.x + dx, self.y + dy, ax, ay);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEMES: [&str; 2] = ["classic", "night"];

    fn themes() -> Vec<String> {
        THEMES.iter().map(ToString::to_string).collect()
    }

    fn variants(svgs: [&str; 2]) -> String {
        THEMES.iter().zip(svgs).fold(String::new(), |mut out, (t, s)| {
            let _ = write!(out, r#"{VARIANT_OPEN}{t}">{s}</div>"#);
            out
        })
    }

    const GLYPH: &str =
        r#"<defs><symbol id="gA" overflow="visible"><path d="M 0 0m 1.123456 2l 0.5 0Z "/></symbol></defs>"#;

    #[test]
    fn grid_format() {
        assert_eq!(fmt_grid(1234, 2), "12.34");
        assert_eq!(fmt_grid(1200, 2), "12");
        assert_eq!(fmt_grid(-5, 2), "-0.05");
        assert_eq!(fmt_grid(0, 2), "0");
        assert_eq!(fmt_grid(7, 0), "7");
        assert_eq!(round_num(1e-9, 2), "0");
    }

    #[test]
    fn path_rounding_does_not_drift() {
        // Сто шагов по 0.004: по отдельности каждый округлился бы в 0.
        let d = format!("M 0 0{}", "h 0.004".repeat(100));
        let r = round_path(&d, 2).unwrap();
        let sum: f64 = split_numbers(&r).into_iter().filter_map(Result::ok).sum();
        assert!((sum - 0.4).abs() < 0.006, "{r}");
        assert_eq!(
            round_path("M 0 0m 1.123456 2.5c 0.1 0.2 0.333 0.444 1 1Z m 0.004 0v -1.999999", 2).unwrap(),
            // 1.123456 + 0.333 = 1.456 → 1.46 − 1.12 = 0.34 (разность округлённых точек)
            "M 0 0 m 1.12 2.5 c 0.1 0.2 0.34 0.44 1 1Z m 0.01 0 v -2"
        );
        assert_eq!(round_path("M 0 0 X 1 2", 2), None, "незнакомая команда");
    }

    #[test]
    fn transform_keeps_scale_precision() {
        assert_eq!(
            round_transform("matrix(0.70710678 0.70710678 -0.70710678 0.70710678 12.3456789 -0.001)", 2).unwrap(),
            "matrix(0.7071 0.7071 -0.7071 0.7071 12.35 0)"
        );
        assert_eq!(round_transform("translate(44.220472441 89.915355906)", 1).unwrap(), "translate(44.2 89.9)");
    }

    #[test]
    fn themes_differing_in_colors_merge() {
        let a = r##"<svg style="width: 1.123456em"><path fill="#111111" stroke="#222222" d="M 0 0h 1.23456"/><use xlink:href="#gA" fill="#111111"/></svg>"##;
        let b = r##"<svg style="width: 1.123456em"><path fill="#eeeeee" stroke="#222222" d="M 0 0h 1.23456"/><use xlink:href="#gA" fill="#eeeeee"/></svg>"##;
        let html = format!(r#"<article class="k-doc"><figure>{}</figure></article>"#, variants([a, b]));
        let o = optimize(&html, &themes(), FigureOptions::default());
        assert_eq!(o.stats.merged, 1);
        assert_eq!(o.stats.colors, 1, "одна пара цветов — одна переменная");
        assert!(!o.body.contains("k-frame-v"));
        assert!(
            o.body.contains(r##"<path stroke="#222222" d="M 0 0 h 1.23" style="fill: var(--kf0)"/>"##),
            "{}",
            o.body
        );
        assert!(o.body.contains(r#"<svg style="width: 1.123456em">"#), "корневой тег не трогаем");
        assert!(o.styles.contains(r#":root[data-theme="night"] [data-k-figs="#));
        assert!(o.styles.contains("--kf0: #eeeeee;"));
        assert!(o.body.starts_with(r#"<article class="k-doc" data-k-figs=""#));
    }

    #[test]
    fn different_geometry_keeps_variants() {
        let a = r##"<svg><path fill="#111111" d="M 0 0h 1"/></svg>"##;
        let b = r##"<svg><path fill="#eeeeee" d="M 0 0h 2"/></svg>"##;
        let o = optimize(&variants([a, b]), &themes(), FigureOptions { precision: None });
        assert_eq!(o.stats.merged, 0);
        assert_eq!(o.body, variants([a, b]));
        assert_eq!(o.stats.colors, 0, "отвергнутый рисунок не заводит переменных");
    }

    #[test]
    fn glyphs_are_shared() {
        let svg = format!(r##"<svg>{GLYPH}<use xlink:href="#gA"/></svg>"##);
        let page = format!("<p>1</p>{}<p>2</p>{}", variants([&svg, &svg]), variants([&svg, &svg]));
        let o = optimize(&page, &themes(), FigureOptions::default());
        assert_eq!(o.stats.glyphs, 1);
        assert_eq!(o.body.matches("<symbol").count(), 1);
        assert!(o.body.starts_with(r#"<svg class="k-glyphs""#));
        assert!(o.body.contains(r#"<path d="M 0 0 m 1.12 2 l 0.5 0Z"/>"#), "{}", o.body);
        assert_eq!(o.body.matches(r##"<svg><use xlink:href="#gA"/></svg>"##).count(), 2);
    }

    #[test]
    fn untouched_without_figures() {
        let o = optimize("<p>текст 1.23456</p>", &themes(), FigureOptions::default());
        assert_eq!(o.body, "<p>текст 1.23456</p>");
        assert!(o.styles.is_empty());
    }

    #[test]
    fn variant_without_svg_is_kept() {
        // пустой рисунок: раньше цикл не сдвигался и зависал
        let body = format!(r#"{VARIANT_OPEN}classic"></div>{VARIANT_OPEN}night"></div>"#);
        let o = optimize(&body, &themes(), FigureOptions::default());
        assert_eq!(o.body, body);
    }
}
