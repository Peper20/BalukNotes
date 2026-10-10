//! The app's windows. Labels are `main`, `w2`, `w3`, ...; one function builds
//! them all. A window shows one vault at a time (its URL starts with
//! `/v/<vault>/`), and a vault is shown by at most one window: asking for a
//! vault that a window already shows focuses that window instead of opening
//! another (`open`, `on_navigation`). Closing a window that is not the last
//! one closes only it; the last one quits the app (`quit`).

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gtk::prelude::GtkWindowExt;
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};
use webkit2gtk::WebViewExt;

use crate::external::{self, SCHEME};
use crate::launch;
use crate::proxy;

/// What a window keeps between launches: maximized or fullscreen, not size
/// or position.
pub const WINDOW_STATE: StateFlags = StateFlags::MAXIMIZED.union(StateFlags::FULLSCREEN);

/// The label of the first window.
pub const FIRST: &str = "main";

/// What the windows share: the core's socket, the label counter and which
/// window had the focus last.
#[derive(Debug)]
pub struct State {
    socket: Arc<PathBuf>,
    counter: AtomicU32,
    focused: Mutex<Option<String>>,
    /// The address each window was built on: a window that has not loaded yet
    /// has no URL to tell its vault by.
    initial: Mutex<HashMap<String, String>>,
}

impl State {
    pub fn new(socket: Arc<PathBuf>) -> Self {
        Self { socket, counter: AtomicU32::new(2), focused: Mutex::new(None), initial: Mutex::new(HashMap::new()) }
    }

    fn next_label(&self) -> String {
        format!("w{}", self.counter.fetch_add(1, Ordering::Relaxed))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The client's address on the `notes` scheme.
fn address(path: &str) -> Result<Url, Box<dyn Error>> {
    Ok(format!("{SCHEME}://localhost{path}").parse()?)
}

/// Builds a window on the client's address `path` (`/`, `/v/<vault>/...`).
pub fn create(app: &AppHandle, label: &str, path: &str) -> Result<WebviewWindow, Box<dyn Error>> {
    let url = address(path)?;
    let (nav_app, nav_label) = (app.clone(), label.to_owned());
    let new_app = app.clone();
    lock(&app.state::<State>().initial).insert(label.to_owned(), path.to_owned());
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::CustomProtocol(url))
        .title("baluk notes")
        .inner_size(1300.0, 900.0)
        .on_navigation(move |url| on_navigation(&nav_app, &nav_label, url))
        .on_new_window(move |url, _| {
            on_new_window(&new_app, &url);
            NewWindowResponse::Deny
        })
        .build()?;
    tracing::info!("window {label} created: {path}");
    Ok(window)
}

fn create_next(app: &AppHandle, path: &str) -> Result<WebviewWindow, Box<dyn Error>> {
    let label = app.state::<State>().next_label();
    create(app, &label, path)
}

/// Navigation of a window: our own addresses stay in the window, except a
/// vault that another window shows - that one is focused and this navigation
/// is cancelled. The window's own loads and reloads pass: only other windows
/// are looked at.
fn on_navigation(app: &AppHandle, label: &str, url: &Url) -> bool {
    if !external::allowed(url) {
        return external::open_external(url);
    }
    if url.scheme() == SCHEME
        && let Some(vault) = launch::vault_of_path(url.path())
        && let Some(other) = showing(app, &vault, Some(label))
    {
        tracing::info!("navigation of {label} to {url} cancelled: vault \"{vault}\" is open in {}", other.label());
        focus(&other, None);
        return false;
    }
    true
}

/// `window.open` of the client: a PDF goes to a system program, our own
/// address to a new window (or the window that shows its vault), anything
/// else outside.
fn on_new_window(app: &AppHandle, url: &Url) {
    if external::is_pdf(url) {
        external::open_pdf(app.state::<State>().socket.clone(), url.clone());
    } else if url.scheme() == SCHEME {
        let (app, path) = (app.clone(), url.path().to_owned());
        // Not on the main thread: building a window waits for it.
        tauri::async_runtime::spawn_blocking(move || open(&app, &path, None, || true));
    } else if !external::allowed(url) {
        external::open_external(url);
    }
}

/// What to do with a request to open a client address.
#[derive(Debug, PartialEq, Eq)]
enum Plan {
    /// No vault named: focus the window that had the focus last.
    FocusLast,
    /// A window shows the vault; navigate it if the address names a note.
    Focus { navigate: bool },
    /// A new window.
    New,
    /// The window that had the focus last goes to the address.
    NavigateLast,
}

/// `shown`: some window shows the address's vault. `new_window` tells the
/// setting `vaults.open` (is called only when it matters).
fn plan(path: &str, shown: bool, new_window: impl FnOnce() -> bool) -> Plan {
    if launch::vault_of_path(path).is_none() {
        Plan::FocusLast
    } else if shown {
        Plan::Focus { navigate: launch::names_note(path) }
    } else if new_window() {
        Plan::New
    } else {
        Plan::NavigateLast
    }
}

/// Opens the client address `path`; `token` is the activation token of the
/// launch that asked. See `Plan`.
pub fn open(app: &AppHandle, path: &str, token: Option<&str>, new_window: impl FnOnce() -> bool) {
    let shown = launch::vault_of_path(path).and_then(|vault| showing(app, &vault, None));
    let window = match plan(path, shown.is_some(), new_window) {
        Plan::FocusLast => {
            if let Some(window) = last_focused(app) {
                focus(&window, token);
            } else {
                tracing::warn!("no window to focus for {path}");
            }
            return;
        }
        Plan::Focus { navigate } => shown.inspect(|window| {
            if navigate {
                navigate_to(window, path);
            }
        }),
        Plan::NavigateLast => last_focused(app).inspect(|window| navigate_to(window, path)),
        Plan::New => None,
    };
    match window.map_or_else(|| create_next(app, path), Ok) {
        Ok(window) => focus(&window, token),
        Err(e) => tracing::warn!("window for {path} not created: {e}"),
    }
}

/// The setting `vaults.open` of the core is `"new"`: a vault that no window
/// shows opens in a new window (otherwise in the last used one). The core not
/// answering means `"this"`.
pub fn open_in_new_window(app: &AppHandle) -> bool {
    let socket = app.state::<State>().socket.clone();
    let Ok(request) = tauri::http::Request::get("/api/settings").body(Vec::new()) else { return false };
    let response = tauri::async_runtime::block_on(proxy::forward(&socket, request));
    response.status().is_success() && setting_is_new(response.body())
}

/// `GET /api/settings`: `{"values": {"vaults.open": "this" | "new", ...}}`.
fn setting_is_new(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.pointer("/values/vaults.open").and_then(|s| s.as_str()).map(|s| s == "new"))
        .unwrap_or(false)
}

/// The window that shows `vault` (other than `except`).
fn showing(app: &AppHandle, vault: &str, except: Option<&str>) -> Option<WebviewWindow> {
    let windows = app.webview_windows();
    let mut windows: Vec<_> = windows.into_values().filter(|w| Some(w.label()) != except).collect();
    windows.sort_by(|a, b| a.label().cmp(b.label()));
    windows.into_iter().find(|window| vault_of(app, window).as_deref() == Some(vault))
}

/// The vault a window shows: by its URL, or by the address it was built on
/// while it has no URL of our scheme yet.
fn vault_of(app: &AppHandle, window: &WebviewWindow) -> Option<String> {
    match window.url() {
        Ok(url) if url.scheme() == SCHEME => launch::vault_of_path(url.path()),
        _ => lock(&app.state::<State>().initial).get(window.label()).and_then(|path| launch::vault_of_path(path)),
    }
}

/// The window that had the focus last, else any.
fn last_focused(app: &AppHandle) -> Option<WebviewWindow> {
    let label = lock(&app.state::<State>().focused).clone();
    if let Some(window) = label.and_then(|l| app.get_webview_window(&l)) {
        return Some(window);
    }
    let windows = app.webview_windows();
    windows.into_values().min_by(|a, b| a.label().cmp(b.label()))
}

fn navigate_to(window: &WebviewWindow, path: &str) {
    if window.url().is_ok_and(|url| url.path() == path) {
        return;
    }
    match address(path) {
        Ok(url) => {
            tracing::info!("window {} goes to {path}", window.label());
            if let Err(e) = window.navigate(url) {
                tracing::warn!("window {} not navigated to {path}: {e}", window.label());
            }
        }
        Err(e) => tracing::warn!("bad address {path}: {e}"),
    }
}

/// Brings a window to the front. On Wayland a window cannot take the focus
/// without an activation token: the GTK window gets it before it is presented
/// (best effort).
fn focus(window: &WebviewWindow, token: Option<&str>) {
    let (target, token) = (window.clone(), token.map(str::to_owned));
    let queued = window.app_handle().run_on_main_thread(move || {
        if let Some(token) = &token {
            match target.gtk_window() {
                Ok(gtk_window) => gtk_window.set_startup_id(token),
                Err(e) => tracing::warn!("activation token not set: {e}"),
            }
        }
        if let Err(e) = target.unminimize().and_then(|()| target.set_focus()) {
            tracing::warn!("window {} not focused: {e}", target.label());
        } else {
            tracing::info!("window {} focused", target.label());
        }
    });
    if let Err(e) = queued {
        tracing::warn!("window {} not focused: {e}", window.label());
    }
}

/// Window events of the whole app.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let app = window.app_handle();
    match event {
        WindowEvent::Focused(true) => *lock(&app.state::<State>().focused) = Some(window.label().to_owned()),
        WindowEvent::Destroyed => {
            let state = app.state::<State>();
            lock(&state.initial).remove(window.label());
            let mut focused = lock(&state.focused);
            if focused.as_deref() == Some(window.label()) {
                *focused = None;
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            close(app, window.label());
        }
        _ => {}
    }
}

/// Closes a window: the last one quits the app, another one goes alone.
fn close(app: &AppHandle, label: &str) {
    if app.webview_windows().len() <= 1 {
        quit(app);
        return;
    }
    save_state(app);
    let Some(window) = app.get_webview_window(label) else { return };
    let target = window.clone();
    // The web process is killed before the window goes (see `quit`).
    let queued = window.with_webview(move |webview| {
        webview.inner().terminate_web_process();
        if let Err(e) = target.destroy() {
            tracing::warn!("window {} not closed: {e}", target.label());
        }
    });
    if let Err(e) = queued {
        tracing::warn!("WebKit process of {label} not stopped before closing: {e}");
        let _ = window.destroy();
    }
    tracing::info!("window {label} closed");
}

/// Quits the app, killing the windows' WebKit processes first (WebKit sends
/// them SIGKILL). Left to exit by itself, such a process runs `exit()` while
/// its render threads are still inside EGL and sometimes crashes in the libEGL
/// destructor (hybrid graphics with NVIDIA, `docs/tech-debt.md`). The client
/// saves its state as it goes, so nothing waits for `pagehide`.
pub fn quit(app: &AppHandle) {
    save_state(app);
    let windows: Vec<WebviewWindow> = app.webview_windows().into_values().collect();
    if windows.is_empty() {
        app.exit(0);
        return;
    }
    // The last process stopped exits the app.
    let left = Arc::new(AtomicU32::new(u32::try_from(windows.len()).unwrap_or(u32::MAX)));
    for window in windows {
        let (counter, handle) = (left.clone(), app.clone());
        let done = move || {
            if counter.fetch_sub(1, Ordering::SeqCst) == 1 {
                handle.exit(0);
            }
        };
        let finish = done.clone();
        let queued = window.with_webview(move |webview| {
            webview.inner().terminate_web_process();
            done();
        });
        if let Err(e) = queued {
            tracing::warn!("WebKit process of {} not stopped before exit: {e}", window.label());
            finish();
        }
    }
}

fn save_state(app: &AppHandle) {
    if let Err(e) = app.save_window_state(WINDOW_STATE) {
        tracing::warn!("window state not saved: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_by_vault_and_setting() {
        let never = || -> bool { panic!("the setting is not needed") };
        assert_eq!(plan("/", false, never), Plan::FocusLast);
        assert_eq!(plan("/", true, never), Plan::FocusLast);
        assert_eq!(plan("/v/A/n/x", true, never), Plan::Focus { navigate: true });
        assert_eq!(plan("/v/A/", true, never), Plan::Focus { navigate: false });
        assert_eq!(plan("/v/A/n/x", false, || true), Plan::New);
        assert_eq!(plan("/v/A/", false, || false), Plan::NavigateLast);
    }

    #[test]
    fn setting_is_read_from_the_core_answer() {
        assert!(setting_is_new(br#"{"schema": {}, "values": {"vaults.open": "new", "x": 1}}"#));
        assert!(!setting_is_new(br#"{"values": {"vaults.open": "this"}}"#));
        assert!(!setting_is_new(br#"{"values": {}}"#));
        assert!(!setting_is_new(br#"{"error": "no"}"#));
        assert!(!setting_is_new(b"not json"));
    }

    #[test]
    fn labels_count_up() {
        let state = State::new(Arc::new(PathBuf::new()));
        assert_eq!(state.next_label(), "w2");
        assert_eq!(state.next_label(), "w3");
    }
}
