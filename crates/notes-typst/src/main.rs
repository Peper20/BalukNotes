//! `notes-typst`: the `notes` part that builds with Typst. It takes every
//! command except those of other parts and is called through the thin `notes`
//! (crate `notes`, architecture §1). Works from any directory: vaults live in
//! the user's data directory; the library, client and fonts are embedded in
//! the binary (release build).
//!
//!   notes app                 the window (the `notes-app` part): the app itself
//!   notes serve               the browser version / self-hosting (http://127.0.0.1:8421)
//!   notes new <path>          a note stub (--book: a book)
//!   notes list | tags         notes of the vault / tags
//!   notes check [path]        compile errors and broken links
//!   notes pdf <path>          a note as PDF (the PDF look of baluk)
//!   notes png <path>          the PDF pages as PNG images
//!   notes docs <topic>        how to write notes, the library API
//!   notes info                where the vault, settings and library are
//!   notes vaults [new <name>] vaults / create a new one
//!   notes service install     autostart of notes serve for self-hosting (systemd user service)
//!
//! Data directory (`vaults/`, `settings.json`, `cache/`): `--data` or
//! `NOTES_DATA`, else `data` from `~/.config/baluk-notes/config.toml`, else
//! `~/.local/share/baluk-notes`. Vaults are `<data>/vaults/<name>/`; there is
//! no default vault: note commands always take `--vault <name>` (a directory
//! outside the data directory by path: `--vault tests/vault`), and the user
//! creates the first vault (`notes vaults new`, the app).

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use notes_core::check::{Report, check, check_note};
use notes_core::new_note::NewNote;
use notes_core::rename::RenameKind;
use notes_core::search::{TaggedChapter, tagged_chapters};
use notes_core::settings::{Platform, Schema, SettingsStore};
use notes_core::vault::NoteKind;
use notes_core::{LibrarySource, NoteId, Notes, NotesConfig, VaultName, Vaults};
use notes_store::config;

/// `println!` that exits quietly (code 0) when the reader closed the pipe
/// (`notes list | head`): Rust ignores SIGPIPE, and `println!` would panic.
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

mod service;
mod sync;

/// Writes command output to stdout: a closed pipe ends the process with code
/// 0, another write error with code 1 (the output is lost anyway).
fn write_out(args: std::fmt::Arguments<'_>, newline: bool) {
    use std::io::Write as _;
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

/// Default lifetime of a sign-in session without requests, in days.
const DEFAULT_SESSION_DAYS: u64 = 30;
/// Default address of `notes serve`.
const ADDR: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8421));

#[derive(Debug, Parser)]
#[command(name = "notes", bin_name = "notes", version, about = "Notes in Typst")]
struct Cli {
    /// Data directory: vaults/, settings.json, cache. Default: `data` from
    /// ~/.config/baluk-notes/config.toml, else ~/.local/share/baluk-notes.
    #[arg(long, global = true, env = "NOTES_DATA")]
    data: Option<PathBuf>,

    /// Vault: a name (a directory in <data>/vaults/) or a path to a directory,
    /// with "/" (tests/vault, ./notes). Note commands need it (new, list, tags,
    /// check, pdf); the list: `notes vaults`.
    #[arg(long, global = true)]
    vault: Option<String>,

    /// Where notes deleted in the app go: the system trash by default; a
    /// directory: into it (tests).
    #[arg(long, global = true, env = "NOTES_TRASH", hide = true)]
    trash: Option<PathBuf>,

    /// The style library (baluk/), seen by notes as /_baluk/. Default: the
    /// one embedded in the binary; in a debug build, the repository's baluk/
    /// directory (edits show at once).
    #[arg(long, global = true, env = "NOTES_LIBRARY")]
    library: Option<PathBuf>,

    /// An extra font directory (repeatable).
    #[arg(long = "font-path", global = true, env = "NOTES_FONT_PATHS", value_delimiter = ':')]
    font_paths: Vec<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// The browser version / self-hosting: a local server with the client.
    /// (The app itself is the `notes app` window.)
    Serve {
        /// Address for the browser; 0.0.0.0:8421 for access from the network.
        /// Default: 127.0.0.1:8421, unless only --socket is given.
        #[arg(long)]
        addr: Option<SocketAddr>,
        /// Also a Unix socket, for the `notes app` window (user-only
        /// permissions, no sign-in). Without a path:
        /// $XDG_RUNTIME_DIR/baluk-notes/notes.sock.
        #[arg(long, num_args = 0..=1, value_name = "PATH")]
        #[expect(clippy::option_option, reason = "clap: no flag, a flag without a path, a flag with a path")]
        socket: Option<Option<PathBuf>>,
        /// Sign-in by login and password on the TCP address (accounts:
        /// `notes users add <login>`). Required for an address other than
        /// localhost. Does not apply to the Unix socket. The server speaks
        /// HTTP only: for the network put a reverse proxy with HTTPS in front.
        #[arg(long)]
        auth: bool,
        /// With --auth: a session ends after this many days without requests.
        #[arg(long, default_value_t = DEFAULT_SESSION_DAYS, value_parser = clap::value_parser!(u64).range(1..=3650))]
        session_days: u64,
    },
    /// A stub of a new note or book; prints the path of its file and, on the
    /// second line, the note path (for check, pdf, #see).
    ///
    /// The file name comes from the title (without / \ : * ? " < > |; a taken
    /// name gets a number), in the --folder directory; or the whole path as
    /// the argument. Existing files are never overwritten. How to write on:
    /// `notes docs writing`.
    New {
        /// Path from the vault root, without .typ: "Network/SSH",
        /// "Courses/Calculus"; without it, from the title.
        #[arg(required_unless_present = "title", conflicts_with = "folder")]
        id: Option<String>,
        /// Directory for a note named from its title: "Network", "Courses/Calculus"; default: the root.
        #[arg(long)]
        folder: Option<String>,
        /// A book: a directory with main.typ, chapters are files next to it (otherwise a note, one file).
        #[arg(long)]
        book: bool,
        /// Title, any text; default (with a path): the last path segment.
        #[arg(long)]
        title: Option<String>,
        /// A tag (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Note language (ISO 639: en, de, ...); default: Russian.
        #[arg(long)]
        lang: Option<String>,
    },
    /// Notes and books of the vault: path, kind, title, tags; under a book, its chapters with their own tags.
    List {
        /// As JSON.
        #[arg(long)]
        json: bool,
    },
    /// Tags of the vault and the number of notes with each.
    Tags,
    /// Check the vault or one note: compile errors, warnings, broken links
    /// (exit code 1: errors or broken links).
    Check {
        /// Only this note or book (links are checked against the whole vault).
        id: Option<String>,
        /// Report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// A note or book as PDF.
    Pdf {
        /// Note path from the vault root: "Network/SSH", "Courses/Calculus".
        id: String,
        /// Output file; default: <note name>.pdf here.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Theme; default: the first (light).
        #[arg(long)]
        theme: Option<String>,
    },
    /// Pages of a note or book as PNG images, exactly as in the PDF (for
    /// looking at the result without a PDF viewer). Prints one file per line.
    Png {
        /// Note path from the vault root: "Network/SSH", "Courses/Calculus".
        id: String,
        /// Output directory (created if missing); files are
        /// <note name>-<page>.png. Default: here.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Theme; default: the first (light).
        #[arg(long)]
        theme: Option<String>,
        /// Pages from 1: "3", "2-5", "1,4,7-9"; default: all.
        #[arg(long)]
        pages: Option<String>,
        /// Resolution, dots per inch.
        #[arg(long, default_value_t = 110)]
        dpi: u16,
    },
    /// Rename a note, book or folder as the app does: the new title goes into
    /// the file (a folder: `_folder.toml`), the file (folder) name follows it,
    /// and `#see` links to it in other notes are rewritten. The parent folder
    /// stays. Prints the new path, then where links were fixed.
    Rename {
        /// Path of the note, book or folder from the vault root: "Network/SSH".
        id: String,
        /// The new title, any text.
        title: String,
        /// Only show what would change.
        #[arg(long)]
        dry_run: bool,
    },
    /// Documentation: how to write notes, the style library API.
    Docs {
        /// What to show.
        topic: Topic,
    },
    /// Where the vault, settings and cache are, which library, how to open the app.
    Info,
    /// Vaults of the data directory; `new <name>` creates a new one.
    Vaults {
        #[command(subcommand)]
        action: Option<VaultsAction>,
    },
    /// Autostart of `notes serve` for self-hosting: a systemd user service
    /// (Linux), no root. The `notes app` window does not need it.
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Sync vaults with a storage server (`notes-hub`): sign in, link a vault,
    /// sync now. `notes serve` (and the app) then keep linked vaults in sync
    /// by themselves.
    Sync {
        #[command(subcommand)]
        action: sync::SyncAction,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceAction {
    /// Install and start: the browser server runs in the background, starts
    /// at login and restarts after a crash. Already installed: restart it
    /// (after updating notes). Runs this same `notes` binary; --data,
    /// --library and --font-path are passed to the service.
    Install {
        /// Address; 0.0.0.0:8421 for access from the network (needs --auth).
        #[arg(long, default_value_t = ADDR)]
        addr: SocketAddr,
        /// Sign-in by login and password, as `notes serve --auth`.
        #[arg(long)]
        auth: bool,
        /// With --auth: a session ends after this many days without requests
        /// (as `notes serve --session-days`).
        #[arg(long, default_value_t = DEFAULT_SESSION_DAYS, value_parser = clap::value_parser!(u64).range(1..=3650))]
        session_days: u64,
    },
    /// Stop and remove the service.
    Remove,
    /// Whether the service is installed and running, what it runs, where its log is.
    Status,
}

#[derive(Debug, Subcommand)]
enum VaultsAction {
    /// A new empty vault: <data>/vaults/<name>/.
    New {
        /// Name, also the directory name: "Study", "Work 2026".
        name: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Topic {
    /// How to write notes: process, text, figures, checks.
    Writing,
    /// The baluk style library: templates, blocks, figures, interactivity.
    Library,
}

impl Topic {
    fn text(self) -> &'static str {
        match self {
            Self::Writing => include_str!("../../../docs/writing.md"),
            Self::Library => include_str!("../../../baluk/README.md"),
        }
    }
}

/// Data directory: explicit, from the config file, or the standard one
/// (`notes_store::config`, the rule shared by all parts).
fn data_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    // The error text already has its cause: not a chain, `{e:#}` would repeat it.
    config::data_dir(explicit).map_err(|e| anyhow::anyhow!("{e}"))
}

/// The style library: the given one, the repository's in a debug build, else
/// the embedded one.
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

/// jemalloc: memory goes back to the system after builds (docs/research/E5.md).
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

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
    if let Err(e) = notes::check_version("notes-typst") {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Which vault: `--vault`.
#[derive(Debug, Clone)]
enum VaultArg {
    /// A vault of the data directory, by name.
    Name(String),
    /// A directory anywhere (the argument has a `/`).
    Path(PathBuf),
}

impl VaultArg {
    fn parse(raw: &str) -> Self {
        if raw.contains(['/', '\\']) || raw == "." || raw == ".." {
            Self::Path(PathBuf::from(raw))
        } else {
            Self::Name(raw.to_owned())
        }
    }
}

/// The vault for a command: name and directory. There is no default vault:
/// none named is an error with the list. A directory given by path is created
/// if missing (tests); by name, only an existing vault.
fn pick_vault(vaults: &Vaults, arg: Option<&VaultArg>) -> Result<(VaultName, PathBuf)> {
    match arg {
        Some(VaultArg::Path(dir)) => {
            if !dir.exists() {
                std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
                tracing::info!("created an empty vault {}", dir.display());
            }
            let dir = dir.canonicalize().with_context(|| format!("vault {}", dir.display()))?;
            let name = dir.file_name().and_then(|n| n.to_str()).map(VaultName::new);
            let Some(Ok(name)) = name else { bail!("the directory name {} does not fit a vault", dir.display()) };
            Ok((name, dir))
        }
        Some(VaultArg::Name(name)) => {
            let name = vaults.find(name)?;
            Ok((name.clone(), vaults.path(&name)))
        }
        None => Err(notes_core::Error::VaultRequired(vaults.list()?).into()),
    }
}

#[expect(clippy::too_many_lines, reason = "the dispatch of every command")]
fn run(cli: Cli) -> Result<ExitCode> {
    // Without a vault: documentation, the service.
    match &cli.command {
        Command::Docs { topic } => {
            out_raw!("{}", topic.text());
            return Ok(ExitCode::SUCCESS);
        }
        Command::Service { action } => {
            match action {
                ServiceAction::Install { addr, auth, session_days } => {
                    // The service must start: check what `serve` checks, here and now.
                    check_sign_in(Some(*addr), *auth)?;
                    if *auth {
                        open_auth(&data_dir(cli.data.as_deref())?, *session_days)?;
                    }
                    let exe = std::env::current_exe().context("path of the notes binary")?;
                    if cfg!(debug_assertions) {
                        eprintln!(
                            "warning: a debug build; the service will run it (usually: tools/install.sh and notes from PATH)"
                        );
                    }
                    service::install(&exe, &service_args(&cli, *addr, *auth, *session_days)?, *addr)?;
                }
                ServiceAction::Remove => service::remove()?,
                ServiceAction::Status => service::status()?,
            }
            return Ok(ExitCode::SUCCESS);
        }
        _ => {}
    }
    let data = data_dir(cli.data.as_deref())?;
    let vaults = Vaults::new(&data);
    let arg = cli.vault.as_deref().map(VaultArg::parse);
    let config = NotesConfig {
        vault: PathBuf::new(),
        library: library(cli.library.as_ref()),
        font_dirs: cli.font_paths.clone(),
        cache: Some(notes_core::cache::default_dir(&data)),
        trash: cli.trash.clone(),
    };

    // Without an open vault: the vault list, info, the server.
    match cli.command {
        Command::Vaults { action } => return vaults_command(&vaults, action),
        Command::Sync { action } => return sync::run(&data, cli.vault.as_deref(), action),
        Command::Info => return info(&vaults, &data, arg.as_ref(), cli.library.as_ref()),
        Command::Serve { addr, socket, auth, session_days } => {
            let socket = socket
                .map(|path| {
                    path.or_else(notes::default_socket).context("no $XDG_RUNTIME_DIR: give a path: --socket <path>")
                })
                .transpose()?;
            return serve(vaults, config, &data, arg, Listen { addr, socket }, SignIn { enabled: auth, session_days });
        }
        _ => {}
    }

    let (name, vault) = pick_vault(&vaults, arg.as_ref())?;
    let started = std::time::Instant::now();
    let notes = Notes::open(&NotesConfig { vault: vault.clone(), ..config }).context("open the vault")?;
    tracing::debug!(ms = started.elapsed().as_millis(), "vault \"{name}\": {}", notes.vault().location());

    match cli.command {
        Command::New { id, folder, book, title, tags, lang } => {
            let kind = if book { NoteKind::Book } else { NoteKind::Note };
            let note = NewNote { kind, title, tags, lang };
            let id = if let Some(id) = id {
                new_note_id(&id, &vault)?
            } else {
                // The file name comes from the title, in the --folder directory (none: the root).
                let folder = match folder.as_deref().map(|f| f.trim_matches('/')) {
                    Some(f) if !f.is_empty() => new_note_id(f, &vault)?.to_string(),
                    _ => String::new(),
                };
                note.id_in(notes.vault(), &folder)?
            };
            let main = note.create(notes.vault(), &id)?;
            out!("{}", notes.vault().storage().display(&main).display());
            out!("{id}");
            eprintln!("in the app: notes app --vault \"{name}\" \"{id}\"");
            Ok(ExitCode::SUCCESS)
        }
        Command::List { json } => list(&notes, json),
        Command::Tags => tags(&notes),
        Command::Check { id, json } => {
            // Parallel builds and Typst memory as in the app.
            notes.apply_device(&open_settings(&notes, &data)?.device());
            let report = match id {
                Some(id) => check_note(&notes, &note_id(&id, &vault)?)?,
                None => check(&notes)?,
            };
            print_check(&report, json)
        }
        Command::Pdf { id, out, theme } => {
            // Packages beyond the whitelist as in the app.
            notes.apply_device(&open_settings(&notes, &data)?.device());
            pdf(&notes, &note_id(&id, &vault)?, out, theme)
        }
        Command::Png { id, out, theme, pages, dpi } => {
            notes.apply_device(&open_settings(&notes, &data)?.device());
            png(&notes, &note_id(&id, &vault)?, out, theme, pages.as_deref(), dpi)
        }
        Command::Rename { id, title, dry_run } => rename(&notes, &note_id(&id, &vault)?, &title, dry_run),
        Command::Docs { .. }
        | Command::Service { .. }
        | Command::Info
        | Command::Vaults { .. }
        | Command::Sync { .. }
        | Command::Serve { .. } => {
            unreachable!("handled above")
        }
    }
}

/// Arguments of the service: `serve --addr ...` and the global flags of this
/// run (absolute paths: the service has its own working directory).
fn service_args(cli: &Cli, addr: SocketAddr, auth: bool, session_days: u64) -> Result<Vec<String>> {
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

fn rename(notes: &Notes, id: &NoteId, title: &str, dry_run: bool) -> Result<ExitCode> {
    // Neither a note nor a book: a folder (missing: the core's "not found" error).
    let kind = if notes.vault().entry(id).is_ok() { RenameKind::Note } else { RenameKind::Folder };
    let plan = notes.rename(kind, id, title, !dry_run)?;
    out!("{}", plan.to);
    let links: Vec<String> = plan.links.iter().map(|l| format!("{} ({})", l.note, l.count)).collect();
    match (links.is_empty(), dry_run) {
        (true, _) => eprintln!("no links to it in other notes"),
        (false, false) => eprintln!("links fixed: {}", links.join(", ")),
        (false, true) => eprintln!("links to fix: {}", links.join(", ")),
    }
    if dry_run {
        eprintln!("nothing changed (--dry-run)");
    }
    Ok(ExitCode::SUCCESS)
}

fn vaults_command(vaults: &Vaults, action: Option<VaultsAction>) -> Result<ExitCode> {
    if let Some(VaultsAction::New { name }) = action {
        let name = VaultName::new(name)?;
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

fn info(vaults: &Vaults, data: &Path, arg: Option<&VaultArg>, library_dir: Option<&PathBuf>) -> Result<ExitCode> {
    let config = config::config_path().map(|p| p.display().to_string()).unwrap_or_default();
    let library = match library(library_dir) {
        LibrarySource::Dir(dir) => dir.display().to_string(),
        LibrarySource::Embedded => "embedded in the binary".into(),
    };
    // No vault named is not an error here: show the list.
    match arg {
        Some(arg) => {
            let (name, path) = pick_vault(vaults, Some(arg))?;
            out!("vault:     {} (\"{name}\")", path.display());
        }
        None => out!("vault:     not chosen (--vault \"Name\")"),
    }
    let names: Vec<String> = vaults.list()?.iter().map(ToString::to_string).collect();
    let names = if names.is_empty() { "none".to_owned() } else { names.join(", ") };
    out!("vaults:    {names} ({})", vaults.root().display());
    out!("data:      {} (settings.json, cache/)", data.display());
    out!("config:    {config} (data = \"...\" sets another data directory)");
    out!("library:   {library}");
    out!("app:       notes app (the window); in a browser: notes serve -> http://{ADDR}/");
    Ok(ExitCode::SUCCESS)
}

/// A note path from the command line. A common mistake (agents especially) is
/// a disk path instead of a path from the vault root: suggest the right one.
fn note_id(raw: &str, vault: &Path) -> Result<NoteId> {
    let path = Path::new(raw);
    if path.is_absolute() {
        let inside = std::iter::once(vault.to_path_buf())
            .chain(vault.canonicalize().ok())
            .find_map(|v| path.strip_prefix(v).ok().map(|rest| rest.to_string_lossy().into_owned()))
            .filter(|rest| !rest.is_empty());
        match inside {
            Some(rest) => {
                let rest = rest.strip_suffix(".typ").unwrap_or(&rest);
                bail!("\"{raw}\" is a disk path; give the path from the vault root: \"{rest}\"")
            }
            None => bail!(
                "\"{raw}\" is a disk path; give the path from the vault root ({}), e.g. \"Folder/Title\"",
                vault.display()
            ),
        }
    }
    Ok(NoteId::new(raw)?)
}

/// A new note's path: on top of `note_id`, it must not start with the vault
/// directory's name (`vault/Topic` would create `vault/vault/Topic.typ`)
/// unless the vault has such a folder.
fn new_note_id(raw: &str, vault: &Path) -> Result<NoteId> {
    let id = note_id(raw, vault)?;
    if let Some(name) = vault.file_name().and_then(|n| n.to_str())
        && let Some(rest) = id.as_str().strip_prefix(&format!("{name}/"))
        && !vault.join(name).is_dir()
    {
        bail!("\"{raw}\" starts with the vault directory name; the path is from its root: \"{rest}\"");
    }
    Ok(id)
}

fn open_settings(notes: &Notes, data: &Path) -> Result<SettingsStore> {
    let schema = Schema::new(notes.themes().themes(), Platform::current());
    SettingsStore::open(data.join("settings.json"), schema).context("settings")
}

/// Where `notes serve` listens: `--addr` and `--socket`; neither means the
/// default address.
#[derive(Debug)]
struct Listen {
    addr: Option<SocketAddr>,
    socket: Option<PathBuf>,
}

/// `notes serve --auth` and `--session-days`.
#[derive(Debug, Clone, Copy)]
struct SignIn {
    enabled: bool,
    session_days: u64,
}

/// The TCP address a server with these flags listens on, if any.
fn tcp_addr(listen: &Listen) -> Option<SocketAddr> {
    listen.addr.or_else(|| listen.socket.is_none().then_some(ADDR))
}

/// The rule of the TCP address: other than localhost, it needs sign-in (secure
/// by default); sign-in is for the TCP address only.
fn check_sign_in(addr: Option<SocketAddr>, auth: bool) -> Result<()> {
    match addr {
        Some(addr) if !auth && !addr.ip().is_loopback() => bail!(
            "{addr} is not a localhost address: add --auth (sign-in by login and password; accounts: notes users add <login>). The server speaks HTTP only: HTTPS is the job of a reverse proxy in front of it"
        ),
        None if auth => bail!("--auth protects the TCP address: add --addr"),
        _ => Ok(()),
    }
}

/// The sign-in of the TCP address: needs at least one account.
fn open_auth(data: &Path, session_days: u64) -> Result<Arc<notes_hub::Auth>> {
    let auth = notes_hub::Auth::open(data, std::time::Duration::from_secs(session_days * 24 * 3600))?;
    if auth.accounts().is_empty()? {
        bail!("no users: create one: notes users add <login>");
    }
    Ok(auth)
}

/// The server: all vaults of the data directory (opened on request; there may
/// be none, the first is created in the app) or one (`--vault <name or path>`).
fn serve(
    vaults: Vaults,
    config: NotesConfig,
    data: &Path,
    arg: Option<VaultArg>,
    listen: Listen,
    sign_in: SignIn,
) -> Result<ExitCode> {
    let addr = tcp_addr(&listen);
    check_sign_in(addr, sign_in.enabled)?;
    let auth = sign_in.enabled.then(|| open_auth(data, sign_in.session_days)).transpose()?;
    if let Some(addr) = addr.filter(|a| sign_in.enabled && !a.ip().is_loopback()) {
        tracing::warn!("{addr} is not localhost: passwords travel in the clear unless a reverse proxy does HTTPS");
    }
    let set = if let Some(arg) = arg {
        let (name, vault) = pick_vault(&vaults, Some(&arg))?;
        let notes = Notes::open(&NotesConfig { vault, ..config }).context("open the vault")?;
        let settings = Arc::new(open_settings(&notes, data)?);
        tracing::info!("vault \"{name}\": {}", notes.vault().location());
        notes_server::VaultSet::single(name, Arc::new(notes), settings)
    } else {
        // Themes and fonts are the same for all vaults (the library): a core
        // without a vault, so that the server works with no vaults at all.
        let library =
            Notes::with_storage(Arc::new(notes_core::storage::MemStorage::new()), &config).context("style library")?;
        let settings = Arc::new(open_settings(&library, data)?);
        tracing::info!("vaults: {}", vaults.root().display());
        notes_server::VaultSet::registry(vaults, config, Arc::new(library), settings)
    };
    let device_sync = sync::service(data, set.settings().clone());
    let state = notes_server::AppState::new(set).with_auth(auth).with_device_sync(Some(device_sync));
    if state.auth.is_some() {
        tracing::info!("sign-in on the TCP address (accounts: notes users)");
    }
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let mut listeners = Vec::new();
        if let Some(addr) = addr {
            let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("bind {addr}"))?;
            tracing::info!("open: http://{addr}/");
            listeners.push(notes_server::Listen::Tcp(listener));
        }
        if let Some(path) = &listen.socket {
            listeners.push(bind_socket(path)?);
            tracing::info!("socket: {}", path.display());
        }
        let served = notes_server::serve(listeners, state, stop_signal()).await;
        // A clean stop removes the socket (after a crash the next start removes it).
        if let Some(path) = &listen.socket {
            let _ = std::fs::remove_file(path);
        }
        served?;
        Ok(ExitCode::SUCCESS)
    })
}

/// The Unix socket for the window: the directory and the socket are user-only.
/// A socket left by a crashed server is removed; a live one (someone answers)
/// is an error.
#[cfg(unix)]
fn bind_socket(path: &Path) -> Result<notes_server::Listen> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .with_context(|| format!("create {}", dir.display()))?;
    }
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            bail!("socket {} is in use: notes serve is already running", path.display());
        }
        std::fs::remove_file(path).with_context(|| format!("remove the old socket {}", path.display()))?;
    }
    let listener = tokio::net::UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(notes_server::Listen::Unix(listener))
}

#[cfg(not(unix))]
fn bind_socket(_path: &Path) -> Result<notes_server::Listen> {
    bail!("--socket works only on Linux and macOS")
}

/// Ctrl+C or SIGTERM (how the systemd service stops it): the server stops
/// cleanly and waiting event requests get an answer.
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
/// A list line or JSON object: a note from the source index.
#[derive(Debug, serde::Serialize)]
struct ListItem<'a> {
    id: &'a NoteId,
    kind: NoteKind,
    title: Option<&'a str>,
    tags: &'a [String],
    /// The book's chapters with their own tags (they inherit the book's tags).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    chapters: Vec<TaggedChapter>,
}

fn list(notes: &Notes, json: bool) -> Result<ExitCode> {
    let index = notes.index()?;
    let items: Vec<ListItem> = index
        .outlines()
        .map(|(entry, outline)| ListItem {
            id: &entry.id,
            kind: entry.kind,
            title: outline.title.as_deref(),
            tags: &outline.tags,
            chapters: tagged_chapters(outline),
        })
        .collect();
    if json {
        out!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(ExitCode::SUCCESS);
    }
    for item in &items {
        let kind = if item.kind == NoteKind::Book { "  [book]" } else { "" };
        let title = item.title.filter(|t| *t != item.id.name()).map(|t| format!("  \"{t}\"")).unwrap_or_default();
        let tags: String = item.tags.iter().flat_map(|t| ["  #", t.as_str()]).collect();
        out!("{}{kind}{title}{tags}", item.id);
        for chapter in &item.chapters {
            let tags: String = chapter.tags.iter().flat_map(|t| ["  #", t.as_str()]).collect();
            out!("  chapter \"{}\"{tags}", chapter.title);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn tags(notes: &Notes) -> Result<ExitCode> {
    let index = notes.index()?;
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for (_, outline) in index.outlines() {
        // A book counts once, however many chapters carry the tag.
        for tag in outline.all_tags().collect::<std::collections::BTreeSet<_>>() {
            *counts.entry(tag).or_default() += 1;
        }
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (tag, count) in sorted {
        out!("{tag}\t{count}");
    }
    Ok(ExitCode::SUCCESS)
}

fn pdf(notes: &Notes, id: &NoteId, out: Option<PathBuf>, theme: Option<String>) -> Result<ExitCode> {
    let theme = theme.unwrap_or_else(|| notes.themes().names().first().cloned().unwrap_or_default());
    let out = out.unwrap_or_else(|| PathBuf::from(format!("{}.pdf", id.name())));
    match notes.pdf(id, &theme)? {
        Ok(bytes) => {
            std::fs::write(&out, bytes).with_context(|| format!("write {}", out.display()))?;
            out!("{} -> {}", id, out.display());
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

fn png(
    notes: &Notes,
    id: &NoteId,
    out: Option<PathBuf>,
    theme: Option<String>,
    pages: Option<&str>,
    dpi: u16,
) -> Result<ExitCode> {
    let theme = theme.unwrap_or_else(|| notes.themes().names().first().cloned().unwrap_or_default());
    let pages = pages.map(page_numbers).transpose()?.unwrap_or_default();
    if !(10..=600).contains(&dpi) {
        bail!("--dpi {dpi}: expected 10 to 600");
    }
    let rendered = match notes.png(id, &theme, &pages, f64::from(dpi))? {
        Ok(rendered) => rendered,
        Err(errors) => {
            for e in errors {
                eprintln!("{id}: {e}");
            }
            return Ok(ExitCode::FAILURE);
        }
    };
    let dir = out.unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let width = rendered.count.to_string().len();
    for (n, bytes) in rendered.pages {
        let path = dir.join(format!("{}-{n:0width$}.png", id.name()));
        std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
        out!("{}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// Page numbers from "3", "2-5", "1,4,7-9" (from 1, in the given order).
fn page_numbers(spec: &str) -> Result<Vec<usize>> {
    let number = |s: &str| -> Result<usize> {
        match s.trim().parse::<usize>() {
            Ok(n) if n > 0 => Ok(n),
            _ => bail!("--pages {spec}: \"{s}\" is not a page number (from 1)"),
        }
    };
    let mut pages = Vec::new();
    for part in spec.split(',') {
        match part.split_once('-') {
            Some((from, to)) => {
                let (from, to) = (number(from)?, number(to)?);
                if from > to {
                    bail!("--pages {spec}: {from}-{to} goes backwards");
                }
                pages.extend(from..=to);
            }
            None => pages.push(number(part)?),
        }
    }
    Ok(pages)
}

fn print_check(report: &Report, json: bool) -> Result<ExitCode> {
    if json {
        out!("{}", serde_json::to_string_pretty(report)?);
    } else {
        for n in &report.notes {
            for e in &n.errors {
                out!("{}: {e}", n.id);
            }
            for w in &n.warnings {
                out!("{}: {w}", n.id);
            }
            for l in &n.broken_links {
                let anchor = l.anchor.as_deref().map(|a| format!(" / {a}")).unwrap_or_default();
                out!("{}: broken link \"{}{anchor}\": {}", n.id, l.target, l.reason);
            }
        }
        for f in &report.folders {
            out!("{}: {}", f.file, f.error);
        }
        out!("{}", report.summary());
    }
    Ok(if report.is_clean() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_numbers_parse() {
        assert_eq!(page_numbers("3").unwrap(), [3]);
        assert_eq!(page_numbers("1,4,7-9").unwrap(), [1, 4, 7, 8, 9]);
        assert_eq!(page_numbers(" 2 - 3 ").unwrap(), [2, 3]);
        for bad in ["0", "", "a", "5-2", "1,,2"] {
            assert!(page_numbers(bad).is_err(), "{bad}");
        }
    }

    fn err(result: Result<NoteId>) -> String {
        result.unwrap_err().to_string()
    }

    /// The thin `notes` finds the command after global flags with a value: its
    /// list matches `Cli`, and commands of other parts are not taken here.
    #[test]
    fn thin_notes_knows_global_flags_and_commands() {
        use clap::CommandFactory;
        let cli = Cli::command();
        let mut flags: Vec<String> = cli
            .get_arguments()
            .filter(|a| a.is_global_set() && a.get_action().takes_values())
            .filter_map(|a| a.get_long().map(|l| format!("--{l}")))
            .collect();
        flags.sort();
        let mut expected: Vec<&str> = notes::VALUE_FLAGS.to_vec();
        expected.sort_unstable();
        assert_eq!(flags, expected);
        for part in notes::PARTS {
            for command in part.commands {
                assert!(cli.find_subcommand(command).is_none(), "command {command} belongs to the part {}", part.bin);
            }
        }
    }

    #[test]
    fn service_carries_the_serve_options() {
        let args = |line: &[&str]| {
            let cli =
                Cli::try_parse_from(["notes", "--data", "/srv/notes"].into_iter().chain(line.iter().copied())).unwrap();
            let Command::Service { action: ServiceAction::Install { addr, auth, session_days } } = &cli.command else {
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

    #[test]
    fn disk_path_gets_a_hint() {
        let vault = Path::new("/нет/data/vault");
        assert_eq!(note_id("Сеть/SSH", vault).unwrap().as_str(), "Сеть/SSH");
        assert!(
            err(note_id("/нет/data/vault/Сеть/SSH.typ", vault))
                .ends_with("give the path from the vault root: \"Сеть/SSH\"")
        );
        assert!(err(note_id("/elsewhere/SSH", vault)).contains("(/нет/data/vault)"));
        assert!(err(note_id("/нет/data/vault", vault)).contains("e.g."));
    }

    #[test]
    fn new_note_is_not_under_vault_name() {
        let vault = Path::new("/нет/data/vault");
        assert!(err(new_note_id("vault/Тема", vault)).ends_with("the path is from its root: \"Тема\""));
        assert_eq!(new_note_id("vaults/Тема", vault).unwrap().as_str(), "vaults/Тема");
        assert_eq!(new_note_id("Тема", vault).unwrap().as_str(), "Тема");
    }

    #[test]
    fn tcp_address_rules() {
        let addr = |s: &str| Some(s.parse::<SocketAddr>().unwrap());
        // Localhost works as it did, with or without sign-in.
        assert!(check_sign_in(addr("127.0.0.1:8421"), false).is_ok());
        assert!(check_sign_in(addr("[::1]:8421"), false).is_ok());
        assert!(check_sign_in(addr("127.0.0.1:8421"), true).is_ok());
        // Any other address without --auth is refused, and the error says what to add.
        for other in ["0.0.0.0:8421", "192.168.1.5:8421", "[::]:8421"] {
            let e = check_sign_in(addr(other), false).unwrap_err().to_string();
            assert!(e.contains(other) && e.contains("add --auth") && e.contains("HTTPS"), "{e}");
            assert!(check_sign_in(addr(other), true).is_ok());
        }
        // Only the socket: no TCP address to protect.
        assert!(check_sign_in(None, false).is_ok());
        assert!(check_sign_in(None, true).unwrap_err().to_string().contains("--addr"));
    }

    #[test]
    fn sign_in_needs_an_account() {
        let data = tempfile::tempdir().unwrap();
        let e = open_auth(data.path(), 30).map(|_| ()).unwrap_err();
        assert_eq!(e.to_string(), "no users: create one: notes users add <login>");
        notes_hub::Auth::open(data.path(), std::time::Duration::from_secs(1))
            .unwrap()
            .accounts()
            .add("ann", "correct-horse-9")
            .unwrap();
        assert!(open_auth(data.path(), 30).is_ok());
    }

    /// There is no default vault: even the only one must be named.
    #[test]
    fn vault_must_be_named() {
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        let err = |arg: Option<&VaultArg>| pick_vault(&vaults, arg).unwrap_err().to_string();
        assert!(err(None).starts_with("there are no vaults; create one"), "{}", err(None));
        vaults.create(&VaultName::new("Учёба").unwrap()).unwrap();
        assert_eq!(err(None), "name a vault: --vault \"Name\"; there are: \"Учёба\"");
        let (name, path) = pick_vault(&vaults, Some(&VaultArg::parse("Учёба"))).unwrap();
        assert_eq!((name.as_str(), path), ("Учёба", vaults.root().join("Учёба")));
        assert!(err(Some(&VaultArg::parse("Нет"))).starts_with("no vault \"Нет\""));
        assert!(!data.path().join("vaults/Нет").exists(), "not created by itself");
    }
}
