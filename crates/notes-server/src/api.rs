//! Ответы API, которые собирает сам сервер (остальные — типы ядра).
//!
//! С фичей `ts` все они выгружаются в TypeScript для клиента `app/`:
//! `cd app && npm run types`.

use notes_core::diag::Diagnostic;
use notes_core::graph::Backlink;
use notes_core::settings::Schema;
use notes_core::{NoteId, NoteKind};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Элемент `GET /api/notes`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NoteListItem {
    pub id: NoteId,
    pub kind: NoteKind,
    /// Последний сегмент пути: `SSH` для `Сеть/SSH`.
    pub name: String,
    /// Папка: `Сеть`; для корня — пустая строка.
    pub folder: String,
    /// Название из шаблона (`title: […]`), если есть.
    pub title: Option<String>,
    pub tags: Vec<String>,
}

/// `POST /api/warm`: что собрать заранее первым (заметки во вкладках,
/// недавние). Неизвестные и неверные пути пропускаются.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WarmRequest {
    pub ids: Vec<String>,
}

/// `GET /api/version/{id}`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VersionResponse {
    pub version: String,
}

/// Ссылка из заметки и есть ли её цель.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OutgoingLink {
    pub target: String,
    pub anchor: Option<String>,
    pub exists: bool,
}

/// `GET /api/links/{id}`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LinksResponse {
    pub outgoing: Vec<OutgoingLink>,
    pub backlinks: Vec<Backlink>,
}

/// `GET /api/settings`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SettingsResponse {
    pub schema: Schema,
    /// Ключ → число, строка или флаг (тип — по схеме).
    #[cfg_attr(feature = "ts", ts(type = "Record<string, number | string | boolean>"))]
    pub values: Map<String, Value>,
}

/// Тело ошибки. `errors` — ошибки компиляции (у PDF), иначе пусто.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ErrorResponse {
    pub error: String,
    pub errors: Vec<Diagnostic>,
}

/// Событие `change` потока `GET /api/events`: файлы хранилища изменились
/// (пачкой, см. `notes_core::watch`) — клиенту пора сверить версию заметки.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChangeEvent {
    /// Растёт с каждым изменением.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seq: u64,
    /// Изменившиеся файлы хранилища; пусто — неизвестно какие (проверить всё).
    pub paths: Vec<String>,
}

/// Первое событие `hello` потока `GET /api/events`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EventsHello {
    /// Сервер следит за файлами: события `change` будут. Иначе — только
    /// опрос версии.
    pub watching: bool,
}
