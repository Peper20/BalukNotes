//! Клиент: страница-оболочка на всех адресах интерфейса и файлы сборки
//! `app/dist/assets`.

use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

use crate::AppState;

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

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(shell))
        .route("/n/{*id}", get(shell))
        .route("/graph", get(shell))
        .route("/tags", get(shell))
        .route("/tags/{*tag}", get(shell))
        .route("/assets/{*path}", get(asset))
}

/// Файл клиента (`baluk.css`, `static.js`, …) — для статической сборки.
pub fn web_asset(name: &str) -> Option<Vec<u8>> {
    WebAssets::get(&format!("assets/{name}")).map(|f| f.data.into_owned())
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

/// Ответ CSS, который может поменяться (темы, шрифты): без долгого кэша.
pub(crate) fn css(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}
