use axum::body::Body;
use axum::http::Request;
use notes_core::Notes;
use notes_core::settings::{Schema, SettingsStore};
use notes_server::{AppState, VaultSet, router};
use notes_store::names::VaultName;
use notes_store::platform::Platform;
use std::sync::Arc;
use tower::ServiceExt;

use crate::common::NOTES;

fn app() -> (axum::Router, tempfile::TempDir) {
    fn single(notes: Arc<Notes>, dir: &tempfile::TempDir) -> AppState {
        let schema = Schema::new(notes.themes().themes(), Platform::Desktop);
        let settings = Arc::new(SettingsStore::open(dir.path().join("settings.json"), schema).unwrap());
        AppState::new(VaultSet::single(VaultName::new("test").unwrap(), notes, settings))
    }
    let dir = tempfile::tempdir().unwrap();
    (router(single(NOTES.clone(), &dir)), dir)
}

#[tokio::test]
async fn shell_has_csp() {
    let (app, _dir) = app();
    let res = app.clone().oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
    assert!(
        res.status() == axum::http::StatusCode::OK || res.status() == axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "the client is built (200) or not (503)"
    );
    let csp = res.headers().get("content-security-policy").unwrap().to_str().unwrap();
    assert!(csp.contains("script-src 'self' 'wasm-unsafe-eval';"));
    assert!(!csp.contains("'unsafe-eval'"), "no eval of JS");
    assert!(!csp.contains("worker-src"), "workers fall back to script-src");
    assert!(csp.contains("object-src 'none'"));
    assert!(csp.contains("base-uri 'none'"));
}

/// `assets/config.js` is the setting the client reads first (the hub's browser
/// version replaces it): served like any asset, never cached for good.
#[tokio::test]
async fn config_js_is_an_asset() {
    let (app, _dir) = app();
    let res = app.oneshot(Request::get("/assets/config.js").body(Body::empty()).unwrap()).await.unwrap();
    if res.status() == axum::http::StatusCode::NOT_FOUND {
        // The client is not built (`npm run build` makes it; the full check builds it first).
        assert!(notes_assets::index_html().is_none(), "the client is built but config.js is missing");
        return;
    }
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    assert_eq!(res.headers()["cache-control"], "no-cache");
    assert!(res.headers()["content-type"].to_str().unwrap().contains("javascript"));
}
