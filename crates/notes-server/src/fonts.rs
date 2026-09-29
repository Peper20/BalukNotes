//! Шрифты оформления для браузера: `@font-face` по частям с `unicode-range`
//! и сами части (WOFF2). Какие шрифты нужны — основные шрифты тем
//! (`notes_core::themes::ThemeSet::web_fonts`).

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use notes_core::fonts::WebVariant;

use crate::AppState;
use crate::assets::css;
use crate::error::blocking;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/fonts.css", get(fonts_css)).route("/fonts/{family}/{style}/{file}", get(font))
}

/// `@font-face` на каждую часть шрифта: браузер качает только части со
/// знаками страницы (`unicode-range`).
async fn fonts_css(State(s): State<AppState>) -> Response {
    let notes = s.vaults.library().clone();
    let css_text = blocking(move || Ok(notes.fonts().font_faces(notes.themes().web_fonts(), "/fonts/"))).await;
    match css_text {
        Ok(text) => css(text),
        Err(e) => e.into_response(),
    }
}

async fn font(State(s): State<AppState>, Path((family, style, file)): Path<(String, String, String)>) -> Response {
    let (Some(variant), Some(chunk)) = (WebVariant::from_slug(&style), file.strip_suffix(".woff2")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let notes = s.vaults.library().clone();
    if !notes.themes().web_fonts().contains(&family) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let chunk = chunk.to_owned();
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
