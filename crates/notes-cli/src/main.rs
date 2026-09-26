//! `notes` — заметки на Typst из командной строки.
//!
//!   notes serve            локальный сервер с клиентом (http://127.0.0.1:8421)
//!   notes check            ошибки компиляции и битые ссылки во всём хранилище
//!   notes build <каталог>  статический сайт (для VPS без сервера)
//!   notes pdf <заметка>    заметка в PDF (вид PDF из baluk)
//!
//! Данные — в `--data` (по умолчанию `./data`): `vault/` и `settings.json`;
//! хранилище можно указать отдельно: `--vault tests/vault`.

mod build;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use notes_core::check::check;
use notes_core::settings::{Schema, SettingsStore};
use notes_core::{LibrarySource, NoteId, Notes, NotesConfig};

#[derive(Debug, Parser)]
#[command(version, about = "Заметки на Typst")]
struct Cli {
    /// Каталог данных: хранилище vault/ и settings.json.
    #[arg(long, global = true, env = "NOTES_DATA", default_value = "data")]
    data: PathBuf,

    /// Хранилище, если не <data>/vault (например, tests/vault).
    #[arg(long, global = true, env = "NOTES_VAULT")]
    vault: Option<PathBuf>,

    /// Библиотека оформления (baluk/), видна заметкам как /_baluk/.
    /// По умолчанию — встроенная в бинарник; в отладочной сборке — каталог
    /// baluk/ репозитория (правки видны сразу).
    #[arg(long, global = true, env = "NOTES_LIBRARY")]
    library: Option<PathBuf>,

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
        /// Токен доступа: без него сервер отвечает 401. Передаётся заголовком
        /// `Authorization: Bearer …`, параметром `?token=` (сервер ставит
        /// cookie) или cookie `notes_token`. Нужен встроенному серверу Tauri.
        #[arg(long, env = "NOTES_TOKEN", hide_env_values = true)]
        token: Option<String>,
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
    /// Заметку или книгу — в PDF.
    Pdf {
        /// Путь заметки от корня хранилища: «Сеть/SSH», «Конспекты/Матан».
        id: String,
        /// Файл результата; по умолчанию — <имя заметки>.pdf здесь.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Тема; по умолчанию — первая (светлая).
        #[arg(long)]
        theme: Option<String>,
    },
}

/// Библиотека оформления: явно указанная, в отладочной сборке — из
/// репозитория, иначе — встроенная.
fn library(explicit: Option<&PathBuf>) -> LibrarySource {
    if let Some(dir) = explicit {
        return LibrarySource::Dir(dir.clone());
    }
    let repo = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../baluk"));
    if cfg!(debug_assertions) && repo.join("lib.typ").is_file() {
        LibrarySource::Dir(repo)
    } else {
        LibrarySource::Embedded
    }
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
    let config = NotesConfig {
        vault,
        library: library(cli.library.as_ref()),
        font_dirs: cli.font_paths.clone(),
        cache: Some(notes_core::cache::default_dir(&cli.data)),
    };
    let notes = Notes::open(&config).context("открыть хранилище")?;
    tracing::info!(ms = started.elapsed().as_millis(), "хранилище {}", notes.vault().root().display());

    match cli.command {
        Command::Serve { addr, token } => serve(notes, &cli.data, addr, token),
        Command::Check { json } => run_check(&notes, json),
        Command::Build { out } => {
            // Рисунки — с той же точностью, что выбрана в приложении.
            let opts = open_settings(&notes, &cli.data)?.figure_options();
            build::build(&notes, &out, opts).map(|()| ExitCode::SUCCESS)
        }
        Command::Pdf { id, out, theme } => pdf(&notes, &id, out, theme),
    }
}

fn open_settings(notes: &Notes, data: &std::path::Path) -> Result<SettingsStore> {
    let schema = Schema::new(notes.themes().themes());
    SettingsStore::open(data.join("settings.json"), schema).context("настройки")
}

fn serve(notes: Notes, data: &std::path::Path, addr: SocketAddr, token: Option<String>) -> Result<ExitCode> {
    let settings = open_settings(&notes, data)?;
    let state = notes_server::AppState::new(Arc::new(notes), Arc::new(settings)).with_token(token);
    if state.token.is_some() {
        tracing::info!("доступ — только с токеном");
    }
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

fn pdf(notes: &Notes, id: &str, out: Option<PathBuf>, theme: Option<String>) -> Result<ExitCode> {
    let id = NoteId::new(id)?;
    let theme = theme.unwrap_or_else(|| notes.themes().names().first().cloned().unwrap_or_default());
    let out = out.unwrap_or_else(|| PathBuf::from(format!("{}.pdf", id.name())));
    match notes.pdf(&id, &theme)? {
        Ok(bytes) => {
            std::fs::write(&out, bytes).with_context(|| format!("записать {}", out.display()))?;
            println!("{} → {}", id, out.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(errors) => {
            for e in errors {
                eprintln!("{id}: {e}");
            }
            Ok(ExitCode::FAILURE)
        }
    }
}

fn run_check(notes: &Notes, json: bool) -> Result<ExitCode> {
    let report = check(notes)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
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
        }
        println!("{}", report.summary());
    }
    Ok(if report.is_clean() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}
