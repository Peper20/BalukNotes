//! Search in the text of all notes (`?q=&limit=`) or of one (`&note=<path>`:
//! "in this note" and Ctrl+F, every section in text order).

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use notes_core::NoteId;
use notes_core::search::SearchHit;

use crate::AppState;
use crate::error::{ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/vaults/{vault}/search", get(search))
}

#[derive(Debug, serde::Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<usize>,
    note: Option<String>,
}

async fn search(
    State(s): State<AppState>,
    Path(vault): Path<String>,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<Vec<SearchHit>>> {
    let notes = s.vault(vault).await?.notes.clone();
    let limit = q.limit.unwrap_or(30).min(200);
    let note = q.note.as_deref().map(NoteId::new).transpose()?;
    Ok(Json(
        blocking(move || match &note {
            Some(id) => notes.search_in(id, &q.q, limit),
            None => notes.search(&q.q, limit),
        })
        .await?,
    ))
}
