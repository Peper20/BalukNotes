//! Ответы API, которые собирает сам сервер (остальные — типы ядра).
//!
//! С фичей `ts` все они выгружаются в TypeScript для клиента `app/`:
//! `cd app && npm run types`.

use notes_core::diag::Diagnostic;
use notes_core::graph::Backlink;
use notes_core::search::TaggedChapter;
use notes_core::settings::Schema;
use notes_core::{NoteId, NoteKind, VaultName};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `GET /api/vaults`, ответ `POST /api/vaults`. Хранилища по умолчанию нет:
/// какое открыть, решает клиент (открытое последним) или пользователь.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VaultsResponse {
    /// Все хранилища по алфавиту (может не быть ни одного).
    pub vaults: Vec<VaultName>,
    /// Можно ли создать новое (нельзя, если сервер открыт на одном
    /// хранилище: `notes serve --vault <путь>`).
    pub can_create: bool,
}

/// `POST /api/vaults`: новое пустое хранилище.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CreateVaultRequest {
    pub name: String,
}

/// `PATCH /api/vaults/{хранилище}`: новое имя хранилища.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RenameVaultRequest {
    pub name: String,
}

/// Элемент `GET /api/vaults/{хранилище}/notes`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NoteListItem {
    pub id: NoteId,
    pub kind: NoteKind,
    /// Последний сегмент пути — имя файла: `SSH` для `Сеть/SSH`.
    pub name: String,
    /// Папка: `Сеть`; для корня — пустая строка.
    pub folder: String,
    /// Название для показа: из шаблона (`title: […]`), иначе — имя файла.
    pub title: String,
    /// Теги заметки; у книги — корня (`main.typ`), их наследуют все главы.
    pub tags: Vec<String>,
    /// Главы книги со своими тегами.
    pub chapters: Vec<TaggedChapter>,
}

/// Элемент `GET /api/vaults/{хранилище}/folders`: папка с заметками или
/// пустая (`Vault::folders`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FolderListItem {
    /// Путь от корня: `Сеть/Linux`.
    pub path: String,
    /// Название для показа: из `_folder.toml`, иначе — имя папки.
    pub title: String,
}

/// `POST /api/vaults/{хранилище}/rename`: переименовать заметку (книгу) или
/// папку; без `apply` — только план (новый путь, какие ссылки перепишутся).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RenameRequest {
    pub kind: notes_core::rename::RenameKind,
    /// Путь заметки или папки.
    pub id: String,
    /// Новое название.
    pub title: String,
    #[serde(default)]
    pub apply: bool,
}

/// `POST /api/vaults/{хранилище}/warm`: что собрать заранее первым (заметки во вкладках,
/// недавние). Неизвестные и неверные пути пропускаются.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WarmRequest {
    pub ids: Vec<String>,
}

/// `GET /api/vaults/{хранилище}/version/{id}`.
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

/// `GET /api/vaults/{хранилище}/links/{id}`.
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

/// `GET`/`PUT /api/vaults/{хранилище}/settings`: настройки для хранилища —
/// итог (`values`) и откуда он: общие для всех (`shared`) и заданные только
/// в этом хранилище (`own`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VaultSettingsResponse {
    pub schema: Schema,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, number | string | boolean>"))]
    pub values: Map<String, Value>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, number | string | boolean>"))]
    pub shared: Map<String, Value>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, number | string | boolean>"))]
    pub own: Map<String, Value>,
}

/// Тело ошибки. `errors` — ошибки компиляции (у PDF), иначе пусто.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ErrorResponse {
    pub error: String,
    pub errors: Vec<Diagnostic>,
}

/// Событие `change` потока `GET /api/vaults/{хранилище}/events`: файлы хранилища изменились
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

/// Первое событие `hello` потока `GET /api/vaults/{хранилище}/events`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EventsHello {
    /// Сервер следит за файлами: события `change` будут. Иначе — только
    /// опрос версии.
    pub watching: bool,
}
