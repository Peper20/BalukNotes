//! `notes-app`: the app window (Tauri 2) without Typst (architecture §1).
//! `notes app [--vault <name|path>] [<note>]` (`launch`) opens the `app/`
//! client, which uses its own URL scheme `notes://localhost/...`; the window
//! passes each request to the core, `notes-typst serve`, over a Unix socket
//! (`proxy`). If the core is already running (the `notes service` service),
//! the window connects to it; otherwise it starts its own and stops it on exit
//! (`core`). External links and PDFs open in system programs (`external`).
//!
//! There is one app per user session (`instance`): a second `notes app`
//! hands its start page to the running one and exits. The running app shows
//! the page in a window (`windows`): a vault is shown by at most one window,
//! and a vault no window shows opens in a new window or in the last used one,
//! by the core's setting `vaults.open`. The last window closed quits the app.
//! `NOTES_APP_START=/v/<vault>/n/<note>` (checks, debugging) is the start page
//! when the command line names no vault.

mod core;
mod external;
mod instance;
mod launch;
mod proxy;
mod windows;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use tauri::RunEvent;

use instance::{Claim, Instance, Message};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    if let Err(e) = notes::check_version("notes-app") {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    let Ok(args) = std::env::args_os().skip(1).map(OsString::into_string).collect::<Result<Vec<_>, _>>() else {
        eprintln!("error: the arguments must be UTF-8\n\n{}", launch::USAGE);
        return ExitCode::from(2);
    };
    let launch = match launch::parse(&args) {
        Ok(launch::Command::Run(launch)) => launch,
        Ok(launch::Command::Help) => {
            let _ = writeln!(std::io::stdout(), "{}", launch::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{}", launch::USAGE);
            return ExitCode::from(2);
        }
    };
    let mut start = launch.start;
    if !launch.has_vault
        && let Some(path) = std::env::var("NOTES_APP_START").ok().filter(|p| !p.is_empty())
    {
        start = format!("/{}", path.trim_start_matches('/'));
    }
    let Some(socket) = notes::app_socket() else {
        eprintln!("error: no $XDG_RUNTIME_DIR");
        return ExitCode::FAILURE;
    };
    // Before the core starts: a second launch only hands its page over.
    let token = std::env::var("XDG_ACTIVATION_TOKEN").ok().filter(|t| !t.is_empty());
    let instance = match Instance::claim(&socket, &Message { path: start.clone(), token }) {
        Ok(Claim::Handed) => {
            tracing::info!("the app is already running: {start} handed over");
            return ExitCode::SUCCESS;
        }
        Ok(Claim::Owner(instance)) => Arc::new(instance),
        Err(e) => {
            eprintln!("error: app socket {}: {e}", socket.display());
            return ExitCode::FAILURE;
        }
    };
    let core = match core::Core::start(&launch.core_args) {
        Ok(core) => core,
        Err(e) => {
            eprintln!("error: core: {e}");
            return ExitCode::FAILURE;
        }
    };
    let core_socket = Arc::new(core.socket().to_owned());
    match run(core_socket, (core, instance), start) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: window: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the app until the last window closes. `owned`: what stops with the
/// app - our own core and the socket of the instance.
fn run(socket: Arc<PathBuf>, owned: (core::Core, Arc<Instance>), start: String) -> tauri::Result<()> {
    let scheme_socket = socket.clone();
    let instance = owned.1.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().with_state_flags(windows::WINDOW_STATE).build())
        .manage(windows::State::new(socket))
        .register_asynchronous_uri_scheme_protocol(external::SCHEME, move |_ctx, request, responder| {
            let socket = scheme_socket.clone();
            tauri::async_runtime::spawn(async move { responder.respond(proxy::forward(&socket, request).await) });
        })
        .on_window_event(windows::on_window_event)
        .setup(move |app| {
            // SIGTERM and Ctrl+C quit cleanly (our own core stops) instead of
            // killing the process and orphaning the core.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                stop_signal().await;
                windows::quit(&handle);
            });
            windows::create(app.handle(), windows::FIRST, &start)?;
            // After the first window exists: a launch that came while the app
            // was starting waits in the queue until now.
            let handle = app.handle().clone();
            instance.serve(move |message| {
                tracing::info!("second launch handed over: {}", message.path);
                windows::open(&handle, &message.path, message.token.as_deref(), || {
                    windows::open_in_new_window(&handle)
                });
            })?;
            Ok(())
        })
        .build(tauri::generate_context!())?;
    let owned = Mutex::new(Some(owned));
    app.run(move |_, event| {
        if let RunEvent::Exit = event {
            // Stop our own core and remove the socket; the service's core keeps running.
            drop(owned.lock().unwrap_or_else(PoisonError::into_inner).take());
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
