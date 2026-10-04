//! The client: the shell page on every interface URL and the build files
//! `app/dist/assets` (kept by `notes-assets`).

use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

use crate::AppState;

/// The page shown instead of the client when it is not built.
const NO_CLIENT: &str = "<!doctype html><meta charset=utf-8><title>Client not built</title>\
<p>The client is not built. Run in the project directory:</p>\
<pre>npm --prefix app ci &amp;&amp; npm --prefix app run build</pre>\
<p>and reload the page. To develop the client: <code>npm --prefix app run dev</code>.</p>";

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(shell))
        .route("/v/{vault}", get(shell))
        .route("/v/{vault}/", get(shell))
        .route("/v/{vault}/n/{*id}", get(shell))
        .route("/v/{vault}/f/{*path}", get(shell))
        .route("/v/{vault}/graph", get(shell))
        .route("/v/{vault}/tags", get(shell))
        .route("/v/{vault}/tags/{*tag}", get(shell))
        // URLs without a vault (older ones, links from note HTML): the client
        // opens them in the default vault.
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
    // Build files with a hash in the name (index-CX38oHQo.js) never change.
    let cache = if is_hashed(&path) { "public, max-age=31536000, immutable" } else { "no-cache" };
    ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache.into())], file.data).into_response()
}

/// `name-XXXXXXXX.ext`: Vite puts 8 characters of the content hash in the name.
fn is_hashed(path: &str) -> bool {
    let stem = path.rsplit_once('.').map_or(path, |(s, _)| s);
    stem.rsplit_once('-')
        .is_some_and(|(_, h)| h.len() == 8 && h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

/// A CSS answer that may change (themes, fonts): no long caching.
pub(crate) fn css(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

/// The CSP for the `notes serve` client: strict, no inline or eval scripts.
fn csp() -> &'static str {
    // No unsafe-inline/-eval: scripts come only from files; styles from files
    // and inline (MathML <style>); style attributes are allowed by default.
    // Images: self and data: (SVG inserts from Typst). Fonts: self.
    // No dev-server worker (serve does not use Vite).
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
