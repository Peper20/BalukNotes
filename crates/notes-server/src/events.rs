//! События клиенту (`GET /api/events`, Server-Sent Events): файлы хранилища
//! изменились — клиент сверяет версию показанной заметки и список заметок,
//! вместо опроса раз в N секунд.
//!
//! Первое событие — `hello` ([`EventsHello`]: следит ли сервер за файлами);
//! дальше — `change` ([`ChangeEvent`]) на каждую пачку изменений
//! (`notes_core::watch`). Токен (если задан) — как у всего API: `EventSource`
//! не шлёт заголовков, поэтому cookie или `?token=`. При остановке сервера
//! поток закрывается (иначе плавная остановка ждала бы его вечно); когда
//! наблюдатель сломался — тоже, после последнего `change`.

use std::convert::Infallible;

use axum::Router;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures_util::Stream;
use tokio::sync::broadcast::error::RecvError;

use crate::AppState;
use crate::api::{ChangeEvent, EventsHello};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/events", get(events))
}

async fn events(State(s): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let watching = s.notes.watching();
    let hello = Event::default().event("hello").json_data(EventsHello { watching });
    let rx = s.events.subscribe();
    let closing = s.closing.subscribe();
    let first = futures_util::stream::iter(hello.ok().map(Ok));
    let notes = s.notes.clone();
    let rest = futures_util::stream::unfold((rx, closing, false), move |(mut rx, mut closing, done)| {
        let notes = notes.clone();
        async move {
            if done || *closing.borrow() {
                return None;
            }
            let change = tokio::select! {
                got = rx.recv() => match got {
                    Ok(change) => change,
                    // Не успели прочитать — значит, что-то точно поменялось.
                    Err(RecvError::Lagged(_)) => ChangeEvent { seq: 0, paths: Vec::new() },
                    Err(RecvError::Closed) => return None,
                },
                _ = closing.changed() => return None,
            };
            let event = Event::default().event("change").json_data(&change).ok()?;
            // Наблюдатель сломался — после этого события закрыть поток:
            // клиент переподключится, получит `hello` без наблюдателя и
            // перейдёт на опрос.
            let done = watching && !notes.watching();
            Some((Ok(event), (rx, closing, done)))
        }
    });
    Sse::new(futures_util::StreamExt::chain(first, rest)).keep_alive(KeepAlive::default())
}
