use axum::body::Body;
use axum::http::Request;
use notes_core::settings::{Platform, Schema, SettingsStore};
use notes_core::{Notes, VaultName};
use notes_server::{AppState, VaultSet, router};
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
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("object-src 'none'"));
    assert!(csp.contains("base-uri 'none'"));
}
