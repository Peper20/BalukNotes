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
//! | `GET /`, `/v/{хранилище}/…` (`n/{*id}`, `tags…`, `graph`) | клиент (одна страница, маршрутизация в JS) |
//! | `GET /assets/{*path}`        | файлы клиента (сборка `app/dist/assets`)     |
//! | `GET /api/vaults`            | хранилища и какое открыть по умолчанию       |
//! | `POST /api/vaults`           | создать хранилище `{ name }`                 |
//! | `GET /api/settings`          | схема и значения настроек                    |
//! | `PUT /api/settings`          | изменить настройки (частично)                |
//! | `GET /api/themes`            | темы: имя, название, тёмная ли               |
//! | `GET /api/themes.css`        | CSS-переменные тем                           |
//! | `GET /api/fonts.css`         | `@font-face` для шрифтов оформления (по частям) |
//! | `GET /fonts/{family}/{style}/{part}.woff2` | часть шрифта (WOFF2, набор знаков) |
//!
//! Хранилище — под `/api/vaults/{хранилище}` (дальше — `…`):
//!
//! | Путь                         | Что                                          |
//! |------------------------------|----------------------------------------------|
//! | `GET …/notes`                | список заметок и книг                        |
//! | `GET …/folders`              | папки с названиями (`_folder.toml`)          |
//! | `GET …/notes/{*id}`          | заметка: HTML, заголовки, ссылки, ошибки     |
//! | `…?chapter=N`, `…?anchor=`   | книга — одной главой (N-й или с якорем)      |
//! | `DELETE …/notes/{*id}`       | заметку (книгу — папкой) в корзину           |
//! | `GET …/version/{*id}`        | версия заметки — дёшево, без компиляции      |
//! | `GET …/links/{*id}`          | ссылки заметки и обратные ссылки на неё      |
//! | `GET …/graph`                | граф заметок: узлы и рёбра                   |
//! | `POST …/graph/layout`        | граф по фильтру, разложенный (`notes_core::vault_graph`) |
//! | `GET …/search?q=&limit=`     | поиск по тексту всех заметок                 |
//! | `GET …/preview/{*id}?anchor=` | превью заметки/раздела (без компиляции)     |
//! | `GET …/pdf/{*id}?theme=`     | заметка в PDF (по умолчанию — первая тема)   |
//! | `POST …/warm`                | что собрать заранее первым (см. `notes_core::warm`) |
//! | `GET …/events`               | события: файлы изменились (SSE, см. `events`) |
//!
//! Модули — по областям: `vaults` (хранилища, открытые сервером), `notes`
//! (заметки, PDF, прогрев, удаление), `graph`, `search`, `settings` (и
//! темы), `assets` (клиент), `fonts`, `events` (изменения хранилища);
//! общее — [`AppState`] и `error`. Со [`AppState::token`] все пути требуют токен
//! (`auth`: заголовок, `?token=` или cookie).

pub mod api;
mod assets;
mod auth;
mod error;
mod events;
mod fonts;
mod graph;
mod notes;
mod search;
mod settings;
mod vaults;

use std::sync::Arc;

use axum::Router;
use notes_core::settings::SettingsStore;
use tokio::sync::watch;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};

pub use vaults::{OpenVault, VaultSet};

/// Общее состояние обработчиков.
#[derive(Debug, Clone)]
pub struct AppState {
    pub vaults: Arc<VaultSet>,
    pub settings: Arc<SettingsStore>,
    /// Токен доступа: если задан, без него сервер отвечает 401 (см. `auth`).
    pub token: Option<Arc<str>>,
    /// Сервер останавливается: потоки событий закрываются.
    pub closing: Arc<watch::Sender<bool>>,
}

impl AppState {
    /// Состояние без токена. Изменения хранилища приходят в события
    /// хранилища, когда наблюдатель включён (`Notes::watch`, это делает
    /// [`serve`]). Настройки устройства сразу применяются к ядру.
    pub fn new(vaults: VaultSet) -> Self {
        vaults.apply_device();
        let settings = vaults.settings().clone();
        Self { vaults: Arc::new(vaults), settings, token: None, closing: Arc::new(watch::Sender::new(false)) }
    }

    /// Хранилище из адреса; ещё не открыто — открыть (в отдельном потоке).
    pub(crate) async fn vault(&self, name: String) -> error::ApiResult<Arc<OpenVault>> {
        if let Some(open) = self.vaults.opened(&name) {
            return Ok(open);
        }
        let vaults = self.vaults.clone();
        error::blocking(move || vaults.get(&name)).await
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
        .merge(vaults::routes())
        .merge(notes::routes())
        .merge(graph::routes())
        .merge(search::routes())
        .merge(settings::routes())
        .merge(fonts::routes())
        .merge(events::routes());
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
    // ждала бы сжатия (математический шрифт — ~2 с). Шрифты и темы у всех
    // хранилищ общие (библиотека одна).
    let library = state.vaults.library().clone();
    std::thread::spawn(move || library.warm_fonts());
    // Прогрев и наблюдатель файлов — у каждого открытого хранилища.
    state.vaults.start_background();
    let closing = state.closing.clone();
    let shutdown = async move {
        shutdown.await;
        closing.send_replace(true);
    };
    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown).await
}
