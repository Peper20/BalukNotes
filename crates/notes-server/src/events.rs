//! Events for the client (`GET /api/vaults/{vault}/events?after=<seq>`): vault
//! files changed, so the client checks the version of the shown note and the
//! note list, instead of polling every N seconds.
//!
//! Long polling: the answer ([`EventsResponse`]) comes at once if there were
//! changes after `after`, otherwise on the first change or empty after
//! [`WAIT`]; the next request carries the `seq` of the answer. Without `after`
//! it comes at once: the number and whether the server watches the files. Not
//! SSE: the Tauri window gets an answer of its URL scheme only as a whole
//! (docs/research/E7.md). Not watching (the watcher broke, an in-memory vault):
//! nothing to wait for, the answer comes at once. A server stop answers the
//! waiting requests.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};

use crate::AppState;
use crate::api::EventsResponse;
use crate::error::ApiResult;

/// How long a request waits for changes: less than proxy and browser timeouts.
pub const WAIT: Duration = Duration::from_secs(25);

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/vaults/{vault}/events", get(events))
}

#[derive(Debug, serde::Deserialize)]
struct EventsQuery {
    after: Option<u64>,
}

async fn events(
    State(s): State<AppState>,
    Path(vault): Path<String>,
    Query(q): Query<EventsQuery>,
) -> ApiResult<Json<EventsResponse>> {
    let vault = s.vault(vault).await?;
    let (mut seq, _waiting) = vault.wait_events();
    let mut closing = s.closing.subscribe();
    if let Some(after) = q.after
        && vault.notes.watching()
        && vault.events.since(after).is_empty()
        && !*closing.borrow_and_update()
    {
        tokio::select! {
            _ = seq.changed() => {}
            _ = closing.changed() => {}
            () = tokio::time::sleep(WAIT) => {}
        }
    }
    let changes = q.after.map(|after| vault.events.since(after)).unwrap_or_default();
    Ok(Json(EventsResponse { watching: vault.notes.watching(), seq: vault.events.latest(), changes }))
}
