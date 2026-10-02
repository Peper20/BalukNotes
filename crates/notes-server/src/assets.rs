//! Клиент: страница-оболочка на всех адресах интерфейса и файлы сборки
//! `app/dist/assets` (их хранит `notes-assets`).

use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

use crate::AppState;

/// Страница вместо клиента, если он не собран.
const NO_CLIENT: &str = "<!doctype html><meta charset=utf-8><title>Клиент не собран</title>\
<p>Клиент не собран. Выполните в каталоге проекта:</p>\
<pre>npm --prefix app ci &amp;&amp; npm --prefix app run build</pre>\
<p>и обновите страницу. Для разработки клиента — <code>npm --prefix app run dev</code>.</p>";

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(shell))
        .route("/v/{vault}", get(shell))
        .route("/v/{vault}/", get(shell))
        .route("/v/{vault}/n/{*id}", get(shell))
        .route("/v/{vault}/graph", get(shell))
        .route("/v/{vault}/tags", get(shell))
        .route("/v/{vault}/tags/{*tag}", get(shell))
        // Адреса без хранилища (прежние, ссылки из HTML заметок): клиент
        // откроет их в хранилище по умолчанию.
        .route("/n/{*id}", get(shell))
        .route("/graph", get(shell))
        .route("/tags", get(shell))
        .route("/tags/{*tag}", get(shell))
        .route("/assets/{*path}", get(asset))
}

async fn shell() -> Response {
    match notes_assets::index_html() {
        Some(file) => {
            ([(header::CACHE_CONTROL, "no-cache"), (header::CONTENT_SECURITY_POLICY, csp())], Html(file.data))
                .into_response()
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CACHE_CONTROL, "no-cache"), (header::CONTENT_SECURITY_POLICY, csp())],
            Html(NO_CLIENT),
        )
            .into_response(),
    }
}

async fn asset(Path(path): Path<String>) -> Response {
    let Some(file) = notes_assets::asset(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = file.mime;
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

/// Ответ CSS, который может поменяться (темы, шрифты): без долгого кэша.
pub(crate) fn css(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

/// CSP для клиента `notes serve` — строгая, без inline/eval для скриптов.
fn csp() -> &'static str {
    // Без unsafe-inline/-eval: скрипты — только из файлов; стили — из файлов
    // и inline (<style> MatML), style-attr — по умолчанию разрешён.
    // Изображения — self и data: (вставки SVG из Typst). Шрифты — self.
    // Воркер dev-сервера не нужен (serve не использует Vite).
    concat!(
        "default-src 'self';",
        "script-src 'self';",
        "object-src 'none';",
        "base-uri 'none';",
        "img-src 'self' data: blob:;",
        "style-src 'self' 'unsafe-inline';",
        "font-src 'self';",
        "connect-src 'self';",
        "frame-ancestors 'self'"
    )
}
