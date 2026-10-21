//! The browser version on the hub (architecture §9): a folder built on the
//! computer (`tools/web-bundle.sh`) answered as it is, so the hub, which has no
//! Typst, serves the client, the core module and the theme stylesheets and
//! fonts that `notes serve` would answer.
//!
//! Only the fallback of the hub's router: the hub's own routes win. A GET
//! answers the file of that path, or `index.html` for a path that is not a file
//! (the client routes in JS) unless it is under a prefix that only holds files
//! ([`FILE_PREFIXES`]): there a missing file is a 404.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use notes_store::serve_rules::{CLIENT_CSP, is_hashed};
use percent_encoding::percent_decode_str;
use tokio_util::io::ReaderStream;

/// Folders of the bundle that hold files only: a path under them that is not a
/// file is a 404, not the client page.
const FILE_PREFIXES: [&str; 4] = ["api", "assets", "wasm", "fonts"];

/// The stylesheets the page loads before any sign-in: the only files under
/// `/api/` that need no session.
const PUBLIC_API_FILES: [&str; 2] = ["/api/fonts.css", "/api/themes.css"];

/// Does a request need no session: a GET of the page, a file or a stylesheet.
/// Everything under `/api/` but the two stylesheets stays behind sign-in.
pub(crate) fn is_public(method: &Method, path: &str) -> bool {
    *method == Method::GET && (!path.starts_with("/api/") || PUBLIC_API_FILES.contains(&path))
}

/// The folder being served.
#[derive(Debug)]
pub(crate) struct Web {
    /// Canonical: a file counts as inside it after its own symlinks are resolved.
    root: PathBuf,
}

/// What a request path comes to.
#[derive(Debug, PartialEq, Eq)]
enum Target {
    /// A file inside the folder: its path and the path below the folder, `/`-separated.
    File(PathBuf, String),
    /// Nothing there.
    Missing,
    /// Not a path of the folder (`..`) or a link out of it.
    Refused(StatusCode),
}

impl Web {
    /// The folder of a bundle.
    ///
    /// # Errors
    /// It is not a folder, or has no `index.html` (the message says how to build it).
    pub(crate) fn open(dir: &Path) -> io::Result<Arc<Self>> {
        let root = dir.canonicalize().map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", dir.display())))?;
        let web = Self { root };
        if !matches!(web.resolve("index.html"), Target::File(..)) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{}: no index.html; build the folder with tools/web-bundle.sh", dir.display()),
            ));
        }
        Ok(Arc::new(web))
    }

    /// The answer to a request nothing else of the hub took. The body is
    /// streamed from the disk: the module is tens of MB and the server is small.
    pub(crate) async fn respond(self: Arc<Self>, method: Method, uri: Uri) -> Response {
        if method != Method::GET && method != Method::HEAD {
            return StatusCode::NOT_FOUND.into_response();
        }
        let path = uri.path().to_owned();
        let located = match tokio::task::spawn_blocking(move || self.locate(&path)).await {
            Ok(Ok(located)) => located,
            Ok(Err(status)) => return status.into_response(),
            Err(e) => {
                tracing::error!("web: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        let (path, rel) = located;
        let opened = async {
            let file = tokio::fs::File::open(&path).await?;
            let len = file.metadata().await?.len();
            Ok::<_, io::Error>((file, len))
        };
        match opened.await {
            Ok((file, len)) => served(&rel, len, Body::from_stream(ReaderStream::new(file))),
            Err(e) => {
                tracing::error!("web: {}: {e}", path.display());
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }

    /// The file to answer for a request path: its own, or the client page.
    fn locate(&self, uri_path: &str) -> Result<(PathBuf, String), StatusCode> {
        let Ok(decoded) = percent_decode_str(uri_path).decode_utf8() else {
            return Err(StatusCode::BAD_REQUEST);
        };
        let first = decoded.trim_start_matches('/').split('/').next().unwrap_or_default();
        let target = match self.resolve(&decoded) {
            Target::Missing if !FILE_PREFIXES.contains(&first) => self.resolve("index.html"),
            other => other,
        };
        match target {
            Target::File(path, rel) => Ok((path, rel)),
            Target::Missing => Err(StatusCode::NOT_FOUND),
            Target::Refused(status) => Err(status),
        }
    }

    /// The file of a request path (decoded, `/`-separated).
    fn resolve(&self, request_path: &str) -> Target {
        let mut parts = Vec::new();
        for part in request_path.split('/').filter(|p| !p.is_empty()) {
            if matches!(part, "." | "..") || part.contains(['\\', '\0']) {
                return Target::Refused(StatusCode::BAD_REQUEST);
            }
            parts.push(part);
        }
        let rel = if parts.is_empty() { "index.html".to_owned() } else { parts.join("/") };
        let Ok(path) = self.root.join(&rel).canonicalize() else { return Target::Missing };
        if !path.starts_with(&self.root) {
            return Target::Refused(StatusCode::NOT_FOUND);
        }
        if path.is_file() { Target::File(path, rel) } else { Target::Missing }
    }
}

/// The answer for the file `rel` (below the folder, `/`-separated) of `len` bytes.
fn served(rel: &str, len: u64, body: Body) -> Response {
    // Hashed build files never change; the rest (the page, the module, the
    // stylesheets) can be replaced by a deployment.
    let cache =
        if rel.starts_with("assets/") && is_hashed(rel) { "public, max-age=31536000, immutable" } else { "no-cache" };
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type(rel))),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
            (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CLIENT_CSP)),
            (header::CONTENT_LENGTH, HeaderValue::from(len)),
        ],
        body,
    )
        .into_response()
}

/// The type by extension: what a bundle holds.
fn content_type(rel: &str) -> &'static str {
    match rel.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("wasm") => "application/wasm",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_by_extension() {
        assert_eq!(content_type("wasm/notes_wasm_bg.wasm"), "application/wasm");
        assert_eq!(content_type("assets/App-AbCd1234.JS"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("fonts/a/b/c.woff2"), "font/woff2");
        assert_eq!(content_type("noext"), "application/octet-stream");
    }

    #[test]
    fn public_requests() {
        assert!(is_public(&Method::GET, "/"));
        assert!(is_public(&Method::GET, "/v/x/n/y"));
        assert!(is_public(&Method::GET, "/api/themes.css"));
        assert!(!is_public(&Method::GET, "/api/sync/vaults"));
        assert!(!is_public(&Method::POST, "/n/y"));
    }
}
