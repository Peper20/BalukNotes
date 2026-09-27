//! Настройки (схема и значения) и темы оформления.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use notes_core::themes::Theme;
use serde_json::{Map, Value};

use crate::AppState;
use crate::api::SettingsResponse;
use crate::assets::css;
use crate::error::{ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/themes", get(themes))
        .route("/api/themes.css", get(themes_css))
}

async fn get_settings(State(s): State<AppState>) -> Json<SettingsResponse> {
    Json(SettingsResponse { schema: s.settings.schema().clone(), values: s.settings.values() })
}

async fn put_settings(State(s): State<AppState>, Json(patch): Json<Map<String, Value>>) -> ApiResult<Json<Value>> {
    let settings = s.settings.clone();
    let values = blocking(move || settings.update(&patch)).await?;
    Ok(Json(Value::Object(values)))
}

async fn themes(State(s): State<AppState>) -> Json<Vec<Theme>> {
    Json(s.notes.themes().themes().to_vec())
}

async fn themes_css(State(s): State<AppState>) -> axum::response::Response {
    css(s.notes.themes().css().to_owned())
}
