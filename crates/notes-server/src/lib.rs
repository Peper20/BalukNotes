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
//! | `GET /`, `/n/{*id}`, `/tags…`| клиент (одна страница, маршрутизация в JS)   |
//! | `GET /assets/{*path}`        | файлы клиента (сборка `app/dist/assets`)     |
//! | `GET /api/notes`             | список заметок и книг                        |
//! | `GET /api/notes/{*id}`       | заметка: HTML, заголовки, ссылки, ошибки     |
//! | `…?chapter=N`, `…?anchor=`   | книга — одной главой (N-й или с якорем)      |
//! | `GET /api/version/{*id}`     | версия заметки — дёшево, без компиляции      |
//! | `GET /api/links/{*id}`       | ссылки заметки и обратные ссылки на неё      |
//! | `GET /api/graph`             | граф заметок: узлы и рёбра                   |
//! | `GET /api/search?q=&limit=`  | поиск по тексту всех заметок                 |
//! | `GET /api/preview/{*id}?anchor=` | превью заметки/раздела (без компиляции)  |
//! | `GET /api/pdf/{*id}?theme=`  | заметка в PDF (по умолчанию — первая тема)   |
//! | `GET /api/settings`          | схема и значения настроек                    |
//! | `PUT /api/settings`          | изменить настройки (частично)                |
//! | `GET /api/themes`            | темы: имя, тёмная ли                         |
//! | `GET /api/themes.css`        | CSS-переменные тем                           |
//! | `GET /api/fonts.css`         | `@font-face` для шрифтов оформления          |
//! | `GET /fonts/{family}/{style}`| файл шрифта                                  |

pub mod api;

use std::fmt::Write as _;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use notes_core::book::{Select, chapter_page};
use notes_core::fonts::WebVariant;
use notes_core::graph::Graph;
use notes_core::search::{Preview, SearchHit};
use notes_core::settings::SettingsStore;
use notes_core::themes::Theme;
use notes_core::{NoteId, Notes};

use crate::api::{ErrorResponse, LinksResponse, NoteListItem, OutgoingLink, SettingsResponse, VersionResponse};
use rust_embed::RustEmbed;
use serde_json::{Map, Value};
use tower_http::compression::CompressionLayer;

/// Шрифты оформления, которые нужны браузеру (из `konspekt/theme.typ`).
const WEB_FONTS: &[&str] = &["Gentium Plus", "JetBrains Mono", "New Computer Modern Math"];

/// Клиент — сборка `app/` (`npm run build` → `app/dist`). В отладочной
/// сборке читается с диска (пересобрали клиент — перезагрузите страницу), в
/// релизной — встроен в бинарник (без `app/dist` её не собрать, см. build.rs).
#[derive(RustEmbed)]
#[folder = "../../app/dist/"]
#[allow_missing = true]
struct WebAssets;

/// Страница вместо клиента, если он не собран.
const NO_CLIENT: &str = "<!doctype html><meta charset=utf-8><title>Клиент не собран</title>\
<p>Клиент не собран. Выполните в каталоге проекта:</p>\
<pre>npm --prefix app ci &amp;&amp; npm --prefix app run build</pre>\
<p>и обновите страницу. Для разработки клиента — <code>npm --prefix app run dev</code>.</p>";

#[derive(Debug, Clone)]
pub struct AppState {
    pub notes: Arc<Notes>,
    pub settings: Arc<SettingsStore>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(shell))
        .route("/n/{*id}", get(shell))
        .route("/tags", get(shell))
        .route("/tags/{*tag}", get(shell))
        .route("/assets/{*path}", get(asset))
        .route("/api/notes", get(list_notes))
        .route("/api/notes/{*id}", get(note))
        .route("/api/version/{*id}", get(version))
        .route("/api/links/{*id}", get(links))
        .route("/api/graph", get(graph))
        .route("/api/search", get(search))
        .route("/api/preview/{*id}", get(preview))
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

/// Файл клиента (`konspekt.css`, `static.js`, …) — для статической сборки.
pub fn web_asset(name: &str) -> Option<Vec<u8>> {
    WebAssets::get(&format!("assets/{name}")).map(|f| f.data.into_owned())
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
        (self.0, Json(ErrorResponse { error: self.1, errors: Vec::new() })).into_response()
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
        Some(file) => ([(header::CACHE_CONTROL, "no-cache")], Html(file.data)).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, Html(NO_CLIENT)).into_response(),
    }
}

async fn asset(Path(path): Path<String>) -> Response {
    let Some(file) = WebAssets::get(&format!("assets/{path}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = file.metadata.mimetype().to_owned();
    // Файлы сборки с хэшем в имени (index-CX38oHQo.js) не меняются никогда.
    let cache = if is_hashed(&path) { "public, max-age=31536000, immutable" } else { "no-cache" };
    ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache.into())], file.data).into_response()
}

/// `name-XXXXXXXX.ext`: Vite ставит в имя 8 символов хэша содержимого.
fn is_hashed(path: &str) -> bool {
    let stem = path.rsplit_once('.').map_or(path, |(s, _)| s);
    stem.rsplit_once('-')
        .is_some_and(|(_, h)| h.len() == 8 && h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

async fn list_notes(State(s): State<AppState>) -> ApiResult<Json<Vec<NoteListItem>>> {
    let notes = s.notes.clone();
    let list = blocking(move || {
        let index = notes.index()?;
        Ok(index
            .outlines()
            .map(|(e, o)| NoteListItem {
                id: e.id.clone(),
                kind: e.kind,
                name: e.id.name().to_owned(),
                folder: e.id.parent().to_owned(),
                title: o.title.clone(),
                tags: o.tags.clone(),
            })
            .collect())
    })
    .await?;
    Ok(Json(list))
}

#[derive(Debug, serde::Deserialize)]
struct PreviewQuery {
    anchor: Option<String>,
}

async fn preview(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> ApiResult<Json<Preview>> {
    let id = NoteId::new(id)?;
    let notes = s.notes.clone();
    Ok(Json(blocking(move || notes.preview(&id, q.anchor.as_deref())).await?))
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

#[derive(Debug, serde::Deserialize)]
struct NoteQuery {
    chapter: Option<usize>,
    anchor: Option<String>,
}

/// Заметка. С `chapter` или `anchor` книга приходит одной главой (с
/// оглавлением книги в `book`); не книга — целиком, как без них.
async fn note(State(s): State<AppState>, Path(id): Path<String>, Query(q): Query<NoteQuery>) -> ApiResult<Response> {
    let id = NoteId::new(id)?;
    let (notes, opts) = (s.notes.clone(), s.settings.figure_options());
    let by_chapter = q.chapter.is_some() || q.anchor.is_some();
    let page = blocking(move || {
        let page = notes.page(&id, opts)?;
        let select = Select { chapter: q.chapter, anchor: q.anchor.as_deref() };
        Ok(if by_chapter { chapter_page(&page, select).map(Arc::new) } else { None }.unwrap_or(page))
    })
    .await?;
    Ok(Json(page).into_response())
}

async fn version(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<VersionResponse>> {
    let id = NoteId::new(id)?;
    let (notes, opts) = (s.notes.clone(), s.settings.figure_options());
    let version = blocking(move || notes.version(&id, opts)).await?;
    Ok(Json(VersionResponse { version }))
}

async fn links(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<LinksResponse>> {
    let id = NoteId::new(id)?;
    let notes = s.notes.clone();
    blocking(move || {
        notes.vault().entry(&id)?;
        let snap = notes.index()?;
        let outgoing = snap
            .outgoing(&id)
            .iter()
            .map(|l| OutgoingLink {
                target: l.target.clone(),
                anchor: l.anchor.clone(),
                exists: snap.exists(&l.target),
            })
            .collect();
        Ok(Json(LinksResponse { outgoing, backlinks: snap.backlinks(&id) }))
    })
    .await
}

async fn graph(State(s): State<AppState>) -> ApiResult<Json<Graph>> {
    let notes = s.notes.clone();
    let graph = blocking(move || Ok(notes.index()?.graph())).await?;
    Ok(Json(graph))
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
        Err(errors) => (StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error: "не собралось".into(), errors }))
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
