//! HTTP API и раздача веб-клиента.
//!
//! Один и тот же интерфейс для браузера (VPS) и приложения (Tauri поверх
//! localhost). Компиляция — блокирующая работа, она уходит в
//! `spawn_blocking`, чтобы не держать поток асинхронного рантайма.
//!
//! Ответы сжимаются (brotli или gzip — что примет клиент): HTML заметок с
//! формулами и рисунками сжимается в 7–13 раз («Матан»: 3,7 МБ → 0,28 МБ
//! brotli, 0,54 МБ gzip; уровни по умолчанию — быстрее «лучших» при почти том же
//! размере).
//!
//! | Путь                         | Что                                          |
//! |------------------------------|----------------------------------------------|
//! | `GET /`, `GET /n/{*id}`      | клиент (одна страница, маршрутизация в JS)   |
//! | `GET /assets/{*path}`        | файлы клиента из `web/`                      |
//! | `GET /api/notes`             | список заметок и книг                        |
//! | `GET /api/notes/{*id}`       | заметка: HTML, заголовки, ссылки, ошибки     |
//! | `GET /api/version/{*id}`     | версия заметки — дёшево, без компиляции      |
//! | `GET /api/links/{*id}`       | ссылки заметки и обратные ссылки на неё      |
//! | `GET /api/graph`             | граф заметок: узлы и рёбра                   |
//! | `GET /api/pdf/{*id}?theme=`  | заметка в PDF (по умолчанию — первая тема)   |
//! | `GET /api/settings`          | схема и значения настроек                    |
//! | `PUT /api/settings`          | изменить настройки (частично)                |
//! | `GET /api/themes`            | темы: имя, тёмная ли                         |
//! | `GET /api/themes.css`        | CSS-переменные тем                           |
//! | `GET /api/fonts.css`         | `@font-face` для шрифтов оформления          |
//! | `GET /fonts/{family}/{style}`| файл шрифта                                  |

use std::fmt::Write as _;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use notes_core::fonts::WebVariant;
use notes_core::settings::SettingsStore;
use notes_core::{NoteId, Notes};
use rust_embed::RustEmbed;
use serde_json::{Map, Value, json};
use tower_http::compression::CompressionLayer;

/// Шрифты оформления, которые нужны браузеру (из `konspekt/theme.typ`).
const WEB_FONTS: &[&str] = &["Gentium Plus", "JetBrains Mono", "New Computer Modern Math"];

/// Файлы клиента. В отладочной сборке читаются с диска (правка без
/// пересборки), в релизной — встроены в бинарник.
#[derive(RustEmbed)]
#[folder = "../../web/"]
struct WebAssets;

#[derive(Debug, Clone)]
pub struct AppState {
    pub notes: Arc<Notes>,
    pub settings: Arc<SettingsStore>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(shell))
        .route("/n/{*id}", get(shell))
        .route("/assets/{*path}", get(asset))
        .route("/api/notes", get(list_notes))
        .route("/api/notes/{*id}", get(note))
        .route("/api/version/{*id}", get(version))
        .route("/api/links/{*id}", get(links))
        .route("/api/graph", get(graph))
        .route("/api/pdf/{*id}", get(pdf))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/themes", get(themes))
        .route("/api/themes.css", get(themes_css))
        .route("/api/fonts.css", get(fonts_css))
        .route("/fonts/{family}/{style}", get(font))
        .layer(CompressionLayer::new())
        .with_state(state)
}

/// Запускает сервер на готовом сокете до сигнала `shutdown`.
pub async fn serve(
    listener: tokio::net::TcpListener,
    state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown).await
}

/// Файл клиента из `web/` — для статической сборки.
pub fn web_asset(path: &str) -> Option<Vec<u8>> {
    WebAssets::get(path).map(|f| f.data.into_owned())
}

/// Шрифты оформления, которые нужны браузеру.
pub fn web_fonts() -> &'static [&'static str] {
    WEB_FONTS
}

/// Ошибка API: код и сообщение в JSON.
#[derive(Debug)]
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<notes_core::Error> for ApiError {
    fn from(e: notes_core::Error) -> Self {
        use notes_core::Error as E;
        let code = match e {
            E::NotFound(_) => StatusCode::NOT_FOUND,
            E::InvalidId { .. } | E::Setting { .. } => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if code == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!("{e}");
        }
        Self(code, e.to_string())
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// Блокирующая работа ядра — в отдельном потоке.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> notes_core::Result<T> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("сбой задачи: {e}")))?
        .map_err(Into::into)
}

async fn shell() -> Response {
    match WebAssets::get("index.html") {
        Some(file) => Html(file.data).into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, "нет web/index.html").into_response(),
    }
}

async fn asset(Path(path): Path<String>) -> Response {
    let Some(file) = WebAssets::get(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = file.metadata.mimetype().to_owned();
    ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache".into())], file.data).into_response()
}

async fn list_notes(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let notes = s.notes.clone();
    let entries = blocking(move || notes.entries()).await?;
    let list: Vec<Value> = entries
        .iter()
        .map(|e| json!({ "id": e.id, "kind": e.kind, "name": e.id.name(), "folder": e.id.parent() }))
        .collect();
    Ok(Json(Value::Array(list)))
}

async fn note(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = NoteId::new(id)?;
    let (notes, opts) = (s.notes.clone(), s.settings.figure_options());
    let page = blocking(move || notes.page(&id, opts)).await?;
    Ok(Json(page).into_response())
}

async fn version(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = NoteId::new(id)?;
    let (notes, opts) = (s.notes.clone(), s.settings.figure_options());
    let version = blocking(move || notes.version(&id, opts)).await?;
    Ok(Json(json!({ "version": version })))
}

async fn links(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = NoteId::new(id)?;
    let notes = s.notes.clone();
    blocking(move || {
        notes.vault().entry(&id)?;
        let snap = notes.links()?;
        let outgoing: Vec<Value> = snap
            .outgoing(&id)
            .iter()
            .map(|l| json!({ "target": l.target, "anchor": l.anchor, "exists": snap.exists(&l.target) }))
            .collect();
        Ok(Json(json!({ "outgoing": outgoing, "backlinks": snap.backlinks(&id) })))
    })
    .await
}

async fn graph(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let notes = s.notes.clone();
    let graph = blocking(move || Ok(notes.links()?.graph())).await?;
    Ok(Json(json!(graph)))
}

#[derive(Debug, serde::Deserialize)]
struct PdfQuery {
    theme: Option<String>,
}

async fn pdf(State(s): State<AppState>, Path(id): Path<String>, Query(q): Query<PdfQuery>) -> ApiResult<Response> {
    let id = NoteId::new(id)?;
    let notes = s.notes.clone();
    let theme = q.theme.unwrap_or_else(|| notes.themes().names().first().cloned().unwrap_or_default());
    let name = id.name().to_owned();
    let result = blocking(move || notes.pdf(&id, &theme)).await?;
    Ok(match result {
        Ok(bytes) => {
            // Имя файла в заголовке — по RFC 5987 (кириллица).
            let disposition = format!("inline; filename*=UTF-8''{}.pdf", percent(&name));
            ([(header::CONTENT_TYPE, "application/pdf".to_owned()), (header::CONTENT_DISPOSITION, disposition)], bytes)
                .into_response()
        }
        Err(errors) => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "не собралось", "errors": errors })))
            .into_response(),
    })
}

/// Процентное кодирование всего, кроме букв, цифр и `-._~`.
fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

async fn get_settings(State(s): State<AppState>) -> Json<Value> {
    Json(json!({ "schema": s.settings.schema(), "values": s.settings.values() }))
}

async fn put_settings(State(s): State<AppState>, Json(patch): Json<Map<String, Value>>) -> ApiResult<Json<Value>> {
    let settings = s.settings.clone();
    let values = blocking(move || settings.update(&patch)).await?;
    Ok(Json(Value::Object(values)))
}

async fn themes(State(s): State<AppState>) -> Json<Value> {
    Json(json!(s.notes.themes().themes()))
}

async fn themes_css(State(s): State<AppState>) -> Response {
    css(s.notes.themes().css().to_owned())
}

async fn fonts_css(State(s): State<AppState>) -> Response {
    let mut out = String::from("/* Шрифты оформления: те же файлы, что у Typst. */\n");
    for family in WEB_FONTS {
        for v in WebVariant::ALL {
            if s.notes.fonts().web_font(family, v).is_none() {
                continue;
            }
            let _ = writeln!(
                out,
                "@font-face {{ font-family: \"{family}\"; src: url(\"/fonts/{}/{}\"); font-style: {}; font-weight: {}; font-display: swap; }}",
                family.replace(' ', "%20"),
                v.slug(),
                if v.italic { "italic" } else { "normal" },
                if v.bold { 700 } else { 400 },
            );
        }
    }
    css(out)
}

async fn font(State(s): State<AppState>, Path((family, style)): Path<(String, String)>) -> Response {
    let Some(variant) = WebVariant::from_slug(&style) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !WEB_FONTS.contains(&family.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match s.notes.fonts().web_font(&family, variant) {
        Some(font) => {
            ([(header::CONTENT_TYPE, font.mime), (header::CACHE_CONTROL, "public, max-age=86400")], font.data.to_vec())
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn css(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}
