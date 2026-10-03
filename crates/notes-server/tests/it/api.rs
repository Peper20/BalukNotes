//! API на тестовом хранилище `tests/vault`: коды ответов и проверка ввода.

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use notes_core::settings::{Platform, Schema, SettingsStore};
use notes_core::storage::MemStorage;
use notes_core::{LibrarySource, Notes, NotesConfig, VaultName, Vaults};
use notes_server::{AppState, VaultSet, router};
use serde_json::Value;
use tower::ServiceExt;

use crate::common::NOTES;

/// Настройки во временном каталоге `dir`.
fn settings(dir: &tempfile::TempDir) -> Arc<SettingsStore> {
    let schema = Schema::new(NOTES.themes().themes(), Platform::Desktop);
    Arc::new(SettingsStore::open(dir.path().join("settings.json"), schema).unwrap())
}

/// Сервер на одном хранилище `test`.
fn single(notes: Arc<Notes>, dir: &tempfile::TempDir) -> AppState {
    AppState::new(VaultSet::single(VaultName::new("test").unwrap(), notes, settings(dir)))
}

/// Своё хранилище настроек на тест: запись идёт во временный каталог.
fn app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (router(single(NOTES.clone(), &dir)), dir)
}

/// То же с токеном доступа.
fn app_with_token(token: &str) -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (router(single(NOTES.clone(), &dir).with_token(Some(token.into()))), dir)
}

/// Настройки ядра для хранилища `vault` (библиотека — из репозитория).
fn config(vault: PathBuf, trash: Option<PathBuf>) -> NotesConfig {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    NotesConfig { vault, library: LibrarySource::Dir(repo.join("baluk")), font_dirs: vec![], cache: None, trash }
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
    let (status, list) = call(app.clone(), "GET", "/api/vaults/test/notes", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().unwrap().iter().any(|n| n["id"] == "Сеть/SSH" && n["kind"] == "note"));
    let odd = list.as_array().unwrap().iter().find(|n| n["id"] == "Имена/странное").unwrap();
    assert_eq!(odd["name"], "странное");
    assert_eq!(odd["title"], r"@#$@&$*%@#!.:/\ — в названии можно всё", "название — из файла");

    // Папки: название из `_folder.toml`, иначе (и при ошибке в нём) — имя.
    let (status, folders) = call(app.clone(), "GET", "/api/vaults/test/folders", None).await;
    assert_eq!(status, StatusCode::OK);
    let title = |path: &str| folders.as_array().unwrap().iter().find(|f| f["path"] == path).map(|f| f["title"].clone());
    assert_eq!(title("Имена").unwrap(), "Имена: файлы / названия");
    assert_eq!(title("Сеть").unwrap(), "Сеть");
    assert_eq!(title("Глубоко/а/б").unwrap(), "б");
    assert_eq!(title("Книга"), None, "книга — не папка");
    assert_eq!(title("_служебное"), None);

    let (status, page) = call(app, "GET", &uri("/api/vaults/test/notes/Сеть/SSH"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["rendered"]["title"], "SSH");
    assert!(page["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn bad_paths_are_rejected() {
    let (app, _dir) = app();
    assert_eq!(call(app.clone(), "GET", &uri("/api/vaults/test/notes/Нет/такой"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(app.clone(), "GET", "/api/vaults/test/notes/..%2Fetc", None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(app, "GET", "/api/vaults/test/version/_baluk/lib", None).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn settings_are_validated() {
    let (app, _dir) = app();
    let (status, _) = call(app.clone(), "PUT", "/api/settings", Some(r#"{"appearance.font_size": 99}"#)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, values) = call(app.clone(), "PUT", "/api/settings", Some(r#"{"refresh.mode": "manual"}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(values["refresh.mode"], "manual");
    let (_, all) = call(app, "GET", "/api/settings", None).await;
    assert!(all["schema"]["settings"].as_array().unwrap().iter().any(|s| s["key"] == "headings.numbering"));
}

#[tokio::test]
async fn warm_hints_are_accepted() {
    let (app, _dir) = app();
    let (status, _) =
        call(app.clone(), "POST", "/api/vaults/test/warm", Some(r#"{"ids": ["Сеть/SSH", "../чужое", "Нет такой"]}"#))
            .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "неверные и неизвестные пути пропускаются");
    let (status, _) = call(app, "POST", "/api/vaults/test/warm", Some("[]")).await;
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
    let req = Request::get(uri("/api/vaults/test/notes/демо/визуализация"))
        .header("accept-encoding", "br, gzip")
        .body(Body::empty());
    let res = app.oneshot(req.unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-encoding"], "br");
}

#[tokio::test]
async fn links_and_graph() {
    let (app, _dir) = app();
    let (status, links) = call(app.clone(), "GET", &uri("/api/vaults/test/links/Сеть/SSH"), None).await;
    assert_eq!(status, StatusCode::OK);
    let backlinks = links["backlinks"].as_array().unwrap();
    assert!(backlinks.iter().any(|b| b["from"] == "Сеть/UFW" && b["anchor"] == "Смена порта"), "{backlinks:?}");
    assert!(links["outgoing"].as_array().unwrap().iter().any(|l| l["target"] == "Сеть/UFW" && l["exists"] == true));

    let (status, graph) = call(app.clone(), "GET", "/api/vaults/test/graph", None).await;
    assert_eq!(status, StatusCode::OK);
    let missing: Vec<_> =
        graph["nodes"].as_array().unwrap().iter().filter(|n| n["kind"].is_null()).map(|n| &n["id"]).collect();
    assert_eq!(missing, ["Нет/Из несобравшейся", "Нет/Такой заметки", "Сеть/Nginx"]);
    assert!(graph["edges"].as_array().unwrap().iter().any(|e| e["from"] == "Сеть/UFW" && e["to"] == "Сеть/SSH"));

    // Граф по фильтру — уже разложенный: соседи SSH на шаг, без ненаписанных.
    let body = r#"{"around": "Сеть/SSH", "depth": 1, "missing": false}"#;
    let (status, layout) = call(app.clone(), "POST", "/api/vaults/test/graph/layout", Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    let nodes = layout["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(|n| n["id"] == "Сеть/UFW" && n["x"].is_number() && n["group"] == "Сеть"));
    assert!(nodes.iter().all(|n| !n["kind"].is_null()), "ненаписанные скрыты");
    assert_eq!(layout["center"], "Сеть/SSH");

    assert_eq!(call(app, "GET", &uri("/api/vaults/test/links/Нет/такой"), None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pdf_export() {
    let (app, _dir) = app();
    let res = app
        .clone()
        .oneshot(
            Request::get(format!("{}?theme={}", uri("/api/vaults/test/pdf/Сеть/SSH"), uri("night")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "application/pdf");
    assert!(res.headers()["content-disposition"].to_str().unwrap().contains("SSH.pdf"));
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(
        call(app, "GET", &format!("{}?theme={}", uri("/api/vaults/test/pdf/Сеть/SSH"), uri("нет")), None).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn search_preview_and_note_meta() {
    let (app, _dir) = app();
    let (status, hits) =
        call(app.clone(), "GET", &format!("/api/vaults/test/search?q={}", uri("итоги второй")), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hits[0]["id"], "Книга");
    assert_eq!(hits[0]["anchor"], "Итоги-2");

    // В одной книге — все разделы по порядку.
    let (status, inside) =
        call(app.clone(), "GET", &format!("/api/vaults/test/search?q={}&note={}", uri("итоги"), uri("Книга")), None)
            .await;
    assert_eq!(status, StatusCode::OK);
    assert!(inside.as_array().unwrap().iter().all(|h| h["id"] == "Книга"));
    let q = format!("/api/vaults/test/search?q=x&note={}", uri("Нет/такой"));
    assert_eq!(call(app.clone(), "GET", &q, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        call(app.clone(), "GET", "/api/vaults/test/search?q=x&note=../x", None).await.0,
        StatusCode::BAD_REQUEST
    );

    let (status, p) = call(
        app.clone(),
        "GET",
        &format!("{}?anchor={}", uri("/api/vaults/test/preview/Сеть/SSH"), uri("Смена порта")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p["heading"], "Смена порта");
    assert_eq!(
        call(app.clone(), "GET", &uri("/api/vaults/test/preview/Нет/такой"), None).await.0,
        StatusCode::NOT_FOUND
    );

    let (_, list) = call(app.clone(), "GET", "/api/vaults/test/notes", None).await;
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
        async move { call(app, "GET", &format!("{}{q}", uri("/api/vaults/test/notes/Книга")), None).await }
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
    let (_, note) =
        call(app.clone(), "GET", &format!("{}?chapter=1", uri("/api/vaults/test/notes/Сеть/SSH")), None).await;
    assert!(note["book"].is_null() && note["rendered"]["title"] == "SSH");
}

#[tokio::test]
async fn token_is_required_when_set() {
    let (open, _open_dir) = app();
    let (app, _dir) = app_with_token("s3cret");
    let get = |uri: &str| Request::get(uri).body(Body::empty()).unwrap();
    let status = |res: axum::response::Response| res.status();

    // Без токена или с чужим — 401 на всём: API, клиент, шрифты.
    for path in ["/api/vaults/test/notes", "/", "/api/fonts.css", "/assets/baluk.css"] {
        assert_eq!(status(app.clone().oneshot(get(path)).await.unwrap()), StatusCode::UNAUTHORIZED, "{path}");
    }
    let (code, body) = call(app.clone(), "GET", "/api/vaults/test/notes", None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "нужен токен доступа");
    let wrong =
        Request::get("/api/vaults/test/notes").header("authorization", "Bearer s3cre").body(Body::empty()).unwrap();
    assert_eq!(status(app.clone().oneshot(wrong).await.unwrap()), StatusCode::UNAUTHORIZED);

    // С токеном — 200: заголовком, параметром адреса (ставит cookie) и cookie.
    let bearer =
        Request::get("/api/vaults/test/notes").header("authorization", "Bearer s3cret").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(bearer).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().get("set-cookie").is_none());

    let res = app.clone().oneshot(get("/api/vaults/test/notes?token=s3cret")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = res.headers()["set-cookie"].to_str().unwrap().to_owned();
    assert!(cookie.starts_with("notes_token=s3cret;") && cookie.contains("HttpOnly"), "{cookie}");

    let with_cookie =
        Request::get("/api/fonts.css").header("cookie", "theme=x; notes_token=s3cret").body(Body::empty());
    assert_eq!(status(app.oneshot(with_cookie.unwrap()).await.unwrap()), StatusCode::OK);

    // Без токена в настройках — проверки нет.
    assert_eq!(status(open.oneshot(get("/api/vaults/test/notes")).await.unwrap()), StatusCode::OK);
}

/// Следующий кадр тела ответа как текст (с тайм-аутом).
async fn next_frame(body: &mut Body) -> String {
    let frame =
        tokio::time::timeout(std::time::Duration::from_secs(10), body.frame()).await.expect("событие не пришло");
    String::from_utf8(frame.unwrap().unwrap().into_data().unwrap().to_vec()).unwrap()
}

#[tokio::test]
async fn events_report_file_changes() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("A.typ"), "a").unwrap();
    let notes = Arc::new(
        Notes::open(&NotesConfig {
            trash: None,
            vault: vault.path().to_owned(),
            library: LibrarySource::Dir(repo.join("baluk")),
            font_dirs: vec![],
            cache: None,
        })
        .unwrap(),
    );
    let dir = tempfile::tempdir().unwrap();
    let state = single(notes.clone(), &dir);
    assert!(notes.watch(), "каталог на диске — с наблюдателем");
    let res = router(state.clone())
        .oneshot(Request::get("/api/vaults/test/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    let mut body = res.into_body();
    assert_eq!(next_frame(&mut body).await, "event: hello\ndata: {\"watching\":true}\n\n");

    std::fs::write(vault.path().join("B.typ"), "b").unwrap();
    let change = next_frame(&mut body).await;
    assert!(
        change.starts_with("event: change\ndata: {\"seq\":") && change.contains(r#""paths":["B.typ"]"#),
        "{change}"
    );

    // Остановка сервера закрывает поток.
    state.closing.send_replace(true);
    let end = tokio::time::timeout(std::time::Duration::from_secs(10), body.frame()).await.unwrap();
    assert!(end.is_none());
}

#[tokio::test]
async fn broken_watcher_closes_events() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mem = Arc::new(MemStorage::new());
    mem.write("A.typ", "a");
    let config = NotesConfig {
        trash: None,
        vault: PathBuf::new(),
        library: LibrarySource::Dir(repo.join("baluk")),
        font_dirs: vec![],
        cache: None,
    };
    let notes = Arc::new(Notes::with_storage(mem.clone(), &config).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let state = single(notes.clone(), &dir);
    assert!(notes.watch());
    let res = router(state.clone())
        .oneshot(Request::get("/api/vaults/test/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let mut body = res.into_body();
    assert_eq!(next_frame(&mut body).await, "event: hello\ndata: {\"watching\":true}\n\n");

    // Изменения потеряны: «проверь всё», и поток закрывается — клиент
    // переподключится и перейдёт на опрос.
    mem.lose_changes();
    assert!(!notes.watching());
    let change = next_frame(&mut body).await;
    assert!(change.starts_with("event: change\n") && change.contains(r#""paths":[]"#), "{change}");
    let end = tokio::time::timeout(std::time::Duration::from_secs(10), body.frame()).await.unwrap();
    assert!(end.is_none());
}

/// Хранилища каталога данных: у каждого свои заметки; создание нового.
#[tokio::test]
async fn vaults_are_separate() {
    let data = tempfile::tempdir().unwrap();
    let vaults = Vaults::new(data.path());
    // Хранилищ ещё нет — сервер работает, список пуст, ничего само не создаётся.
    let dir = tempfile::tempdir().unwrap();
    let empty = VaultSet::registry(vaults.clone(), config(PathBuf::new(), None), NOTES.clone(), settings(&dir));
    let (_, list) = call(router(AppState::new(empty)), "GET", "/api/vaults", None).await;
    assert_eq!(list, serde_json::json!({ "vaults": [], "can_create": true }));
    assert!(!data.path().join("vaults").exists());

    for (vault, note) in [("Учёба", "Матан"), ("Работа", "Отчёт")] {
        let dir = vaults.create(&VaultName::new(vault).unwrap()).unwrap();
        std::fs::write(dir.join(format!("{note}.typ")), "").unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let set = VaultSet::registry(vaults, config(PathBuf::new(), None), NOTES.clone(), settings(&dir));
    let app = router(AppState::new(set));

    let (status, list) = call(app.clone(), "GET", "/api/vaults", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["vaults"], serde_json::json!(["Работа", "Учёба"]));
    assert!(list.get("default").is_none(), "хранилища по умолчанию нет");
    assert_eq!(list["can_create"], true);

    let ids =
        |list: Value| list.as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    let (status, study) = call(app.clone(), "GET", &uri("/api/vaults/Учёба/notes"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(study), ["Матан"]);
    let (_, work) = call(app.clone(), "GET", &uri("/api/vaults/Работа/notes"), None).await;
    assert_eq!(ids(work), ["Отчёт"]);
    let (status, body) = call(app.clone(), "GET", &uri("/api/vaults/Нет/notes"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("«Работа», «Учёба»"), "{body}");
    assert_eq!(call(app.clone(), "GET", "/api/vaults/..%2Fx/notes", None).await.0, StatusCode::NOT_FOUND);

    let (status, list) = call(app.clone(), "POST", "/api/vaults", Some(r#"{"name": "Новое"}"#)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(list["vaults"], serde_json::json!(["Новое", "Работа", "Учёба"]));
    assert!(data.path().join("vaults/Новое").is_dir());
    let (_, empty) = call(app.clone(), "GET", &uri("/api/vaults/Новое/notes"), None).await;
    assert!(ids(empty).is_empty());
    assert_eq!(call(app.clone(), "POST", "/api/vaults", Some(r#"{"name": "Новое"}"#)).await.0, StatusCode::CONFLICT);
    let (status, body) = call(app.clone(), "POST", "/api/vaults", Some(r#"{"name": "a/b"}"#)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("недопустимое имя хранилища"));

    // Темы и шрифты — общие, без хранилища в адресе.
    assert_eq!(call(app, "GET", "/api/themes", None).await.0, StatusCode::OK);
}

/// Переименование и удаление хранилища: открытое закрывается, заметки
/// переезжают с папкой, удалённое — в корзину.
#[tokio::test]
async fn vaults_rename_and_trash() {
    let data = tempfile::tempdir().unwrap();
    let trash = tempfile::tempdir().unwrap();
    let vaults = Vaults::new(data.path());
    for (vault, note) in [("Учёба", "Матан"), ("Работа", "Отчёт")] {
        let dir = vaults.create(&VaultName::new(vault).unwrap()).unwrap();
        std::fs::write(dir.join(format!("{note}.typ")), "").unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let config = config(PathBuf::new(), Some(trash.path().to_owned()));
    let app = router(AppState::new(VaultSet::registry(vaults, config, NOTES.clone(), settings(&dir))));
    // Открыто до переименования.
    assert_eq!(call(app.clone(), "GET", &uri("/api/vaults/Учёба/notes"), None).await.0, StatusCode::OK);

    let (status, list) = call(app.clone(), "PATCH", &uri("/api/vaults/Учёба"), Some(r#"{"name": "Учёба 2026"}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["vaults"], serde_json::json!(["Работа", "Учёба 2026"]));
    assert_eq!(call(app.clone(), "GET", &uri("/api/vaults/Учёба/notes"), None).await.0, StatusCode::NOT_FOUND);
    let (status, notes) = call(app.clone(), "GET", &uri("/api/vaults/Учёба 2026/notes"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(notes[0]["id"], "Матан");
    let taken = call(app.clone(), "PATCH", &uri("/api/vaults/Работа"), Some(r#"{"name": "Учёба 2026"}"#)).await;
    assert_eq!(taken.0, StatusCode::CONFLICT);
    let bad = call(app.clone(), "PATCH", &uri("/api/vaults/Работа"), Some(r#"{"name": "a/b"}"#)).await;
    assert_eq!(bad.0, StatusCode::BAD_REQUEST);

    let (status, list) = call(app.clone(), "DELETE", &uri("/api/vaults/Работа"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["vaults"], serde_json::json!(["Учёба 2026"]));
    assert!(trash.path().join("Работа/Отчёт.typ").is_file(), "в корзине целиком");
    assert_eq!(call(app, "DELETE", &uri("/api/vaults/Работа"), None).await.0, StatusCode::NOT_FOUND);
}

/// Настройки хранилища: поверх общих, в его папке, переезжают с ним; у
/// другого хранилища — общие; настройки устройства — только общие.
#[tokio::test]
async fn vault_settings_over_shared() {
    let data = tempfile::tempdir().unwrap();
    let vaults = Vaults::new(data.path());
    for vault in ["Учёба", "Работа"] {
        vaults.create(&VaultName::new(vault).unwrap()).unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let set = VaultSet::registry(vaults.clone(), config(PathBuf::new(), None), NOTES.clone(), settings(&dir));
    let app = router(AppState::new(set));
    let size = |v: &Value, part: &str| v[part]["appearance.font_size"].clone();

    let (status, body) =
        call(app.clone(), "PUT", &uri("/api/vaults/Учёба/settings"), Some(r#"{"appearance.font_size": 22}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((size(&body, "values"), size(&body, "own"), size(&body, "shared")), (22.into(), 22.into(), 19.into()));
    assert!(vaults.path(&VaultName::new("Учёба").unwrap()).join(".baluk/settings.json").is_file());
    let (_, other) = call(app.clone(), "GET", &uri("/api/vaults/Работа/settings"), None).await;
    assert_eq!(size(&other, "values"), 19);
    assert!(other["own"].as_object().unwrap().is_empty());

    // Общая настройка меняется у всех, своя у хранилища — сильнее.
    call(app.clone(), "PUT", "/api/settings", Some(r#"{"appearance.font_size": 20}"#)).await;
    assert_eq!(size(&call(app.clone(), "GET", &uri("/api/vaults/Работа/settings"), None).await.1, "values"), 20);
    assert_eq!(size(&call(app.clone(), "GET", &uri("/api/vaults/Учёба/settings"), None).await.1, "values"), 22);

    let device = r#"{"device.builds": 4}"#;
    assert_eq!(
        call(app.clone(), "PUT", &uri("/api/vaults/Учёба/settings"), Some(device)).await.0,
        StatusCode::BAD_REQUEST
    );

    // Переехали с папкой; null — снова общая.
    call(app.clone(), "PATCH", &uri("/api/vaults/Учёба"), Some(r#"{"name": "Учёба 2"}"#)).await;
    assert_eq!(size(&call(app.clone(), "GET", &uri("/api/vaults/Учёба 2/settings"), None).await.1, "values"), 22);
    let (_, body) =
        call(app, "PUT", &uri("/api/vaults/Учёба 2/settings"), Some(r#"{"appearance.font_size": null}"#)).await;
    assert_eq!(size(&body, "values"), 20);
}

/// Сервер на одной папке (`--vault <путь>`): хранилища не создаются, не
/// переименовываются и не удаляются.
#[tokio::test]
async fn single_vault_cannot_create() {
    let (app, _dir) = app();
    let (_, list) = call(app.clone(), "GET", "/api/vaults", None).await;
    assert_eq!(list, serde_json::json!({ "vaults": ["test"], "can_create": false }));
    assert_eq!(call(app.clone(), "POST", "/api/vaults", Some(r#"{"name": "x"}"#)).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(app.clone(), "PATCH", "/api/vaults/test", Some(r#"{"name": "x"}"#)).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(app.clone(), "DELETE", "/api/vaults/test", None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(app, "GET", "/api/vaults/other/notes", None).await.0, StatusCode::NOT_FOUND);
}

/// Удаление — в корзину (в тесте — каталог): заметка файлом, книга папкой.
#[tokio::test]
async fn delete_moves_to_trash() {
    let vault = tempfile::tempdir().unwrap();
    let trash = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Сеть")).unwrap();
    std::fs::write(vault.path().join("Сеть/SSH.typ"), "ssh").unwrap();
    std::fs::create_dir_all(vault.path().join("Книга")).unwrap();
    std::fs::write(vault.path().join("Книга/main.typ"), "").unwrap();
    std::fs::write(vault.path().join("Книга/01.typ"), "").unwrap();
    let notes = Arc::new(Notes::open(&config(vault.path().to_owned(), Some(trash.path().to_owned()))).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let app = router(single(notes, &dir));

    let (status, _) = call(app.clone(), "DELETE", &uri("/api/vaults/test/notes/Сеть/SSH"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(std::fs::read_to_string(trash.path().join("SSH.typ")).unwrap(), "ssh");
    assert_eq!(call(app.clone(), "DELETE", &uri("/api/vaults/test/notes/Книга"), None).await.0, StatusCode::NO_CONTENT);
    assert!(trash.path().join("Книга/01.typ").is_file());
    let (_, list) = call(app.clone(), "GET", "/api/vaults/test/notes", None).await;
    assert_eq!(list, serde_json::json!([]), "список — сразу");

    assert_eq!(
        call(app.clone(), "DELETE", &uri("/api/vaults/test/notes/Сеть/SSH"), None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(call(app.clone(), "DELETE", "/api/vaults/test/notes/..%2Fx", None).await.0, StatusCode::BAD_REQUEST);

    // Папка — целиком; файл или нет такой — 404.
    std::fs::create_dir_all(vault.path().join("Папка/Вложенная")).unwrap();
    std::fs::write(vault.path().join("Папка/Вложенная/Заметка.typ"), "з").unwrap();
    assert_eq!(
        call(app.clone(), "DELETE", &uri("/api/vaults/test/folders/Папка"), None).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(trash.path().join("Папка/Вложенная/Заметка.typ").is_file());
    assert_eq!(
        call(app.clone(), "DELETE", &uri("/api/vaults/test/folders/Папка"), None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(call(app, "DELETE", "/api/vaults/test/folders/..%2Fx", None).await.0, StatusCode::BAD_REQUEST);
}

/// Переименование: план ничего не меняет, `apply` — файл, название, ссылки;
/// пустая папка — в списке папок.
#[tokio::test]
async fn rename_plan_and_apply() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Сеть")).unwrap();
    std::fs::create_dir_all(vault.path().join("Пустая")).unwrap();
    std::fs::write(vault.path().join("Сеть/SSH.typ"), "#show: note.with(title: [SSH])\n").unwrap();
    std::fs::write(vault.path().join("A.typ"), "#see(\"Сеть/SSH\")").unwrap();
    let notes = Arc::new(Notes::open(&config(vault.path().to_owned(), None)).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let app = router(single(notes, &dir));

    let (_, folders) = call(app.clone(), "GET", &uri("/api/vaults/test/folders"), None).await;
    let paths: Vec<&str> = folders.as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert_eq!(paths, ["Пустая", "Сеть"]);

    let body = r#"{"kind":"note","id":"Сеть/SSH","title":"SSH: основы"}"#;
    let (status, plan) = call(app.clone(), "POST", &uri("/api/vaults/test/rename"), Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["to"], "Сеть/SSH основы");
    assert_eq!(plan["links"], serde_json::json!([{"note": "A", "count": 1}]));
    assert!(vault.path().join("Сеть/SSH.typ").is_file(), "план ничего не меняет");

    let body = r#"{"kind":"note","id":"Сеть/SSH","title":"SSH: основы","apply":true}"#;
    assert_eq!(call(app.clone(), "POST", &uri("/api/vaults/test/rename"), Some(body)).await.0, StatusCode::OK);
    assert_eq!(std::fs::read_to_string(vault.path().join("A.typ")).unwrap(), "#see(\"Сеть/SSH основы\")");
    let (_, list) = call(app.clone(), "GET", &uri("/api/vaults/test/notes"), None).await;
    assert!(
        list.as_array().unwrap().iter().any(|n| n["id"] == "Сеть/SSH основы" && n["title"] == "SSH: основы"),
        "{list}"
    );

    let body = r#"{"kind":"folder","id":"Пустая","title":"  "}"#;
    assert_eq!(call(app, "POST", &uri("/api/vaults/test/rename"), Some(body)).await.0, StatusCode::BAD_REQUEST);
}
