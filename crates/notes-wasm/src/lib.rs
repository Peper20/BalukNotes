//! The core in WASM for the browser (`docs/research/E4.md`, architecture §1).
//!
//! One thread, no disk: the host (a Web Worker) hands the vault files and the
//! Typst package files over as bytes, the core keeps them in memory
//! ([`MemStorage`]) and answers the HTTP API requests the host passes in
//! ([`Core::request`]) with the same `notes-server` `Router` as `notes serve`.
//! [`Core`] is plain Rust (its tests run natively); the JS binding is
//! [`wasm::WasmCore`], compiled only for `wasm32`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use notes_core::settings::{Schema, SettingsStore};
use notes_core::{LibrarySource, Notes, NotesConfig, SharedAssets};
use notes_server::{AppState, VaultSet, router};
use notes_store::names::VaultName;
use notes_store::platform::Platform;
use notes_store::storage::MemStorage;
use tower::ServiceExt;

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

/// An opened vault: the core and the router over it.
#[derive(Debug)]
struct Open {
    notes: Arc<Notes>,
    router: Router,
}

/// The core over a vault in memory.
#[derive(Debug)]
pub struct Core {
    name: VaultName,
    vault: Arc<MemStorage>,
    packages: Arc<MemStorage>,
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

/// The state the router serves: one vault, settings in memory.
fn app_state(name: VaultName, notes: Arc<Notes>) -> AppState {
    let schema = Schema::new(notes.themes().themes(), Platform::current());
    let settings = Arc::new(SettingsStore::in_memory(schema));
    AppState::new(VaultSet::single(name, notes, settings))
}

impl Core {
    /// An empty vault and no packages; `vault` is the vault name in the API
    /// paths (`/api/vaults/<vault>/...`).
    pub fn new(vault: &str) -> Result<Self, String> {
        let name = VaultName::new(vault).map_err(|e| e.to_string())?;
        Ok(Self { name, vault: Arc::new(MemStorage::new()), packages: Arc::new(MemStorage::new()), open: None })
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
    /// settings live in memory for the life of the core.
    pub fn open(&mut self) -> Result<(), String> {
        if self.open.is_some() {
            return Ok(());
        }
        let notes = open_notes(&self.vault, &self.packages)?;
        let router = router(app_state(self.name.clone(), notes.clone()));
        self.open = Some(Open { notes, router });
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

    /// Forgets what Typst memoized (there is no idle thread to do it here).
    pub fn release_memory(&self) {
        if let Some(open) = &self.open {
            open.notes.release_memory();
        }
    }
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

    fn core() -> Core {
        let mut core = Core::new(VAULT).unwrap();
        fill(&core.vault, &core.packages);
        core.open().unwrap();
        core
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
        assert_eq!(events["watching"], false);
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
        let native = router(app_state(VaultName::new(VAULT).unwrap(), notes));

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
}

/// The JS binding.
#[cfg(target_family = "wasm")]
pub mod wasm {
    use js_sys::{Object, Reflect, Uint8Array};
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

        #[wasm_bindgen(js_name = releaseMemory)]
        pub fn release_memory(&self) {
            self.0.release_memory();
        }
    }
}
