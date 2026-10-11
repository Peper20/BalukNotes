//! What opens outside the window: external links and a note's PDF go to system
//! programs.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Url;

use crate::proxy;

/// The window's URL scheme.
pub const SCHEME: &str = "notes";

/// The app's own addresses open in a window; nothing else does.
pub fn allowed(url: &Url) -> bool {
    url.scheme() == SCHEME || url.scheme() == "about"
}

/// An address of a note's PDF.
pub fn is_pdf(url: &Url) -> bool {
    url.scheme() == SCHEME && url.path().contains("/pdf/")
}

/// An external link opens in a system program (browser, mail); the window stays.
pub fn open_external(url: &Url) -> bool {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        system_open(url.as_str());
    } else {
        tracing::warn!("link {url} not opened: unsupported scheme");
    }
    false
}

/// A note's PDF: WebKitGTK does not show it, so build it through the core, save
/// it to a temporary directory and open it in a system program.
pub fn open_pdf(socket: Arc<PathBuf>, url: Url) {
    tauri::async_runtime::spawn(async move {
        let name = url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned())
            .unwrap_or_default();
        let request = match tauri::http::Request::get(url.as_str()).body(Vec::new()) {
            Ok(request) => request,
            Err(e) => {
                tracing::warn!("PDF \"{name}\": bad address {url}: {e}");
                return;
            }
        };
        let response = proxy::forward(&socket, request).await;
        if !response.status().is_success() {
            let body = String::from_utf8_lossy(response.body());
            tracing::warn!("PDF \"{name}\" not built: {} {body}", response.status());
            return;
        }
        let Some(dir) = notes::default_socket().and_then(|s| s.parent().map(|d| d.join("pdf"))) else { return };
        let path = dir.join(format!("{}.pdf", safe_name(&name)));
        let written = async {
            tokio::fs::create_dir_all(&dir).await?;
            tokio::fs::write(&path, response.body()).await
        };
        match written.await {
            Ok(()) => system_open(&path.to_string_lossy()),
            Err(e) => tracing::warn!("PDF {}: {e}", path.display()),
        }
    });
}

/// A file name without path separators.
fn safe_name(name: &str) -> String {
    let name: String = name.chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '_' } else { c }).collect();
    if name.trim().is_empty() { "note".into() } else { name }
}

fn system_open(target: &str) {
    if let Err(e) = std::process::Command::new("xdg-open").arg(target).spawn() {
        tracing::warn!("xdg-open {target}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_name("a/b\\c"), "a_b_c");
        assert_eq!(safe_name("  "), "note");
        assert_eq!(safe_name("Заметка"), "Заметка");
    }

    #[test]
    fn urls_are_told_apart() {
        let url = |s: &str| s.parse::<Url>().unwrap();
        assert!(allowed(&url("notes://localhost/v/A/")));
        assert!(allowed(&url("about:blank")));
        assert!(!allowed(&url("https://example.com/")));
        assert!(is_pdf(&url("notes://localhost/api/vaults/A/pdf/x.pdf")));
        assert!(!is_pdf(&url("notes://localhost/v/A/n/x")));
        assert!(!is_pdf(&url("https://example.com/pdf/x")));
    }
}
