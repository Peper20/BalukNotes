//! `notes-app` - окно приложения (Tauri 2) без Typst (architecture §1).
//! Интерфейс `app/` ходит своей схемой адресов `notes://localhost/...`, окно
//! передаёт запрос ядру - `notes-typst serve` - через сокет Unix (`proxy`).
//! Ядро уже работает (служба `notes service`) - окно подключается к нему,
//! нет - запускает своё и останавливает при выходе (`core`). Внешние ссылки
//! и PDF открывают программы системы.

mod core;
mod proxy;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use tauri::webview::NewWindowResponse;
use tauri::{RunEvent, Url, WebviewUrl, WebviewWindowBuilder};

/// Схема адресов окна.
const SCHEME: &str = "notes";

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    if let Err(e) = notes::check_version("notes-app") {
        eprintln!("ошибка: {e}");
        return ExitCode::FAILURE;
    }
    // `notes app --data …`: команда `app` - не флаг ядра, остальное - ему.
    let mut args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == "app") {
        args.remove(i);
    }
    let core = match core::Core::start(&args) {
        Ok(core) => core,
        Err(e) => {
            eprintln!("ошибка: ядро: {e}");
            return ExitCode::FAILURE;
        }
    };
    let socket = Arc::new(core.socket().to_owned());
    match run(socket, core) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: окно: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(socket: Arc<PathBuf>, core: core::Core) -> tauri::Result<()> {
    let scheme_socket = socket.clone();
    let app = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol(SCHEME, move |_ctx, request, responder| {
            let socket = scheme_socket.clone();
            tauri::async_runtime::spawn(async move { responder.respond(proxy::forward(&socket, request).await) });
        })
        .setup(move |app| {
            // SIGTERM и Ctrl+C - штатный выход (своё ядро останавливается), а не
            // смерть процесса с ядром-сиротой.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                stop_signal().await;
                handle.exit(0);
            });
            // `NOTES_APP_START=/v/<хранилище>/n/<заметка>` - открыть с неё (проверка, отладка).
            let path = std::env::var("NOTES_APP_START").unwrap_or_default();
            let start: Url = format!("{SCHEME}://localhost/{}", path.trim_start_matches('/')).parse()?;
            WebviewWindowBuilder::new(app, "main", WebviewUrl::CustomProtocol(start))
                .title("baluk notes")
                .inner_size(1300.0, 900.0)
                .on_navigation(|url| allowed(url) || open_external(url))
                .on_new_window(move |url, _| {
                    if url.scheme() == SCHEME && url.path().contains("/pdf/") {
                        open_pdf(socket.clone(), url);
                    } else if !allowed(&url) {
                        open_external(&url);
                    }
                    NewWindowResponse::Deny
                })
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())?;
    let core = Mutex::new(Some(core));
    app.run(move |_, event| {
        if let RunEvent::Exit = event {
            // Своё ядро - остановить; ядро службы продолжает работать.
            drop(core.lock().unwrap_or_else(PoisonError::into_inner).take());
        }
    });
    Ok(())
}

async fn stop_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let (Ok(mut term), Ok(mut int)) = (signal(SignalKind::terminate()), signal(SignalKind::interrupt())) else {
        return std::future::pending().await;
    };
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
}

/// Адреса самого приложения - в окне; остальное - нет.
fn allowed(url: &Url) -> bool {
    url.scheme() == SCHEME || url.scheme() == "about"
}

/// Внешняя ссылка - в программе системы (браузер, почта); окно остаётся.
fn open_external(url: &Url) -> bool {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        system_open(url.as_str());
    } else {
        tracing::warn!("ссылка {url} не открыта: схема не поддерживается");
    }
    false
}

/// PDF заметки: WebKitGTK его не показывает - собрать через ядро, сохранить
/// во временную папку и открыть программой системы.
fn open_pdf(socket: Arc<PathBuf>, url: Url) {
    tauri::async_runtime::spawn(async move {
        let name = url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned())
            .unwrap_or_default();
        let request = match tauri::http::Request::get(url.as_str()).body(Vec::new()) {
            Ok(request) => request,
            Err(e) => {
                tracing::warn!("PDF «{name}»: bad address {url}: {e}");
                return;
            }
        };
        let response = proxy::forward(&socket, request).await;
        if !response.status().is_success() {
            let body = String::from_utf8_lossy(response.body());
            tracing::warn!("PDF «{name}» не собран: {} {body}", response.status());
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

/// Имя файла без разделителей пути.
fn safe_name(name: &str) -> String {
    let name: String = name.chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '_' } else { c }).collect();
    if name.trim().is_empty() { "заметка".into() } else { name }
}

fn system_open(target: &str) {
    if let Err(e) = std::process::Command::new("xdg-open").arg(target).spawn() {
        tracing::warn!("xdg-open {target}: {e}");
    }
}
