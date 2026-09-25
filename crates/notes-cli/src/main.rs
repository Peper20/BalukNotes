//! `notes` — заметки на Typst из командной строки.
//!
//!   notes serve            локальный сервер с клиентом (http://127.0.0.1:8421)
//!   notes check            ошибки компиляции и битые ссылки во всём хранилище
//!   notes build <каталог>  статический сайт (для VPS без сервера)
//!
//! Данные — в `--data` (по умолчанию `./data`): `vault/` и `settings.json`;
//! хранилище можно указать отдельно: `--vault examples/vault`.

mod build;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use notes_core::check::check;
use notes_core::settings::{Schema, SettingsStore};
use notes_core::{Notes, NotesConfig};

#[derive(Debug, Parser)]
#[command(version, about = "Заметки на Typst")]
struct Cli {
    /// Каталог данных: хранилище vault/ и settings.json.
    #[arg(long, global = true, env = "NOTES_DATA", default_value = "data")]
    data: PathBuf,

    /// Хранилище, если не <data>/vault (например, examples/vault).
    #[arg(long, global = true, env = "NOTES_VAULT")]
    vault: Option<PathBuf>,

    /// Библиотека оформления (konspekt/), видна заметкам как /_konspekt/.
    #[arg(long, global = true, env = "NOTES_LIBRARY", default_value = default_library())]
    library: PathBuf,

    /// Дополнительный каталог шрифтов (можно повторять).
    #[arg(long = "font-path", global = true, env = "NOTES_FONT_PATHS", value_delimiter = ':')]
    font_paths: Vec<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Локальный сервер с клиентом.
    Serve {
        /// Адрес; для доступа из сети — 0.0.0.0:8421.
        #[arg(long, default_value = "127.0.0.1:8421")]
        addr: SocketAddr,
    },
    /// Проверить хранилище: ошибки компиляции и битые ссылки (код выхода 1).
    Check {
        /// Отчёт в JSON.
        #[arg(long)]
        json: bool,
    },
    /// Собрать статический сайт.
    Build {
        /// Каталог результата (создаётся; существующие файлы перезаписываются).
        out: PathBuf,
    },
}

/// Библиотека из репозитория: для запуска через `cargo run`.
/// TODO(tech-debt): встроить библиотеку в релизный бинарник.
fn default_library() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../konspekt")
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,notes=info".into()),
        )
        .with_target(false)
        .init();
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ошибка: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let vault = cli.vault.clone().unwrap_or_else(|| cli.data.join("vault"));
    if !vault.exists() {
        std::fs::create_dir_all(&vault).with_context(|| format!("создать {}", vault.display()))?;
        tracing::info!("создано пустое хранилище {}", vault.display());
    }
    let started = std::time::Instant::now();
    let notes = Notes::open(&NotesConfig { vault, library: cli.library.clone(), font_dirs: cli.font_paths.clone() })
        .context("открыть хранилище")?;
    tracing::info!(ms = started.elapsed().as_millis(), "хранилище {}", notes.vault().root().display());

    match cli.command {
        Command::Serve { addr } => serve(notes, &cli.data, addr),
        Command::Check { json } => run_check(&notes, json),
        Command::Build { out } => build::build(&notes, &out).map(|()| ExitCode::SUCCESS),
    }
}

fn serve(notes: Notes, data: &std::path::Path, addr: SocketAddr) -> Result<ExitCode> {
    let schema = Schema::new(notes.themes().themes());
    let settings = SettingsStore::open(data.join("settings.json"), schema).context("настройки")?;
    let state = notes_server::AppState { notes: Arc::new(notes), settings: Arc::new(settings) };
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("занять {addr}"))?;
        tracing::info!("открыть: http://{addr}/");
        notes_server::serve(listener, state, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
        Ok(ExitCode::SUCCESS)
    })
}

fn run_check(notes: &Notes, json: bool) -> Result<ExitCode> {
    let report = check(notes)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let (mut errors, mut warnings, mut broken) = (0, 0, 0);
        for n in &report.notes {
            for e in &n.errors {
                println!("{}: {e}", n.id);
            }
            for w in &n.warnings {
                println!("{}: {w}", n.id);
            }
            for l in &n.broken_links {
                let anchor = l.anchor.as_deref().map(|a| format!(" / {a}")).unwrap_or_default();
                println!("{}: битая ссылка «{}{anchor}»: {}", n.id, l.target, l.reason);
            }
            errors += n.errors.len();
            warnings += n.warnings.len();
            broken += n.broken_links.len();
        }
        println!(
            "заметок: {}, ошибок: {errors}, предупреждений: {warnings}, битых ссылок: {broken}",
            report.notes.len()
        );
    }
    Ok(if report.is_clean() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}
