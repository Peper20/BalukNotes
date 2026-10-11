//! The vault sync API of the device (`/api/device/sync/...`) against a hub in
//! this process: sign-in, link, the workers, the vault events and what a
//! rename or a delete does to a linked vault.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use notes_core::settings::{Platform, Schema, SettingsStore};
use notes_core::{LibrarySource, NotesConfig, VaultName, Vaults};
use notes_device::testing::{PASSWORD, TestHub};
use notes_device::{DeviceSync, Paths, Prefer, Timing};
use notes_server::{AppState, VaultSet, router};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common::NOTES;

static HUB: LazyLock<TestHub> = LazyLock::new(|| TestHub::start(&["ivan", "pavel", "anna", "dana", "boris"]));

const LONG: Duration = Duration::from_secs(15);

fn fast() -> Timing {
    Timing {
        debounce: Duration::from_millis(100),
        held_recheck: Duration::from_millis(300),
        poll_wait: Duration::from_secs(3),
        backoff_min: Duration::from_millis(100),
        backoff_max: Duration::from_millis(400),
    }
}

/// An app on its own data directory with the sync of that directory.
struct App {
    data: tempfile::TempDir,
    _config: tempfile::TempDir,
    trash: tempfile::TempDir,
    state: AppState,
    router: Router,
    sync: Arc<DeviceSync>,
}

impl App {
    fn new() -> Self {
        Self::with_sync(true)
    }

    fn with_sync(with: bool) -> Self {
        let (data, config_dir, trash) =
            (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let schema = Schema::new(NOTES.themes().themes(), Platform::Desktop);
        let settings = Arc::new(SettingsStore::open(config_dir.path().join("settings.json"), schema).unwrap());
        // No warming: these tests do not build notes.
        settings.update(&json!({"device.warm": "off"}).as_object().unwrap().clone()).unwrap();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = NotesConfig {
            vault: PathBuf::new(),
            library: LibrarySource::Dir(repo.join("baluk")),
            font_dirs: vec![],
            cache: None,
            trash: Some(trash.path().to_owned()),
        };
        let set = VaultSet::registry(Vaults::new(data.path()), config, NOTES.clone(), settings.clone());
        let for_prefer = settings.clone();
        let sync =
            DeviceSync::with_timing(data.path(), move || notes_device::prefer(for_prefer.device().sync_prefer), fast());
        let state = AppState::new(set).with_device_sync(with.then(|| sync.clone()));
        let router = router(state.clone());
        Self { data, _config: config_dir, trash, state, router, sync }
    }

    async fn call(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(json) => {
                req = req.header("content-type", "application/json");
                json.to_string()
            }
            None => String::new(),
        };
        let res = self.router.clone().oneshot(req.body(Body::from(body)).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn login(&self, who: &str) {
        let body = json!({"server": HUB.url, "login": who, "password": PASSWORD});
        let (status, body) = self.call("POST", "/api/device/sync/login", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    async fn vault_status(&self, name: &str) -> Value {
        let (status, body) = self.call("GET", "/api/device/sync", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["vaults"].as_array().unwrap().iter().find(|v| v["name"] == name).cloned().unwrap_or(Value::Null)
    }

    fn write(&self, vault: &str, file: &str, text: &str) {
        let path = self.data.path().join("vaults").join(vault).join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(&self, vault: &str, file: &str) -> Option<String> {
        std::fs::read_to_string(self.data.path().join("vaults").join(vault).join(file)).ok()
    }
}

fn wait_for(what: &str, mut check: impl FnMut() -> bool) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < LONG, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A second device: the library on its own data directory.
fn other_device(who: &str) -> (tempfile::TempDir, Paths) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let server = notes_device::parse_server(&HUB.url).unwrap();
    notes_device::login(&paths, &server, who, PASSWORD).unwrap();
    (dir, paths)
}

#[tokio::test(flavor = "multi_thread")]
async fn sign_in_link_and_the_workers() {
    let app = App::new();
    app.write("notes", "a.typ", "= A\n");

    // Not signed in.
    let (status, body) = app.call("GET", "/api/device/sync", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (body["signed_in"].clone(), body["server"].clone(), body["session_ended"].clone()),
        (false.into(), Value::Null, false.into())
    );
    assert_eq!(body["vaults"][0]["name"], "notes");
    assert_eq!(
        (body["vaults"][0]["local"].clone(), body["vaults"][0]["linked"].clone(), body["vaults"][0]["state"].clone()),
        (true.into(), false.into(), "idle".into())
    );
    let (status, body) = app.call("POST", "/api/device/sync/vaults/notes/link", None).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (StatusCode::CONFLICT, Some("not signed in: notes sync login <server> --login <name>"))
    );

    // Sign in: a wrong password is 422 (never 401), a right one saves the account.
    let wrong = json!({"server": HUB.url, "login": "pavel", "password": "no"});
    let (status, body) = app.call("POST", "/api/device/sync/login", Some(wrong)).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("wrong login or password")));
    let bad = json!({"server": "ftp://x", "login": "ivan", "password": "x"});
    assert_eq!(app.call("POST", "/api/device/sync/login", Some(bad)).await.0, StatusCode::BAD_REQUEST);
    let body = json!({"server": HUB.url, "login": "ivan", "password": PASSWORD});
    let (status, answer) = app.call("POST", "/api/device/sync/login", Some(body)).await;
    assert_eq!((status, answer), (StatusCode::OK, json!({"server": HUB.url, "login": "ivan", "insecure": false})));

    // The vault list knows the server too: the other device's vault is there.
    let (dir2, other) = other_device("ivan");
    let far = VaultName::new("far-away").unwrap();
    std::fs::create_dir_all(other.vault_dir(&far)).unwrap();
    std::fs::write(other.vault_dir(&far).join("f.typ"), "= far").unwrap();
    notes_device::link(&other, &far, Prefer::Local).unwrap();
    let row = app.vault_status("far-away").await;
    assert_eq!(
        (row["local"].clone(), row["remote"].clone(), row["linked"].clone()),
        (false.into(), true.into(), false.into())
    );

    // Link: uploaded, then the worker is idle.
    let (status, report) = app.call("POST", "/api/device/sync/vaults/notes/link", None).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!((report["uploaded"].clone(), report["downloaded"].clone()), (1.into(), 0.into()));
    assert_eq!(report["conflicts"], json!([]));
    wait_for("idle", || futures_status(&app, "notes") == "idle");
    let row = app.vault_status("notes").await;
    assert_eq!(
        (row["linked"].clone(), row["remote"].clone(), row["error"].clone()),
        (true.into(), true.into(), Value::Null)
    );
    assert!(row["report"]["uploaded"].is_u64(), "the last round, as the worker ran it");
    assert!(row["last_sync"].as_u64().unwrap() > 1_700_000_000);
    let (status, report) = app.call("POST", "/api/device/sync/vaults/notes/now", None).await;
    assert_eq!((status, report["uploaded"].clone()), (StatusCode::OK, 0.into()));

    // The worker pushes an edit by itself, and pulls the other device's.
    app.write("notes", "b.typ", "= B\n");
    let near = VaultName::new("notes").unwrap();
    notes_device::link(&other, &near, Prefer::Remote).unwrap();
    wait_for("the push", || {
        notes_device::sync_linked(&other, &near, Prefer::Remote).unwrap();
        std::fs::read_to_string(other.vault_dir(&near).join("b.typ")).is_ok()
    });
    std::fs::write(other.vault_dir(&near).join("c.typ"), "= C from the other device\n").unwrap();
    notes_device::sync_linked(&other, &near, Prefer::Local).unwrap();
    wait_for("the pull", || app.read("notes", "c.typ").is_some());
    drop(dir2);

    // Unlink: files stay, the link is gone, a round says so.
    let (status, _) = app.call("POST", "/api/device/sync/vaults/notes/unlink", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = app.vault_status("notes").await;
    assert_eq!((row["linked"].clone(), row["state"].clone()), (false.into(), "idle".into()));
    assert!(app.read("notes", "c.typ").is_some());
    let (status, body) = app.call("POST", "/api/device/sync/vaults/notes/now", None).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (StatusCode::CONFLICT, Some("vault \"notes\" is not linked: notes sync link --vault \"notes\""))
    );
    let (status, body) = app.call("POST", "/api/device/sync/vaults/ghost/link", None).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("vault \"ghost\" exists neither on this device nor on the server"))
    );
    let (status, _) = app.call("POST", "/api/device/sync/vaults/a%2Fb/link", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Sign out.
    let (status, _) = app.call("POST", "/api/device/sync/logout", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = app.call("GET", "/api/device/sync", None).await;
    assert_eq!(body["signed_in"], false);
    app.sync.stop();
}

/// The worker state of a vault, for waiting (blocking: the status is a quick call).
fn futures_status(app: &App, name: &str) -> String {
    let status = app.sync.status(false).unwrap();
    let row = status.vaults.iter().find(|v| v.name == name).unwrap();
    serde_json::to_value(row.state).unwrap().as_str().unwrap().to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn pulled_files_appear_in_the_vault_events() {
    let app = App::new();
    app.login("anna").await;
    app.write("events", "a.typ", "= A\n");
    let (status, _) = app.call("POST", "/api/device/sync/vaults/events/link", None).await;
    assert_eq!(status, StatusCode::OK);

    // The server as `serve` runs it: the watcher of an open vault is on.
    app.state.vaults.start_background();
    let (status, body) = app.call("GET", "/api/vaults/events/events", None).await;
    assert_eq!((status, body["watching"].clone()), (StatusCode::OK, true.into()), "{body}");
    let after = body["seq"].as_u64().unwrap();

    let (dir, other) = other_device("anna");
    let vault = VaultName::new("events").unwrap();
    notes_device::link(&other, &vault, Prefer::Remote).unwrap();
    std::fs::write(other.vault_dir(&vault).join("pulled.typ"), "= pulled\n").unwrap();
    notes_device::sync_linked(&other, &vault, Prefer::Local).unwrap();

    // The long poll answers when the worker's write is seen by the watcher.
    let started = Instant::now();
    let mut seen = Vec::new();
    while !seen.iter().any(|p: &String| p == "pulled.typ") {
        assert!(started.elapsed() < LONG, "no event for the pulled note: {seen:?}");
        let (_, body) = app.call("GET", &format!("/api/vaults/events/events?after={after}"), None).await;
        seen = body["changes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| c["paths"].as_array().unwrap().clone())
            .filter_map(|p| p.as_str().map(str::to_owned))
            .collect();
    }
    assert_eq!(app.read("events", "pulled.typ").as_deref(), Some("= pulled\n"));
    drop(dir);
    app.sync.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn renaming_or_deleting_a_linked_vault_unlinks_it() {
    let app = App::new();
    app.login("dana").await;
    for name in ["old-name", "doomed"] {
        app.write(name, "a.typ", "= A\n");
        let (status, _) = app.call("POST", &format!("/api/device/sync/vaults/{name}/link"), None).await;
        assert_eq!(status, StatusCode::OK);
    }
    let state_file = |name: &str| app.data.path().join("sync/vaults").join(format!("{name}.json"));
    assert!(state_file("old-name").is_file() && state_file("doomed").is_file());

    let (status, _) = app.call("PATCH", "/api/vaults/old-name", Some(json!({"name": "new-name"}))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!state_file("old-name").exists(), "the state of the old name is gone");
    let row = app.vault_status("new-name").await;
    assert_eq!((row["local"].clone(), row["linked"].clone()), (true.into(), false.into()));
    // The server keeps the copy under the old name.
    let row = app.vault_status("old-name").await;
    assert_eq!(
        (row["local"].clone(), row["remote"].clone(), row["linked"].clone()),
        (false.into(), true.into(), false.into())
    );

    let (status, _) = app.call("DELETE", "/api/vaults/doomed", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!state_file("doomed").exists());
    assert!(app.trash.path().join("doomed").is_dir(), "the folder is in the trash");
    let row = app.vault_status("doomed").await;
    assert_eq!((row["local"].clone(), row["remote"].clone()), (false.into(), true.into()), "not deleted on the server");

    // A rename that fails leaves the vault linked and its worker running.
    app.write("keep", "a.typ", "= A\n");
    app.call("POST", "/api/device/sync/vaults/keep/link", None).await;
    let (status, _) = app.call("PATCH", "/api/vaults/keep", Some(json!({"name": "new-name"}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "the name is taken");
    assert!(state_file("keep").is_file());
    wait_for("idle", || futures_status(&app, "keep") == "idle");
    app.sync.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_emptied_vault_waits_for_confirm_or_restore() {
    let app = App::new();
    app.login("boris").await;
    let empty = |app: &App| {
        for i in 0..12 {
            std::fs::remove_file(app.data.path().join("vaults/wipe").join(format!("n{i}.typ"))).unwrap();
        }
    };
    for i in 0..12 {
        app.write("wipe", &format!("n{i}.typ"), &format!("= {i}\n"));
    }
    let (status, _) = app.call("POST", "/api/device/sync/vaults/wipe/link", None).await;
    assert_eq!(status, StatusCode::OK);
    wait_for("idle", || futures_status(&app, "wipe") == "idle");

    empty(&app);
    wait_for("held", || futures_status(&app, "wipe") == "held");
    let row = app.vault_status("wipe").await;
    assert_eq!(row["held"], json!({"side": "server", "count": 12, "total": 12}));
    assert!(row["error"].as_str().unwrap().contains("12 of 12 files are gone from this device"), "{row}");
    let (status, body) = app.call("POST", "/api/device/sync/vaults/wipe/now", None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Back from the server.
    let (status, body) = app.call("POST", "/api/device/sync/vaults/wipe/restore", None).await;
    assert_eq!((status, body["downloaded"].clone()), (StatusCode::OK, 12.into()), "{body}");
    assert_eq!(app.read("wipe", "n3.typ").as_deref(), Some("= 3\n"));
    wait_for("idle", || futures_status(&app, "wipe") == "idle");
    assert_eq!(app.vault_status("wipe").await["held"], Value::Null);

    // Deleted for good.
    empty(&app);
    wait_for("held again", || futures_status(&app, "wipe") == "held");
    let (status, body) = app.call("POST", "/api/device/sync/vaults/wipe/confirm", None).await;
    assert_eq!((status, body["removed_remote"].clone()), (StatusCode::OK, 12.into()), "{body}");
    wait_for("idle again", || futures_status(&app, "wipe") == "idle");
    let (status, _) = app.call("POST", "/api/device/sync/vaults/wipe/confirm", None).await;
    assert_eq!(status, StatusCode::CONFLICT, "nothing waits any more");
    app.sync.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_server_leaves_the_remote_unknown() {
    let mut hub = TestHub::start(&["olga"]);
    let app = App::new();
    let body = json!({"server": hub.url, "login": "olga", "password": PASSWORD});
    assert_eq!(app.call("POST", "/api/device/sync/login", Some(body)).await.0, StatusCode::OK);
    app.write("v", "a.typ", "= A\n");
    hub.stop();

    let (status, body) = app.call("GET", "/api/device/sync", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["signed_in"], true);
    assert!(body["server_error"].as_str().unwrap().starts_with("cannot reach the server"), "{body}");
    assert_eq!(body["vaults"][0]["remote"], Value::Null);
    let (status, body) = app.call("POST", "/api/device/sync/vaults/v/link", None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    app.sync.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn without_sync_the_routes_say_so() {
    let app = App::with_sync(false);
    let (status, body) = app.call("GET", "/api/device/sync", None).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::NOT_FOUND, Some("this server runs without vault sync")));
    let (status, _) = app.call("POST", "/api/device/sync/vaults/v/now", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
