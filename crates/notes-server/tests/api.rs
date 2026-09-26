//! API на тестовом хранилище `tests/vault`: коды ответов и проверка ввода.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use notes_core::settings::{Schema, SettingsStore};
use notes_core::{LibrarySource, Notes, NotesConfig};
use notes_server::{AppState, router};
use serde_json::Value;
use tower::ServiceExt;

static NOTES: LazyLock<Arc<Notes>> = LazyLock::new(|| {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::new(
        Notes::open(&NotesConfig {
            vault: repo.join("tests/vault"),
            library: LibrarySource::Dir(repo.join("baluk")),
            font_dirs: vec![],
            cache: None,
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
    assert_eq!(call(app, "GET", "/api/version/_baluk/lib", None).await.0, StatusCode::BAD_REQUEST);
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
async fn warm_hints_are_accepted() {
    let (app, _dir) = app();
    let (status, _) =
        call(app.clone(), "POST", "/api/warm", Some(r#"{"ids": ["Сеть/SSH", "../чужое", "Нет такой"]}"#)).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "неверные и неизвестные пути пропускаются");
    let (status, _) = call(app, "POST", "/api/warm", Some("[]")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn client_and_fonts_are_served() {
    let (app, _dir) = app();
    let res = app.clone().oneshot(Request::get(uri("/n/Сеть/SSH")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // Шрифты — по частям в WOFF2: `@font-face` на часть с `unicode-range`.
    let res = app.clone().oneshot(Request::get("/api/fonts.css").body(Body::empty()).unwrap()).await.unwrap();
    let css = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(css.contains(r#"url("/fonts/Gentium%20Plus/regular/cyrillic.woff2") format("woff2")"#), "{css}");
    assert!(css.contains("unicode-range: U+300-33F, U+342-36F, U+400-45F"));
    assert!(css.contains(r#"url("/fonts/New%20Computer%20Modern%20Math/regular/all.woff2")"#));
    let req = Request::get("/fonts/Gentium%20Plus/regular/cyrillic.woff2").header("accept-encoding", "br");
    let res = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "font/woff2");
    assert!(res.headers().get("content-encoding").is_none(), "WOFF2 не сжимается второй раз");
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..4], b"wOF2");
    for bad in ["/fonts/Comic%20Sans/regular/latin.woff2", "/fonts/Gentium%20Plus/regular/klingon.woff2"] {
        let res = app.clone().oneshot(Request::get(bad).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{bad}: раздаются только части шрифтов оформления");
    }
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
    let backlinks = links["backlinks"].as_array().unwrap();
    assert!(backlinks.iter().any(|b| b["from"] == "Сеть/UFW" && b["anchor"] == "Смена порта"), "{backlinks:?}");
    assert!(links["outgoing"].as_array().unwrap().iter().any(|l| l["target"] == "Сеть/UFW" && l["exists"] == true));

    let (status, graph) = call(app.clone(), "GET", "/api/graph", None).await;
    assert_eq!(status, StatusCode::OK);
    let missing: Vec<_> =
        graph["nodes"].as_array().unwrap().iter().filter(|n| n["kind"].is_null()).map(|n| &n["id"]).collect();
    assert_eq!(missing, ["Нет/Такой заметки", "Сеть/Nginx"]);
    assert!(graph["edges"].as_array().unwrap().iter().any(|e| e["from"] == "Сеть/UFW" && e["to"] == "Сеть/SSH"));

    // Граф по фильтру — уже разложенный: соседи SSH на шаг, без ненаписанных.
    let body = r#"{"around": "Сеть/SSH", "depth": 1, "missing": false}"#;
    let (status, layout) = call(app.clone(), "POST", "/api/graph/layout", Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    let nodes = layout["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(|n| n["id"] == "Сеть/UFW" && n["x"].is_number() && n["group"] == "Сеть"));
    assert!(nodes.iter().all(|n| !n["kind"].is_null()), "ненаписанные скрыты");
    assert_eq!(layout["center"], "Сеть/SSH");

    assert_eq!(call(app, "GET", &uri("/api/links/Нет/такой"), None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pdf_export() {
    let (app, _dir) = app();
    let res = app
        .clone()
        .oneshot(
            Request::get(format!("{}?theme={}", uri("/api/pdf/Сеть/SSH"), uri("night"))).body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "application/pdf");
    assert!(res.headers()["content-disposition"].to_str().unwrap().contains("SSH.pdf"));
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(
        call(app, "GET", &format!("{}?theme={}", uri("/api/pdf/Сеть/SSH"), uri("нет")), None).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn search_preview_and_note_meta() {
    let (app, _dir) = app();
    let (status, hits) = call(app.clone(), "GET", &format!("/api/search?q={}", uri("итоги второй")), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hits[0]["id"], "Книга");
    assert_eq!(hits[0]["anchor"], "Итоги-2");

    let (status, p) =
        call(app.clone(), "GET", &format!("{}?anchor={}", uri("/api/preview/Сеть/SSH"), uri("Смена порта")), None)
            .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p["heading"], "Смена порта");
    assert_eq!(call(app.clone(), "GET", &uri("/api/preview/Нет/такой"), None).await.0, StatusCode::NOT_FOUND);

    let (_, list) = call(app.clone(), "GET", "/api/notes", None).await;
    let book = list.as_array().unwrap().iter().find(|n| n["id"] == "Книга").unwrap();
    assert_eq!(book["title"], "Тестовая книга");
    assert_eq!(book["tags"][0], "книга");

    // Страница тегов — тот же клиент.
    let res = app.oneshot(Request::get(uri("/tags/сеть")).body(Body::empty()).unwrap()).await.unwrap();
    assert!(
        res.status() == StatusCode::OK || res.status() == StatusCode::SERVICE_UNAVAILABLE,
        "клиент или «не собран»"
    );
}

#[tokio::test]
async fn book_by_chapters() {
    let (app, _dir) = app();
    let get = |q: &'static str| {
        let app = app.clone();
        async move { call(app, "GET", &format!("{}{q}", uri("/api/notes/Книга")), None).await }
    };
    let (status, whole) = get("").await;
    assert_eq!(status, StatusCode::OK);
    assert!(whole["book"].is_null(), "без chapter — целиком");
    let whole_body = whole["rendered"]["body"].as_str().unwrap();
    assert!(whole_body.contains("id=\"гл-основы\"") && whole_body.contains("id=\"Приложение\""));

    let (status, second) = get("?chapter=1").await;
    assert_eq!(status, StatusCode::OK);
    let book = &second["book"];
    assert_eq!(book["chapter"], 1);
    let titles: Vec<_> = book["chapters"].as_array().unwrap().iter().map(|c| c["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["Основы", "Продолжение", "Приложение"]);
    assert_eq!(book["chapters"][2]["num"], "3");
    assert_eq!(book["anchors"]["особый"], 1);
    assert_eq!(book["anchors"]["Итоги-3"], 2);
    let body = second["rendered"]["body"].as_str().unwrap();
    assert!(body.contains("id=\"Продолжение\"") && !body.contains("id=\"гл-основы\"") && !body.contains("k-title"));
    assert_eq!(second["version"], whole["version"]);
    assert_eq!(second["rendered"]["headings"], whole["rendered"]["headings"], "оглавление — всей книги");

    // По якорю — его глава; неизвестный якорь — первая (с титулом).
    let (_, by_anchor) = get("?anchor=%D0%98%D1%82%D0%BE%D0%B3%D0%B8-3").await;
    assert_eq!(by_anchor["book"]["chapter"], 2);
    let (_, unknown) = get("?anchor=nope").await;
    assert_eq!(unknown["book"]["chapter"], 0);
    assert!(unknown["rendered"]["body"].as_str().unwrap().contains("k-title"));

    // Не книга — целиком и с chapter.
    let (_, note) = call(app.clone(), "GET", &format!("{}?chapter=1", uri("/api/notes/Сеть/SSH")), None).await;
    assert!(note["book"].is_null() && note["rendered"]["title"] == "SSH");
}
