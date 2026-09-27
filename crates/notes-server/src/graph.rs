//! Граф заметок: весь (узлы и рёбра) и разложенный по фильтру
//! (`notes_core::vault_graph`).

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use notes_core::graph::Graph;
use notes_core::vault_graph::{GraphFilter, GraphLayout};

use crate::AppState;
use crate::error::{ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/graph", get(graph)).route("/api/graph/layout", post(graph_layout))
}

async fn graph_layout(State(s): State<AppState>, Json(filter): Json<GraphFilter>) -> ApiResult<Json<GraphLayout>> {
    let notes = s.notes.clone();
    let layout = blocking(move || notes.graph_layout(&filter)).await?;
    Ok(Json(layout))
}

async fn graph(State(s): State<AppState>) -> ApiResult<Json<Graph>> {
    let notes = s.notes.clone();
    let graph = blocking(move || Ok(notes.index()?.graph())).await?;
    Ok(Json(graph))
}
