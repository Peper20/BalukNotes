//! События клиенту (`GET /api/vaults/{хранилище}/events?after=<seq>`): файлы
//! хранилища изменились - клиент сверяет версию показанной заметки и список
//! заметок, вместо опроса раз в N секунд.
//!
//! Долгий опрос: ответ ([`EventsResponse`]) сразу, если после `after` были
//! изменения, иначе - на первом изменении или через [`WAIT`] пустой; следующий
//! запрос - с `seq` ответа. Без `after` - сразу: номер и следит ли сервер за
//! файлами. Не SSE: окно Tauri получает ответ своей схемы адресов только
//! целиком (docs/research/E7.md). Не следит (наблюдатель сломался, хранилище
//! в памяти) - ждать нечего, ответ сразу. Остановка сервера отвечает ждущим.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};

use crate::AppState;
use crate::api::EventsResponse;
use crate::error::ApiResult;

/// Сколько запрос ждёт изменений: меньше тайм-аутов прокси и браузера.
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
