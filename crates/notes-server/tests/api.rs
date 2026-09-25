//! API на тестовом хранилище `examples/vault`: коды ответов и проверка ввода.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use notes_core::settings::{Schema, SettingsStore};
use notes_core::{Notes, NotesConfig};
use notes_server::{AppState, router};
use serde_json::Value;
use tower::ServiceExt;

static NOTES: LazyLock<Arc<Notes>> = LazyLock::new(|| {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::new(
        Notes::open(&NotesConfig {
            vault: repo.join("examples/vault"),
            library: repo.join("konspekt"),
            font_dirs: vec![],
        })
        .unwrap(),
    )
});

/// Своё хранилище настроек на тест: запись идёт во временный каталог.
fn app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let settings = SettingsStore::open(dir.path().join("settings.json"), Schema::new(NOTES.themes().themes())).unwrap();
    (router(AppState { notes: NOTES.clone(), settings: Arc::new(settings) }), dir)
}

async fn call(app: axum::Router, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let res = app.oneshot(req.body(Body::from(body.unwrap_or("").to_owned())).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Кириллица в пути — как её шлёт браузер.
fn uri(path: &str) -> String {
    path.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-_.".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[tokio::test]
async fn note_list_and_page() {
    let (app, _dir) = app();
    let (status, list) = call(app.clone(), "GET", "/api/notes", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().unwrap().iter().any(|n| n["id"] == "Сеть/SSH" && n["kind"] == "note"));

    let (status, page) = call(app, "GET", &uri("/api/notes/Сеть/SSH"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["rendered"]["title"], "SSH");
    assert!(page["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn bad_paths_are_rejected() {
    let (app, _dir) = app();
    assert_eq!(call(app.clone(), "GET", &uri("/api/notes/Нет/такой"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(app.clone(), "GET", "/api/notes/..%2Fetc", None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(app, "GET", "/api/version/_konspekt/lib", None).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn settings_are_validated() {
    let (app, _dir) = app();
    let (status, _) = call(app.clone(), "PUT", "/api/settings", Some(r#"{"appearance.font_size": 99}"#)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, values) = call(app.clone(), "PUT", "/api/settings", Some(r#"{"refresh.interval": 0}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(values["refresh.interval"], 0);
    let (_, all) = call(app, "GET", "/api/settings", None).await;
    assert!(all["schema"]["settings"].as_array().unwrap().iter().any(|s| s["key"] == "headings.numbering"));
}

#[tokio::test]
async fn client_and_fonts_are_served() {
    let (app, _dir) = app();
    let res = app.clone().oneshot(Request::get(uri("/n/Сеть/SSH")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res =
        app.clone().oneshot(Request::get("/fonts/Gentium%20Plus/regular").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app.oneshot(Request::get("/fonts/Comic%20Sans/regular").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND, "раздаются только шрифты оформления");
}

#[tokio::test]
async fn responses_are_compressed() {
    let (app, _dir) = app();
    let req =
        Request::get(uri("/api/notes/демо/визуализация")).header("accept-encoding", "br, gzip").body(Body::empty());
    let res = app.oneshot(req.unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-encoding"], "br");
}

#[tokio::test]
async fn links_and_graph() {
    let (app, _dir) = app();
    let (status, links) = call(app.clone(), "GET", &uri("/api/links/Сеть/SSH"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(links["backlinks"][0]["from"], "Сеть/UFW");
    assert_eq!(links["backlinks"][0]["anchor"], "Смена порта");
    assert!(links["outgoing"].as_array().unwrap().iter().any(|l| l["target"] == "Сеть/UFW" && l["exists"] == true));

    let (status, graph) = call(app.clone(), "GET", "/api/graph", None).await;
    assert_eq!(status, StatusCode::OK);
    let missing: Vec<_> = graph["nodes"].as_array().unwrap().iter().filter(|n| n["kind"].is_null()).collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0]["id"], "Сеть/Nginx");
    assert!(graph["edges"].as_array().unwrap().iter().any(|e| e["from"] == "Сеть/UFW" && e["to"] == "Сеть/SSH"));

    assert_eq!(call(app, "GET", &uri("/api/links/Нет/такой"), None).await.0, StatusCode::NOT_FOUND);
}
