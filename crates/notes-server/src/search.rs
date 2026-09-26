//! Поиск по тексту всех заметок.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use notes_core::search::SearchHit;

use crate::AppState;
use crate::error::{ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/search", get(search))
}

#[derive(Debug, serde::Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<usize>,
}

async fn search(State(s): State<AppState>, Query(q): Query<SearchQuery>) -> ApiResult<Json<Vec<SearchHit>>> {
    let notes = s.notes.clone();
    let limit = q.limit.unwrap_or(30).min(200);
    Ok(Json(blocking(move || notes.search(&q.q, limit)).await?))
}
