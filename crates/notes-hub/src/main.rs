//! `notes-hub`: the storage server online and its accounts. Started through
//! the thin `notes` (`notes hub serve`, `notes users add <login>`), which hands
//! over the whole argument list.
//!
//! ```text
//! notes hub serve [--addr 127.0.0.1:8422] [--session-days 30]
//! notes users list | add <login> | passwd <login> | remove <login>
//! ```
//!
//! The data directory (`--data`, `NOTES_DATA`, the config file) holds
//! `users.json`, `sessions.json` and `hub/<login>/<vault>/`.

use std::io::BufRead as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use notes_hub::{DEFAULT_ADDR, Hub};
use notes_store::accounts::Accounts;
use notes_store::config::{self, hub_dir, sessions_file, users_file};
use notes_store::sessions::Sessions;

/// `println!` that exits quietly (code 0) when the reader closed the pipe
/// (`notes users list | head`): Rust ignores SIGPIPE, and `println!` would panic.
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::write_out(format_args!($($arg)*))
    };
}

fn write_out(args: std::fmt::Arguments<'_>) {
    use std::io::Write as _;
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = stdout.write_fmt(args).and_then(|()| stdout.write_all(b"\n")) {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("error: writing the output: {e}");
        std::process::exit(1);
    }
}

#[derive(Debug, Parser)]
#[command(name = "notes", bin_name = "notes", version, about = "The storage server online and its accounts")]
struct Cli {
    /// Data directory: users.json, sessions.json, hub/. Default: `data` from
    /// ~/.config/baluk-notes/config.toml, else ~/.local/share/baluk-notes.
    #[arg(long, global = true, env = "NOTES_DATA")]
    data: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// The storage server: vault files and versions, sign-in, sync.
    Hub {
        #[command(subcommand)]
        action: HubAction,
    },
    /// Accounts of the storage server (each has its own vaults).
    Users {
        #[command(subcommand)]
        action: UsersAction,
    },
}

#[derive(Debug, Subcommand)]
enum HubAction {
    /// Runs the server on HTTP; HTTPS is the job of a reverse proxy.
    Serve {
        /// Address to listen on. Keep it on localhost: the proxy (nginx) in
        /// front of it does HTTPS and sends X-Forwarded-Proto and Host.
        #[arg(long, default_value = DEFAULT_ADDR)]
        addr: SocketAddr,
        /// A session ends after this many days without requests.
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=3650))]
        session_days: u64,
    },
}

#[derive(Debug, Subcommand)]
enum UsersAction {
    /// The logins.
    List,
    /// Adds an account.
    Add(Who),
    /// Changes a password and ends the sessions of the account.
    Passwd(Who),
    /// Removes an account and ends its sessions; its vaults stay on the disk.
    Remove { login: String },
}

#[derive(Debug, Args)]
struct Who {
    login: String,
    /// Read the password from a line of stdin instead of asking twice on the
    /// terminal (scripts).
    #[arg(long)]
    password_stdin: bool,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,notes=info".into()),
        )
        .with_target(false)
        // No colors outside a terminal: the service log (journald), a file.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_writer(std::io::stderr)
        .init();
    if let Err(e) = notes::check_version("notes-hub") {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let data = config::data_dir(cli.data.as_deref())?;
    match cli.command {
        Command::Hub { action: HubAction::Serve { addr, session_days } } => serve(&data, addr, session_days),
        Command::Users { action } => users(&data, action),
    }
}

fn serve(data: &Path, addr: SocketAddr, session_days: u64) -> Result<()> {
    let hub = Hub::open(data, Duration::from_secs(session_days * 24 * 3600))?;
    if hub.auth().accounts().is_empty()? {
        bail!("no users: create one: notes users add <login>");
    }
    if !addr.ip().is_loopback() {
        eprintln!(
            "warning: {addr} is not a localhost address: the server speaks HTTP only, so passwords travel in the clear; do HTTPS with a reverse proxy and listen on localhost"
        );
    }
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("bind {addr}"))?;
        tracing::info!("hub: http://{addr}/ data: {}", data.display());
        notes_hub::serve(listener, hub, stop_signal()).await?;
        tracing::info!("stopped");
        Ok(())
    })
}

/// Ctrl+C or SIGTERM (how systemd stops a service).
async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn users(data: &Path, action: UsersAction) -> Result<()> {
    let accounts = Accounts::open(users_file(data))?;
    // No lifetime: this command only ends sessions, it never prunes the old ones.
    let sessions = || Sessions::open(sessions_file(data), Duration::from_secs(u64::MAX));
    match action {
        UsersAction::List => {
            for login in accounts.list()? {
                out!("{login}");
            }
        }
        UsersAction::Add(who) => {
            let password = read_password(who.password_stdin)?;
            accounts.add(&who.login, &password)?;
            out!("user \"{}\" added", who.login.to_lowercase());
        }
        UsersAction::Passwd(who) => {
            let password = read_password(who.password_stdin)?;
            accounts.set_password(&who.login, &password)?;
            let ended = sessions()?.revoke_user(&who.login.to_lowercase())?;
            out!("password of \"{}\" changed; sessions ended: {ended}", who.login.to_lowercase());
        }
        UsersAction::Remove { login } => {
            accounts.remove(&login)?;
            let login = login.to_lowercase();
            let ended = sessions()?.revoke_user(&login)?;
            let vaults = hub_dir(data).join(&login);
            let kept = if vaults.is_dir() { format!("; vaults kept in {}", vaults.display()) } else { String::new() };
            out!("user \"{login}\" removed; sessions ended: {ended}{kept}");
        }
    }
    Ok(())
}

/// The password: a line of stdin, or asked twice without echo.
fn read_password(from_stdin: bool) -> Result<String> {
    if from_stdin {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).context("read the password from stdin")?;
        return Ok(line.trim_end_matches(['\n', '\r']).to_owned());
    }
    let first = rpassword::prompt_password("Password: ").context("read the password")?;
    let second = rpassword::prompt_password("Repeat the password: ").context("read the password")?;
    if first != second {
        bail!("the passwords differ");
    }
    Ok(first)
}
