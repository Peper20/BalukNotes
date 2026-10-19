//! `notes`: the thin command (architecture §1). It runs the light commands
//! itself (`docs`, `vaults`, `sync`, `service`: no Typst, no vault opened) and
//! hands the rest to an app part in its own directory (`notes app` ->
//! `notes-app`, `notes hub` -> `notes-hub`, the rest -> `notes-typst`),
//! passing its version. Help is its own and lists every command and which
//! parts are installed; `notes --version` lists the parts' versions.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory as _, FromArgMatches as _};
use cli::{Cli, Command as Light, ServiceAction, VaultsAction};
use notes_parts::{LIGHT, PARTS, Part, TYPST, VERSION, VERSION_ENV};
use notes_store::config;
use notes_store::names::VaultName;
use notes_store::serve_rules;
use notes_store::vaults::Vaults;

/// `println!` that exits quietly (code 0) when the reader closed the pipe
/// (`notes vaults | head`): Rust ignores SIGPIPE, and `println!` would panic.
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::write_out(format_args!($($arg)*), true)
    };
}

/// `print!` with the same handling of a closed pipe as [`out!`].
macro_rules! out_raw {
    ($($arg:tt)*) => {
        $crate::write_out(format_args!($($arg)*), false)
    };
}

mod cli;
#[cfg(feature = "sync")]
mod device;
mod service;
mod sync;

/// Writes command output to stdout: a closed pipe ends the process with code
/// 0, another write error with code 1 (the output is lost anyway).
fn write_out(args: std::fmt::Arguments<'_>, newline: bool) {
    let mut stdout = std::io::stdout().lock();
    let written = stdout.write_fmt(args).and_then(|()| if newline { stdout.write_all(b"\n") } else { Ok(()) });
    if let Err(e) = written {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("error: writing the output: {e}");
        std::process::exit(1);
    }
}

/// Who runs a command line.
#[derive(Debug, PartialEq, Eq)]
enum Route {
    /// This binary: a light command, or the help (its own `clap` answers).
    Here,
    /// A part.
    Part(Part),
}

/// The command (after `notes help`, the one help is asked for) decides.
fn route<S: AsRef<str>>(args: &[S]) -> Route {
    let Some(command) = notes_parts::command(args) else { return Route::Here };
    let target = if command == "help" {
        let mut after = args.iter().map(AsRef::as_ref).skip_while(|a| *a != "help").skip(1);
        match after.find(|a| !a.starts_with('-')) {
            Some(target) => target,
            None => return Route::Here,
        }
    } else {
        command
    };
    if LIGHT.contains(&target) || target == "help" {
        Route::Here
    } else {
        Route::Part(notes_parts::part_for(&[target]))
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let text: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let dir = match notes_parts::parts_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("error: notes directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let command = notes_parts::command(&text);
    if command.is_none() && text.iter().any(|a| a == "-V" || a == "--version") {
        let _ = writeln!(std::io::stdout(), "notes {VERSION}");
        let _ = std::io::stdout().write_all(parts_list(&dir).as_bytes());
        return ExitCode::SUCCESS;
    }
    match route(&text) {
        Route::Here => run_here(&args, &dir),
        Route::Part(part) => run_part(part, &args, command.unwrap_or_default(), &dir),
    }
}

/// Hands the command line to a part; the process becomes the part.
fn run_part(part: Part, args: &[OsString], command: &str, dir: &Path) -> ExitCode {
    let path = part.path(dir);
    if !path.is_file() {
        eprintln!(
            "error: \"notes {command}\" belongs to the part {} ({}), which is not installed: no {}",
            part.bin,
            part.about,
            path.display()
        );
        return ExitCode::FAILURE;
    }
    let mut child = Command::new(&path);
    child.args(args).env(VERSION_ENV, VERSION);
    exec(child, &path)
}

/// The parts and which of them are installed (help and `--version`). Write
/// errors are ignored by the callers: `notes --help | head` closes the pipe early.
fn parts_list(dir: &Path) -> String {
    let lines = PARTS.iter().map(|part| {
        let state = if part.path(dir).is_file() { "yes" } else { "no " };
        format!("  {state}  {:<12} {}\n", part.bin, part.about)
    });
    format!("Parts (in {}):\n{}", dir.display(), lines.collect::<String>())
}

/// The command line of this binary: help, usage errors and `--version` of a
/// subcommand are answered (and the process ends) by `clap`.
fn parse(args: &[OsString], dir: &Path) -> Cli {
    let command = Cli::command().after_help(parts_list(dir));
    let matches = command.get_matches_from(std::iter::once(OsString::from("notes")).chain(args.iter().cloned()));
    Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
}

fn run_here(args: &[OsString], dir: &Path) -> ExitCode {
    let cli = parse(args, dir);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,notes=info".into()),
        )
        .with_target(false)
        // No colors outside a terminal: a log file, a pipe.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_writer(std::io::stderr)
        .init();
    match run(&cli, dir) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Data directory: explicit, from the config file, or the standard one
/// (`notes_store::config`, the rule shared by all parts).
fn data_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    // The error text already has its cause: not a chain, `{e:#}` would repeat it.
    config::data_dir(explicit).map_err(|e| anyhow::anyhow!("{e}"))
}

fn run(cli: &Cli, dir: &Path) -> Result<ExitCode> {
    match &cli.command {
        Light::Docs { topic } => {
            out_raw!("{}", topic.text());
            Ok(ExitCode::SUCCESS)
        }
        Light::Service { action } => {
            service_command(cli, action, dir)?;
            Ok(ExitCode::SUCCESS)
        }
        Light::Vaults { action } => vaults_command(&Vaults::new(&data_dir(cli.data.as_deref())?), action.as_ref()),
        Light::Sync { action } => sync_command(cli, action),
        _ => bail!("this command belongs to a part: not for the thin notes to run"),
    }
}

fn vaults_command(vaults: &Vaults, action: Option<&VaultsAction>) -> Result<ExitCode> {
    if let Some(VaultsAction::New { name }) = action {
        let name = VaultName::new(name.clone())?;
        let path = vaults.create(&name)?;
        out!("{}", path.display());
        eprintln!("in the app: notes app --vault \"{name}\"");
        return Ok(ExitCode::SUCCESS);
    }
    let list = vaults.list()?;
    for name in &list {
        out!("{name}");
    }
    if list.is_empty() {
        eprintln!("no vaults: create one: notes vaults new \"Name\" (or in the app)");
    }
    Ok(ExitCode::SUCCESS)
}

fn sync_command(cli: &Cli, action: &sync::SyncAction) -> Result<ExitCode> {
    let data = data_dir(cli.data.as_deref())?;
    sync::run(&data, cli.vault.as_deref(), action)
}

fn service_command(cli: &Cli, action: &ServiceAction, dir: &Path) -> Result<()> {
    match action {
        ServiceAction::Install { addr, auth, session_days } => {
            // The service must start: check what `serve` checks, here and now.
            serve_rules::check_sign_in(Some(*addr), *auth)?;
            if *auth {
                let lifetime = Duration::from_secs(session_days * 24 * 3600);
                serve_rules::check_accounts(&data_dir(cli.data.as_deref())?, lifetime)?;
            }
            // The unit runs the part that serves, not this thin command.
            let exe = TYPST.path(dir);
            if !exe.is_file() {
                bail!(
                    "\"notes service install\" runs the part {} ({}), which is not installed: no {}",
                    TYPST.bin,
                    TYPST.about,
                    exe.display()
                );
            }
            if cfg!(debug_assertions) {
                eprintln!(
                    "warning: a debug build; the service will run it (usually: tools/install.sh and notes from PATH)"
                );
            }
            service::install(&exe, &service_args(cli, *addr, *auth, *session_days)?, *addr)
        }
        ServiceAction::Remove => service::remove(),
        ServiceAction::Status => service::status(),
    }
}

/// Arguments of the service: `serve --addr ...` and the global flags of this
/// run (absolute paths: the service has its own working directory).
fn service_args(cli: &Cli, addr: std::net::SocketAddr, auth: bool, session_days: u64) -> Result<Vec<String>> {
    let absolute = |p: &PathBuf| -> Result<String> {
        Ok(std::path::absolute(p).with_context(|| format!("path {}", p.display()))?.to_string_lossy().into_owned())
    };
    // The socket lets the `notes app` window use the service's core instead of starting a second one.
    let mut args = vec!["serve".to_owned(), "--addr".to_owned(), addr.to_string(), "--socket".to_owned()];
    if auth {
        args.extend(["--auth".to_owned(), "--session-days".to_owned(), session_days.to_string()]);
    }
    if let Some(data) = &cli.data {
        args.extend(["--data".to_owned(), absolute(data)?]);
    }
    if let Some(library) = &cli.library {
        args.extend(["--library".to_owned(), absolute(library)?]);
    }
    for dir in &cli.font_paths {
        args.extend(["--font-path".to_owned(), absolute(dir)?]);
    }
    Ok(args)
}

/// Unix: this process becomes the part (signals, input and exit code are its own).
#[cfg(unix)]
fn exec(mut child: Command, path: &Path) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let e = child.exec();
    eprintln!("error: starting {}: {e}", path.display());
    ExitCode::FAILURE
}

#[cfg(not(unix))]
fn exec(mut child: Command, path: &Path) -> ExitCode {
    match child.status() {
        Ok(status) => status.code().and_then(|c| u8::try_from(c).ok()).map_or(ExitCode::FAILURE, ExitCode::from),
        Err(e) => {
            eprintln!("error: starting {}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::*;

    fn cli(line: &[&str]) -> Cli {
        Cli::try_parse_from(["notes", "--data", "/srv/notes"].into_iter().chain(line.iter().copied())).unwrap()
    }

    #[test]
    fn service_carries_the_serve_options() {
        let args = |line: &[&str]| {
            let cli = cli(line);
            let Light::Service { action: ServiceAction::Install { addr, auth, session_days } } = &cli.command else {
                panic!("not a service install");
            };
            service_args(&cli, *addr, *auth, *session_days).unwrap()
        };
        let with_auth = args(&["service", "install", "--addr", "0.0.0.0:8421", "--auth", "--session-days", "7"]);
        assert_eq!(
            with_auth,
            ["serve", "--addr", "0.0.0.0:8421", "--socket", "--auth", "--session-days", "7", "--data", "/srv/notes"]
        );
        // The default is written out too: the unit does not change if the default does.
        assert!(args(&["service", "install", "--auth"]).join(" ").contains("--auth --session-days 30"));
        // Without --auth the option has nothing to say.
        assert!(!args(&["service", "install", "--session-days", "7"]).iter().any(|a| a == "--session-days"));
    }

    /// The thin `notes` finds the command after global flags with a value: its
    /// list matches `Cli`; its commands are the light ones and the parts' ones.
    #[test]
    fn global_flags_and_commands_match_the_parts_table() {
        let command = Cli::command();
        let mut flags: Vec<String> = command
            .get_arguments()
            .filter(|a| a.is_global_set() && a.get_action().takes_values())
            .filter_map(|a| a.get_long().map(|l| format!("--{l}")))
            .collect();
        flags.sort();
        let mut expected: Vec<&str> = notes_parts::VALUE_FLAGS.to_vec();
        expected.sort_unstable();
        assert_eq!(flags, expected);

        let mut commands: Vec<&str> = command.get_subcommands().map(clap::Command::get_name).collect();
        commands.retain(|name| *name != "help");
        commands.sort_unstable();
        let mut expected: Vec<&str> =
            LIGHT.iter().copied().chain(PARTS.iter().flat_map(|p| p.commands.iter().copied())).collect();
        expected.sort_unstable();
        assert_eq!(commands, expected, "every command is in the help, once");
    }

    #[test]
    fn routes() {
        assert_eq!(route(&["docs", "writing"]), Route::Here);
        assert_eq!(route(&["--data", "d", "sync", "now"]), Route::Here);
        assert_eq!(route::<&str>(&[]), Route::Here);
        assert_eq!(route(&["--help"]), Route::Here);
        assert_eq!(route(&["help"]), Route::Here);
        assert_eq!(route(&["help", "vaults"]), Route::Here);
        assert_eq!(route(&["list"]), Route::Part(TYPST));
        assert_eq!(route(&["--vault", "app", "list"]), Route::Part(TYPST));
        assert_eq!(route(&["help", "list"]), Route::Part(TYPST));
        assert_eq!(route(&["app"]), Route::Part(notes_parts::APP));
        assert_eq!(route(&["users", "list"]), Route::Part(notes_parts::HUB));
        assert_eq!(route(&["frobnicate"]), Route::Part(TYPST));
    }
}
