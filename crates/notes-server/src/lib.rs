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
//! | `GET /`, `/n/{*id}`, `/tags…`, `/graph` | клиент (одна страница, маршрутизация в JS) |
//! | `GET /assets/{*path}`        | файлы клиента (сборка `app/dist/assets`)     |
//! | `GET /api/notes`             | список заметок и книг                        |
//! | `GET /api/notes/{*id}`       | заметка: HTML, заголовки, ссылки, ошибки     |
//! | `…?chapter=N`, `…?anchor=`   | книга — одной главой (N-й или с якорем)      |
//! | `GET /api/version/{*id}`     | версия заметки — дёшево, без компиляции      |
//! | `GET /api/links/{*id}`       | ссылки заметки и обратные ссылки на неё      |
//! | `GET /api/graph`             | граф заметок: узлы и рёбра                   |
//! | `POST /api/graph/layout`     | граф по фильтру, разложенный (`notes_core::vault_graph`) |
//! | `GET /api/search?q=&limit=`  | поиск по тексту всех заметок                 |
//! | `GET /api/preview/{*id}?anchor=` | превью заметки/раздела (без компиляции)  |
//! | `GET /api/pdf/{*id}?theme=`  | заметка в PDF (по умолчанию — первая тема)   |
//! | `GET /api/settings`          | схема и значения настроек                    |
//! | `PUT /api/settings`          | изменить настройки (частично)                |
//! | `POST /api/warm`             | что собрать заранее первым (см. `notes_core::warm`) |
//! | `GET /api/themes`            | темы: имя, название, тёмная ли               |
//! | `GET /api/themes.css`        | CSS-переменные тем                           |
//! | `GET /api/fonts.css`         | `@font-face` для шрифтов оформления (по частям) |
//! | `GET /fonts/{family}/{style}/{part}.woff2` | часть шрифта (WOFF2, набор знаков) |
//!
//! Модули — по областям: `notes` (заметки, PDF, прогрев), `graph`,
//! `search`, `settings` (и темы), `assets` (клиент), `fonts`; общее —
//! [`AppState`] и `error`. Со [`AppState::token`] все пути требуют токен
//! (`auth`: заголовок, `?token=` или cookie).

pub mod api;
mod assets;
mod auth;
mod error;
mod fonts;
mod graph;
mod notes;
mod search;
mod settings;

use std::sync::Arc;

use axum::Router;
use notes_core::Notes;
use notes_core::settings::SettingsStore;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};

/// Общее состояние обработчиков.
#[derive(Debug, Clone)]
pub struct AppState {
    pub notes: Arc<Notes>,
    pub settings: Arc<SettingsStore>,
    /// Токен доступа: если задан, без него сервер отвечает 401 (см. `auth`).
    pub token: Option<Arc<str>>,
}

impl AppState {
    /// Состояние без токена.
    pub fn new(notes: Arc<Notes>, settings: Arc<SettingsStore>) -> Self {
        Self { notes, settings, token: None }
    }

    /// С токеном доступа; пустая строка — как без токена.
    #[must_use]
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token.filter(|t| !t.is_empty()).map(Into::into);
        self
    }
}

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .merge(assets::routes())
        .merge(notes::routes())
        .merge(graph::routes())
        .merge(search::routes())
        .merge(settings::routes())
        .merge(fonts::routes());
    if let Some(token) = state.token.clone() {
        app = app.layer(axum::middleware::from_fn_with_state(token, auth::require_token));
    }
    // WOFF2 уже сжат brotli — второй раз не жать.
    app.layer(
        CompressionLayer::new().compress_when(DefaultPredicate::new().and(NotForContentType::const_new("font/woff2"))),
    )
    .with_state(state)
}

/// Запускает сервер на готовом сокете до сигнала `shutdown`.
pub async fn serve(
    listener: tokio::net::TcpListener,
    state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    // Шрифты для браузера сжимаются в фоне заранее: иначе первая страница
    // ждала бы сжатия (математический шрифт — ~2 с).
    let notes = state.notes.clone();
    std::thread::spawn(move || notes.fonts().warm_web(notes.themes().web_fonts()));
    // Заметки — тоже заранее: все, по приоритету, пропуская собранные.
    let (notes, settings) = (state.notes.clone(), state.settings.clone());
    std::thread::spawn(move || notes.warm_forever(|| settings.figure_options()));
    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown).await
}
