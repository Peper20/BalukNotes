//! Шрифты оформления для браузера: `@font-face` по частям с `unicode-range`
//! и сами части (WOFF2). Какие шрифты нужны — основные шрифты тем
//! (`notes_core::themes::ThemeSet::web_fonts`).

use std::fmt::Write as _;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use notes_core::Notes;
use notes_core::fonts::WebVariant;

use crate::AppState;
use crate::assets::css;
use crate::error::blocking;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/fonts.css", get(fonts_css)).route("/fonts/{family}/{style}/{file}", get(font))
}

/// Шрифты оформления, которые нужны браузеру: основные шрифты тем.
pub fn web_fonts(notes: &Notes) -> &[String] {
    notes.themes().web_fonts()
}

/// `@font-face` на каждую часть шрифта: браузер качает только части со
/// знаками страницы (`unicode-range`).
async fn fonts_css(State(s): State<AppState>) -> Response {
    let notes = s.notes.clone();
    let css_text = blocking(move || Ok(font_faces(&notes, "/fonts/"))).await;
    match css_text {
        Ok(text) => css(text),
        Err(e) => e.into_response(),
    }
}

/// `@font-face` шрифтов оформления; `base` — путь к файлам (`/fonts/` у
/// сервера, `fonts/` у статического сайта). Файл части — `{base}{семейство}/{начертание}/{часть}.woff2`
/// (у сайта — [`font_file_name`]).
pub fn font_faces(notes: &Notes, base: &str) -> String {
    let mut out = String::from("/* Шрифты оформления: те же файлы, что у Typst, в WOFF2 и по наборам знаков. */\n");
    for family in web_fonts(notes) {
        for v in WebVariant::ALL {
            let Some(face) = notes.fonts().web_face(family, v) else { continue };
            for chunk in &face.chunks {
                let url = if base.starts_with('/') {
                    format!("{base}{}/{}/{}.woff2", family.replace(' ', "%20"), v.slug(), chunk.name)
                } else {
                    format!("{base}{}", font_file_name(family, v, &chunk.name))
                };
                let range = chunk.unicode_range.as_ref().map(|r| format!(" unicode-range: {r};")).unwrap_or_default();
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

/// Имя файла части шрифта у статического сайта: `Gentium-Plus-regular-latin.woff2`.
pub fn font_file_name(family: &str, variant: WebVariant, chunk: &str) -> String {
    format!("{}-{}-{chunk}.woff2", family.replace(' ', "-"), variant.slug())
}

async fn font(State(s): State<AppState>, Path((family, style, file)): Path<(String, String, String)>) -> Response {
    let (Some(variant), Some(chunk)) = (WebVariant::from_slug(&style), file.strip_suffix(".woff2")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !web_fonts(&s.notes).contains(&family) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (notes, chunk) = (s.notes.clone(), chunk.to_owned());
    let data = blocking(move || Ok(notes.fonts().web_face(&family, variant).and_then(|face| face.file(&chunk)))).await;
    match data {
        Ok(Some(data)) => {
            ([(header::CONTENT_TYPE, "font/woff2"), (header::CACHE_CONTROL, "public, max-age=86400")], data.to_vec())
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => e.into_response(),
    }
}
