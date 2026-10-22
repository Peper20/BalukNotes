//! The core in WASM for the browser (`docs/research/E4.md`, architecture §1).
//!
//! One thread, no disk: the host (a Web Worker) hands the vault files and the
//! Typst package files over as bytes, the core keeps them in memory
//! ([`MemStorage`]) and answers the HTTP API requests the host passes in
//! ([`Core::request`]) with the same `notes-server` `Router` as `notes serve`.
//! The core knows no disk: the host persists what changed ([`Core::take_changes`],
//! [`Core::read_file`], [`Core::settings_json`]) and hands it back at the next
//! start (`add_file`, [`Core::set_settings`]); the worker writes it to OPFS
//! (architecture §1).
//! The vault syncs with the hub through the same core ([`Core::sync`]: the
//! `notes-store` engine over [`tree::VaultTree`], the server wins as on a
//! phone); the host keeps the state and the removed files
//! ([`Core::take_sync_state`], [`Core::take_removed`]). The vault reports its
//! changes as vault events, so an open note refreshes when a round pulls it.
//! [`Core`] is plain Rust (its tests run natively); the JS binding is
//! [`wasm::WasmCore`] with the `XMLHttpRequest` remote ([`xhr`]), compiled
//! only for `wasm32`.

mod tree;
#[cfg(any(test, target_family = "wasm"))]
mod xhr;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use notes_core::settings::{Schema, SettingsStore};
use notes_core::{LibrarySource, Notes, NotesConfig, SharedAssets};
use notes_server::{AppState, VaultSet, router};
use notes_store::names::VaultName;
use notes_store::platform::Platform;
use notes_store::storage::{MemStorage, Storage};
use notes_store::sync::{self, Error as SyncError, Options, Prefer, Remote, Report, State};
use serde_json::{Map, Value, json};
use tower::ServiceExt;

use crate::tree::VaultTree;

/// The answer to an API request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    /// The `Content-Type` header, empty if the answer has none.
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    /// An error in the shape of the API errors (`{ "error": ... }`).
    fn error(status: u16, message: &str) -> Self {
        let body = serde_json::json!({ "error": message }).to_string().into_bytes();
        Self { status, content_type: "application/json".into(), body }
    }
}

/// What changed since the last [`Core::take_changes`]: the host persists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// The vault paths created, rewritten, renamed (both paths) or trashed,
    /// sorted. [`Core::read_file`] says which of them are gone.
    pub files: Vec<String>,
    /// The shared settings changed ([`Core::settings_json`]).
    pub settings: bool,
}

/// An opened vault: the core, the shared settings and the router over it.
#[derive(Debug)]
struct Open {
    notes: Arc<Notes>,
    settings: Arc<SettingsStore>,
    router: Router,
}

/// The sync state and the JSON of it the host has (to tell a change).
#[derive(Debug)]
struct SyncState {
    state: State,
    kept: String,
}

/// The state as JSON; a state of strings and numbers always serializes.
fn state_json(state: &State) -> String {
    serde_json::to_string(state).unwrap_or_default()
}

impl Default for SyncState {
    /// The empty state of a vault never synced: nothing for the host to keep.
    fn default() -> Self {
        let state = State::default();
        Self { kept: state_json(&state), state }
    }
}

/// The core over a vault in memory.
#[derive(Debug)]
pub struct Core {
    name: VaultName,
    vault: Arc<MemStorage>,
    tree: VaultTree,
    sync: Mutex<SyncState>,
    packages: Arc<MemStorage>,
    /// The shared settings the host handed over before [`Core::open`].
    stored: Map<String, Value>,
    open: Option<Open>,
}

/// The core over `vault` and `packages`, with settings in memory.
fn open_notes(vault: &Arc<MemStorage>, packages: &Arc<MemStorage>) -> Result<Arc<Notes>, String> {
    let config = NotesConfig {
        vault: PathBuf::from("memory"),
        library: LibrarySource::Embedded,
        font_dirs: Vec::new(),
        cache: None,
        trash: None,
    };
    let shared = SharedAssets { packages: Some(packages.clone()), ..SharedAssets::default() };
    Notes::with_storage_shared(vault.clone(), &config, &shared).map(Arc::new).map_err(|e| e.to_string())
}

/// The shared settings in memory: the defaults with `stored` on top.
fn shared_settings(notes: &Notes, stored: Map<String, Value>) -> Arc<SettingsStore> {
    let schema = Schema::new(notes.themes().themes(), Platform::current());
    Arc::new(SettingsStore::in_memory_with(schema, stored))
}

/// The state the router serves: one vault, settings in memory.
fn app_state(name: VaultName, notes: Arc<Notes>, settings: Arc<SettingsStore>) -> AppState {
    AppState::new(VaultSet::single(name, notes, settings))
}

impl Core {
    /// An empty vault and no packages; `vault` is the vault name in the API
    /// paths (`/api/vaults/<vault>/...`).
    pub fn new(vault: &str) -> Result<Self, String> {
        let name = VaultName::new(vault).map_err(|e| e.to_string())?;
        let vault = Arc::new(MemStorage::new());
        vault.keep_journal();
        Ok(Self {
            name,
            tree: VaultTree::new(vault.clone()),
            vault,
            sync: Mutex::default(),
            packages: Arc::new(MemStorage::new()),
            stored: Map::new(),
            open: None,
        })
    }

    /// The shared settings the host kept (`{ "key": value }`, what
    /// [`Self::settings_json`] gave). Only before [`Self::open`] (an error
    /// after it): the host hands the kept settings over at start. Invalid
    /// values are dropped, the defaults hold.
    pub fn set_settings(&mut self, json: &str) -> Result<(), String> {
        if self.open.is_some() {
            return Err("the settings are set before the vault is opened".into());
        }
        self.stored = serde_json::from_str(json).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The shared settings as JSON, to be kept by the host and handed back by
    /// [`Self::set_settings`].
    pub fn settings_json(&self) -> String {
        let values = match &self.open {
            Some(open) => open.settings.values(),
            None => self.stored.clone(),
        };
        // A JSON map of values always serializes.
        serde_json::to_string(&values).unwrap_or_else(|_| "{}".into())
    }

    /// The vault files and settings that changed since the last call (the
    /// first call includes the files added before it). Trashing removes a
    /// file for good: the browser has no trash.
    pub fn take_changes(&self) -> Changes {
        Changes {
            files: self.vault.take_journal(),
            settings: self.open.as_ref().is_some_and(|open| open.settings.take_changed()),
        }
    }

    /// The vault file `path`; `None`: it is gone.
    pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.vault.read(path).ok()
    }

    /// Adds or replaces a vault file (`Сеть/SSH.typ`).
    pub fn add_file(&self, path: &str, data: &[u8]) {
        self.vault.write(path, data);
    }

    /// Adds a Typst package file (`preview/cetz/0.4.2/src/lib.typ`).
    pub fn add_package_file(&self, path: &str, data: &[u8]) {
        self.packages.write(path, data);
    }

    /// Opens the vault: loads the embedded fonts, builds the themes and the
    /// router. Once; files added later are seen by the next build. The
    /// settings live in memory for the life of the core; the host persists
    /// them ([`Self::take_changes`]).
    pub fn open(&mut self) -> Result<(), String> {
        if self.open.is_some() {
            return Ok(());
        }
        let notes = open_notes(&self.vault, &self.packages)?;
        // The vault reports its changes (architecture §8): `GET .../events`
        // answers `watching: true`, and the host holds the request until
        // something changes (a request, a sync round).
        notes.watch();
        let settings = shared_settings(&notes, std::mem::take(&mut self.stored));
        let router = router(app_state(self.name.clone(), notes.clone(), settings.clone()));
        self.open = Some(Open { notes, settings, router });
        Ok(())
    }

    /// Answers an API request as `notes serve` does (without compression).
    /// `uri` is the path and query as a browser sends them (percent-encoded);
    /// a non-empty `body` is JSON. A request on a vault that is not open, or a
    /// malformed one, gets an error answer (503, 400), not a failure.
    pub async fn request(&self, method: &str, uri: &str, body: &[u8]) -> Response {
        let Some(open) = &self.open else {
            return Response::error(503, "the vault is not open");
        };
        let mut builder = Request::builder().method(method).uri(uri);
        if !body.is_empty() {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        let request = match builder.body(Body::from(body.to_vec())) {
            Ok(request) => request,
            Err(e) => return Response::error(400, &e.to_string()),
        };
        // The router's error is `Infallible`: an API error is an answer.
        let Ok(response) = open.router.clone().oneshot(request).await;
        let (parts, body) = response.into_parts();
        let content_type = parts.headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or_default();
        match body.collect().await {
            Ok(body) => Response {
                status: parts.status.as_u16(),
                content_type: content_type.to_owned(),
                body: body.to_bytes().into(),
            },
            Err(e) => Response::error(500, &e.to_string()),
        }
    }

    fn sync_state(&self) -> std::sync::MutexGuard<'_, SyncState> {
        self.sync.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The sync state the host kept (what [`Self::take_sync_state`] gave).
    /// One that cannot be read is an error and the state stays empty: the
    /// next round compares the whole vault, which deletes nothing.
    pub fn set_sync_state(&self, json: &str) -> Result<(), String> {
        let state: State = serde_json::from_str(json).map_err(|e| e.to_string())?;
        *self.sync_state() = SyncState { state, kept: json.to_owned() };
        Ok(())
    }

    /// The sync state as JSON if it changed since the host last got it.
    pub fn take_sync_state(&self) -> Option<String> {
        let mut sync = self.sync_state();
        let json = state_json(&sync.state);
        if json == sync.kept {
            return None;
        }
        sync.kept.clone_from(&json);
        Some(json)
    }

    /// What sync replaced or removed in the vault since the last call: the
    /// path and the previous content, for the host to keep aside.
    pub fn take_removed(&self) -> Vec<(String, Vec<u8>)> {
        self.tree.take_removed()
    }

    /// One sync round of the vault with `remote` (architecture §9): the
    /// server wins a conflict ("the others receive"), a mass deletion is held
    /// ([`SyncError::DeletionsHeld`]) having changed nothing. Pulled files are
    /// vault changes like any other: the vault events and the journal show
    /// them.
    ///
    /// # Errors
    /// The round failed (the hub cannot be reached, the session is refused)
    /// or was held; what it did before stays recorded in the state.
    pub fn sync(&self, remote: &dyn Remote) -> sync::Result<Report> {
        let mut sync = self.sync_state();
        sync::sync_with(&self.tree, &mut sync.state, remote, &Options::new(Prefer::Remote))
    }

    /// Forgets what Typst memoized (there is no idle thread to do it here).
    pub fn release_memory(&self) {
        if let Some(open) = &self.open {
            open.notes.release_memory();
        }
    }
}

/// A round's result for the host: `{ ok: true, changed, uploaded,
/// downloaded, removedLocal, removedRemote, conflicts, skipped }` or `{ ok:
/// false, error, held, unauthorized }`.
pub fn round_json(round: &sync::Result<Report>) -> String {
    let value = match round {
        Ok(report) => json!({
            "ok": true,
            "changed": !report.is_idle(),
            "uploaded": report.uploaded,
            "downloaded": report.downloaded,
            "removedLocal": report.removed_local,
            "removedRemote": report.removed_remote,
            "conflicts": report.conflicts,
            "skipped": report.skipped,
        }),
        Err(e) => json!({
            "ok": false,
            "error": e.to_string(),
            "held": matches!(e, SyncError::DeletionsHeld(_)),
            "unauthorized": matches!(e, SyncError::Unauthorized),
        }),
    };
    value.to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const VAULT: &str = "memory";

    /// A note, a book, a note with a whitelisted package and a note with a
    /// missing one; the package files are under `_packages/`.
    fn files() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("Привет.typ", "= Привет\n\nТекст $x^2$.".as_bytes()),
            ("Пакет.typ", "#import \"@preview/oxifmt:1.0.0\": answer\nОтвет: #answer".as_bytes()),
            ("Нет пакета.typ", br#"#import "@preview/cetz:0.4.2""#),
            ("Книга/main.typ", "= Книга\n\n#include \"01-глава.typ\"".as_bytes()),
            ("Книга/01-глава.typ", "= Глава\n\nТекст главы.".as_bytes()),
            (
                "_packages/preview/oxifmt/1.0.0/typst.toml",
                b"[package]\nname = \"oxifmt\"\nversion = \"1.0.0\"\nentrypoint = \"lib.typ\"\n",
            ),
            ("_packages/preview/oxifmt/1.0.0/lib.typ", b"#let answer = 42"),
        ]
    }

    fn fill(vault: &MemStorage, packages: &MemStorage) {
        for (path, data) in files() {
            match path.strip_prefix("_packages/") {
                Some(package) => packages.write(package, data),
                None => vault.write(path, data),
            }
        }
    }

    fn open(mut core: Core) -> Core {
        core.open().unwrap();
        core
    }

    fn core() -> Core {
        let core = Core::new(VAULT).unwrap();
        fill(&core.vault, &core.packages);
        open(core)
    }

    fn json_of(response: &Response) -> Value {
        assert!(response.content_type.starts_with("application/json"), "{}", response.content_type);
        serde_json::from_slice(&response.body).unwrap()
    }

    /// Percent-encodes a path the way a browser sends it.
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

    /// The API over a vault from memory: the lists, a note in HTML, a book, a
    /// package from the package files, a missing package as a build error of
    /// the note, an unknown note, events, settings kept in memory, the session.
    #[tokio::test]
    async fn serves_the_api() {
        let core = core();
        let base = format!("/api/vaults/{VAULT}");

        let list = core.request("GET", &format!("{base}/notes"), b"").await;
        assert_eq!(list.status, 200);
        let mut ids: Vec<_> =
            json_of(&list).as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap().to_owned()).collect();
        ids.sort();
        assert_eq!(ids, ["Книга", "Нет пакета", "Пакет", "Привет"]);
        assert!(json_of(&list).as_array().unwrap().iter().any(|n| n["id"] == "Книга" && n["kind"] == "book"));

        let hello = core.request("GET", &format!("{base}/notes/{}", uri("Привет")), b"").await;
        assert_eq!(hello.status, 200);
        let hello = json_of(&hello);
        assert!(hello["rendered"]["body"].as_str().unwrap().contains("Привет"), "{hello}");
        assert_eq!(hello["errors"], json!([]));

        let book = json_of(&core.request("GET", &format!("{base}/notes/{}", uri("Книга")), b"").await);
        assert!(book["rendered"]["body"].as_str().unwrap().contains("Текст главы"), "{book}");

        let package = json_of(&core.request("GET", &format!("{base}/notes/{}", uri("Пакет")), b"").await);
        assert!(package["rendered"]["body"].as_str().unwrap().contains("42"), "{package}");

        let missing = json_of(&core.request("GET", &format!("{base}/notes/{}", uri("Нет пакета")), b"").await);
        assert!(missing["rendered"].is_null());
        assert!(missing["errors"].to_string().contains("cetz"), "{missing}");

        let unknown = core.request("GET", &format!("{base}/notes/{}", uri("Нет такой")), b"").await;
        assert_eq!(unknown.status, 404);
        assert!(json_of(&unknown)["error"].is_string());

        let events = core.request("GET", &format!("{base}/events?after=0"), b"").await;
        assert_eq!(events.status, 200);
        let events = json_of(&events);
        assert_eq!(events["watching"], true, "the vault reports its changes");
        assert_eq!(events["changes"], json!([]));

        let put = core.request("PUT", "/api/settings", br#"{"device.builds": 3}"#).await;
        assert_eq!(put.status, 200);
        let settings = json_of(&core.request("GET", "/api/settings", b"").await);
        assert_eq!(settings["values"]["device.builds"], 3);

        let session = core.request("GET", "/api/session", b"").await;
        assert_eq!((session.status, session.body.len()), (204, 0));

        let themes = core.request("GET", "/api/themes.css", b"").await;
        assert_eq!(themes.status, 200);
        assert!(themes.content_type.starts_with("text/css"), "{}", themes.content_type);

        core.release_memory();
    }

    /// `take_changes` lists the paths created, rewritten, renamed and trashed
    /// once; `read_file` says what is gone.
    #[test]
    fn takes_changes() {
        let core = core();
        let first = core.take_changes();
        assert_eq!(first.files.len(), 5, "the files added before: {first:?}");
        assert!(!first.settings);
        assert_eq!(core.take_changes(), Changes::default(), "taken once");

        let storage: &dyn Storage = core.vault.as_ref();
        storage.create("Новая.typ", "= Новая".as_bytes()).unwrap();
        storage.rewrite("Привет.typ", "= Привет 2".as_bytes()).unwrap();
        storage.rename("Книга", "Том").unwrap();
        storage.trash("Пакет.typ").unwrap();
        let changes = core.take_changes();
        assert_eq!(
            changes.files,
            [
                "Книга/01-глава.typ",
                "Книга/main.typ",
                "Новая.typ",
                "Пакет.typ",
                "Привет.typ",
                "Том/01-глава.typ",
                "Том/main.typ"
            ]
        );
        assert_eq!(core.read_file("Новая.typ").unwrap(), "= Новая".as_bytes());
        assert_eq!(core.read_file("Том/main.typ").unwrap(), "= Книга\n\n#include \"01-глава.typ\"".as_bytes());
        assert_eq!(core.read_file("Привет.typ").unwrap(), "= Привет 2".as_bytes());
        assert_eq!(core.read_file("Пакет.typ"), None, "trashed");
        assert_eq!(core.read_file("Книга/main.typ"), None, "moved");
        assert_eq!(core.take_changes(), Changes::default());
    }

    /// The shared settings round-trip through the host: a change raises the
    /// flag once, `settings_json` goes into `set_settings` of a new core.
    #[tokio::test]
    async fn keeps_shared_settings() {
        let core = core();
        assert!(!core.take_changes().settings);
        let put = core.request("PUT", "/api/settings", br#"{"device.builds": 3}"#).await;
        assert_eq!(put.status, 200);
        assert!(core.take_changes().settings);
        assert!(!core.take_changes().settings, "raised once");
        let json = core.settings_json();
        assert_eq!(serde_json::from_str::<Value>(&json).unwrap()["device.builds"], 3);

        let mut next = Core::new(VAULT).unwrap();
        next.set_settings(&json).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&next.settings_json()).unwrap()["device.builds"], 3, "before open");
        next.open().unwrap();
        let settings = json_of(&next.request("GET", "/api/settings", b"").await);
        assert_eq!(settings["values"]["device.builds"], 3);
        assert!(!next.take_changes().settings, "loading is not a change");
        assert!(next.set_settings("{}").is_err(), "after open");

        let mut bad = Core::new(VAULT).unwrap();
        assert!(bad.set_settings("[1]").is_err());
        bad.set_settings(r#"{"device.builds": "many", "nothing": 1}"#).unwrap();
        bad.open().unwrap();
        let settings = json_of(&bad.request("GET", "/api/settings", b"").await);
        let defaults = json_of(&Core::new(VAULT).map(open).unwrap().request("GET", "/api/settings", b"").await);
        assert_eq!(settings["values"], defaults["values"], "an invalid value and an unknown key are dropped");
    }

    /// A vault setting set through the API is the vault file
    /// `.baluk/settings.json`: it is a change to persist, and a new core over
    /// the same files reads it back.
    #[tokio::test]
    async fn keeps_vault_settings_in_the_vault() {
        let core = core();
        core.take_changes();
        let base = format!("/api/vaults/{VAULT}");
        let put = core.request("PUT", &format!("{base}/settings"), br#"{"appearance.measure": 50}"#).await;
        assert_eq!(put.status, 200, "{}", String::from_utf8_lossy(&put.body));
        let changes = core.take_changes();
        assert_eq!(changes.files, [".baluk/settings.json"]);
        assert!(!changes.settings, "a vault setting is not a shared one");
        let file = core.read_file(".baluk/settings.json").unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&file).unwrap()["appearance.measure"], 50);

        let mut next = Core::new(VAULT).unwrap();
        next.add_file(".baluk/settings.json", &file);
        next.open().unwrap();
        let settings = json_of(&next.request("GET", &format!("{base}/settings"), b"").await);
        assert_eq!(settings["values"]["appearance.measure"], 50, "{settings}");
        assert_eq!(settings["own"]["appearance.measure"], 50, "{settings}");
    }

    /// A request before `open` and a malformed one are answers, not failures.
    #[tokio::test]
    async fn answers_bad_requests() {
        let mut core = Core::new(VAULT).unwrap();
        assert_eq!(core.request("GET", "/api/session", b"").await.status, 503);
        core.open().unwrap();
        assert_eq!(core.request("GET", "not a uri", b"").await.status, 400);
        assert_eq!(core.request("GET", "/api/nothing", b"").await.status, 404);
        assert!(Core::new("_internal").is_err());
    }

    /// The answer is byte for byte the one of the `notes-server` router built
    /// over the same files on its own.
    #[tokio::test]
    async fn matches_the_native_router() {
        let core = core();

        let vault = Arc::new(MemStorage::new());
        let packages = Arc::new(MemStorage::new());
        fill(&vault, &packages);
        let notes = open_notes(&vault, &packages).unwrap();
        let settings = shared_settings(&notes, Map::new());
        let native = router(app_state(VaultName::new(VAULT).unwrap(), notes, settings));

        for path in ["Привет", "Пакет", "Нет пакета", "Книга"] {
            let uri = format!("/api/vaults/{VAULT}/notes/{}", uri(path));
            let want = native.clone().oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap()).await.unwrap();
            let status = want.status().as_u16();
            let want = want.into_body().collect().await.unwrap().to_bytes();
            let got = core.request("GET", &uri, b"").await;
            assert_eq!(got.status, status, "{path}");
            assert_eq!(got.body, want.as_ref(), "{path}");
        }
    }

    /// The paths of the vault events after `after`, waited for (natively the
    /// watcher hands them out after a pause).
    async fn events_after(core: &Core, after: u64) -> Vec<String> {
        for _ in 0..50 {
            let events = json_of(&core.request("GET", &format!("/api/vaults/{VAULT}/events?after={after}"), b"").await);
            let paths: Vec<String> = events["changes"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|c| c["paths"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_owned()))
                .collect();
            if !paths.is_empty() {
                return paths;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Vec::new()
    }

    async fn latest_event(core: &Core) -> u64 {
        json_of(&core.request("GET", &format!("/api/vaults/{VAULT}/events"), b"").await)["seq"].as_u64().unwrap()
    }

    fn hub_write(hub: &sync::HubVault, path: &str, data: &[u8]) {
        let base = match hub.read(path) {
            Ok((entry, _)) => sync::Base::Hash(entry.hash.unwrap()),
            Err(_) => sync::Base::Absent,
        };
        hub.write(path, &base, data).unwrap().unwrap();
    }

    fn hub_delete(hub: &sync::HubVault, path: &str) {
        let (entry, _) = hub.read(path).unwrap();
        hub.delete(path, &sync::Base::Hash(entry.hash.unwrap())).unwrap().unwrap();
    }

    fn round(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    /// Rounds over the core's vault: the first one uploads and downloads,
    /// a conflict goes to the server and the local version is kept aside, a
    /// deletion goes both ways, a pulled change is a vault event, the state
    /// round-trips through JSON, a mass deletion is held.
    #[tokio::test]
    async fn syncs_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let hub = sync::HubVault::open(dir.path().join("hub")).unwrap();
        hub_write(&hub, "Хаб.typ", "= Хаб".as_bytes());
        let core = core();
        core.take_changes();
        assert_eq!(core.take_sync_state(), None, "nothing synced yet");

        let report = core.sync(&hub).unwrap();
        assert_eq!((report.uploaded, report.downloaded), (5, 1), "{report:?}");
        assert_eq!(hub.read("Книга/01-глава.typ").unwrap().1, "= Глава\n\nТекст главы.".as_bytes());
        assert_eq!(core.read_file("Хаб.typ").unwrap(), "= Хаб".as_bytes());
        assert_eq!(core.take_changes().files, ["Хаб.typ"], "the host persists a pulled file");
        assert!(core.take_removed().is_empty());
        assert!(core.take_sync_state().is_some(), "the state changed");
        assert_eq!(core.take_sync_state(), None, "taken once");
        assert_eq!(round(&round_json(&Ok(report)))["changed"], true);

        // Both changed: the server wins, the local version is kept aside;
        // a deletion on the hub removes the file, also kept aside.
        let seen = latest_event(&core).await;
        hub_write(&hub, "Привет.typ", "= Привет с хаба".as_bytes());
        hub_delete(&hub, "Хаб.typ");
        let storage: &dyn Storage = core.vault.as_ref();
        storage.rewrite("Привет.typ", "= Привет отсюда".as_bytes()).unwrap();
        let report = core.sync(&hub).unwrap();
        assert_eq!((report.downloaded, report.removed_local, report.conflicts.len()), (1, 1, 1), "{report:?}");
        assert_eq!(core.read_file("Привет.typ").unwrap(), "= Привет с хаба".as_bytes());
        assert_eq!(core.read_file("Хаб.typ"), None);
        assert_eq!(
            core.take_removed(),
            [
                ("Привет.typ".to_owned(), "= Привет отсюда".as_bytes().to_vec()),
                ("Хаб.typ".to_owned(), "= Хаб".as_bytes().to_vec())
            ]
        );
        let events = events_after(&core, seen).await;
        assert!(events.contains(&"Привет.typ".to_owned()) && events.contains(&"Хаб.typ".to_owned()), "{events:?}");

        // A note deleted in the browser is deleted on the hub.
        storage.trash("Пакет.typ").unwrap();
        let report = core.sync(&hub).unwrap();
        assert_eq!(report.removed_remote, 1, "{report:?}");
        assert!(hub.read("Пакет.typ").is_err());
        assert!(core.sync(&hub).unwrap().is_idle());

        // A new core over the same files with the kept state: nothing to do.
        let state = core.take_sync_state().unwrap();
        let next = Core::new(VAULT).unwrap();
        for path in core.vault.paths() {
            next.add_file(&path, &core.read_file(&path).unwrap());
        }
        assert!(next.set_sync_state("[]").is_err());
        next.set_sync_state(&state).unwrap();
        assert_eq!(next.take_sync_state(), None, "a loaded state is not a change");
        let report = next.sync(&hub).unwrap();
        assert!(report.is_idle(), "{report:?}");

        // The hub lost every file: held, nothing removed here.
        for path in ["Книга/01-глава.typ", "Книга/main.typ", "Нет пакета.typ", "Привет.typ"]
        {
            hub_delete(&hub, path);
        }
        let held = round(&round_json(&next.sync(&hub)));
        assert_eq!((&held["ok"], &held["held"], &held["unauthorized"]), (&json!(false), &json!(true), &json!(false)));
        assert!(next.read_file("Привет.typ").is_some());
        assert!(next.take_removed().is_empty());
        assert_eq!(round(&round_json(&Err(SyncError::Unauthorized)))["unauthorized"], true);
    }
}

/// The JS binding.
#[cfg(target_family = "wasm")]
pub mod wasm {
    use js_sys::{Array, Object, Reflect, Uint8Array};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console, js_name = error)]
        fn console_error(message: &str);
    }

    /// A Rust panic aborts the module (wasm32 has no unwinding): its message
    /// goes to the console before that.
    #[wasm_bindgen(start)]
    pub fn start() {
        std::panic::set_hook(Box::new(|info| console_error(&info.to_string())));
    }

    /// The core for JS.
    #[wasm_bindgen]
    #[derive(Debug)]
    pub struct WasmCore(super::Core);

    #[wasm_bindgen]
    impl WasmCore {
        /// `vault` - the vault name in the API paths.
        #[wasm_bindgen(constructor)]
        pub fn new(vault: &str) -> Result<WasmCore, JsError> {
            super::Core::new(vault).map(Self).map_err(|e| JsError::new(&e))
        }

        #[wasm_bindgen(js_name = addFile)]
        pub fn add_file(&self, path: &str, data: &[u8]) {
            self.0.add_file(path, data);
        }

        #[wasm_bindgen(js_name = addPackageFile)]
        pub fn add_package_file(&self, path: &str, data: &[u8]) {
            self.0.add_package_file(path, data);
        }

        pub fn open(&mut self) -> Result<(), JsError> {
            self.0.open().map_err(|e| JsError::new(&e))
        }

        /// Answers an API request: `{ status, contentType, body: Uint8Array }`.
        /// The host sends one request at a time (a build blocks the worker).
        pub async fn request(&self, method: String, uri: String, body: Option<Vec<u8>>) -> Result<JsValue, JsError> {
            let response = self.0.request(&method, &uri, &body.unwrap_or_default()).await;
            let object = Object::new();
            let set = |key: &str, value: JsValue| {
                Reflect::set(&object, &JsValue::from_str(key), &value)
                    .map(drop)
                    .map_err(|_| JsError::new("cannot build the response"))
            };
            set("status", JsValue::from(response.status))?;
            set("contentType", JsValue::from_str(&response.content_type))?;
            set("body", Uint8Array::from(response.body.as_slice()).into())?;
            Ok(object.into())
        }

        /// The shared settings the host kept (JSON), before `open`.
        #[wasm_bindgen(js_name = setSettings)]
        pub fn set_settings(&mut self, json: &str) -> Result<(), JsError> {
            self.0.set_settings(json).map_err(|e| JsError::new(&e))
        }

        /// The shared settings as JSON.
        #[wasm_bindgen(js_name = settingsJson)]
        pub fn settings_json(&self) -> String {
            self.0.settings_json()
        }

        /// What changed since the last call: `{ files: string[], settings: boolean }`.
        #[wasm_bindgen(js_name = takeChanges)]
        pub fn take_changes(&self) -> Result<JsValue, JsError> {
            let changes = self.0.take_changes();
            let files: Array = changes.files.iter().map(|f| JsValue::from_str(f)).collect();
            let object = Object::new();
            let set = |key: &str, value: JsValue| {
                Reflect::set(&object, &JsValue::from_str(key), &value)
                    .map(drop)
                    .map_err(|_| JsError::new("cannot build the changes"))
            };
            set("files", files.into())?;
            set("settings", JsValue::from_bool(changes.settings))?;
            Ok(object.into())
        }

        /// A vault file; `undefined`: it is gone.
        #[wasm_bindgen(js_name = readFile)]
        pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
            self.0.read_file(path)
        }

        /// One sync round with the hub at `hub` (`""`: the page's server),
        /// blocking the worker: the result as JSON (`round_json`).
        pub fn sync(&self, hub: &str) -> String {
            let remote = crate::xhr::Xhr::remote(hub, self.0.name.as_str());
            super::round_json(&self.0.sync(&remote))
        }

        /// The sync state the host kept (`sync.json`).
        #[wasm_bindgen(js_name = setSyncState)]
        pub fn set_sync_state(&self, json: &str) -> Result<(), JsError> {
            self.0.set_sync_state(json).map_err(|e| JsError::new(&e))
        }

        /// The sync state JSON if it changed since the last call.
        #[wasm_bindgen(js_name = takeSyncState)]
        pub fn take_sync_state(&self) -> Option<String> {
            self.0.take_sync_state()
        }

        /// What sync replaced or removed: `[path, Uint8Array][]`.
        #[wasm_bindgen(js_name = takeRemoved)]
        pub fn take_removed(&self) -> Array {
            self.0
                .take_removed()
                .into_iter()
                .map(|(path, data)| Array::of2(&JsValue::from_str(&path), &Uint8Array::from(data.as_slice())))
                .collect()
        }

        #[wasm_bindgen(js_name = releaseMemory)]
        pub fn release_memory(&self) {
            self.0.release_memory();
        }
    }
}
