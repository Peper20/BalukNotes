//! The command line of `notes`: the light commands, which this binary runs
//! itself, in full, and every command of the parts as a line of help only.
//! A part's command is never parsed here: its arguments go to the part, which
//! parses them with its own `clap` (architecture §1).

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use notes_store::serve_rules::{DEFAULT_ADDR, DEFAULT_SESSION_DAYS};

use crate::sync::SyncAction;

/// Global flags are those of `notes-typst` (`notes_parts::VALUE_FLAGS`): the
/// thin `notes` parses the ones the light commands need and ignores the rest.
#[derive(Debug, Parser)]
#[command(name = "notes", bin_name = "notes", version, about = "Notes in Typst", arg_required_else_help = true)]
pub struct Cli {
    /// Data directory: vaults/, settings.json, cache. Default: `data` from
    /// ~/.config/baluk-notes/config.toml, else ~/.local/share/baluk-notes.
    #[arg(long, global = true, env = "NOTES_DATA")]
    pub data: Option<PathBuf>,

    /// Vault: a name (a directory in <data>/vaults/) or a path to a directory,
    /// with "/" (tests/vault, ./notes). Note commands need it (new, list, tags,
    /// check, pdf); the list: `notes vaults`.
    #[arg(long, global = true)]
    pub vault: Option<String>,

    /// Where notes deleted in the app go: the system trash by default; a
    /// directory: into it (tests).
    #[arg(long, global = true, env = "NOTES_TRASH", hide = true)]
    pub trash: Option<PathBuf>,

    /// The style library (baluk/), seen by notes as /_baluk/. Default: the
    /// one embedded in the binary; in a debug build, the repository's baluk/
    /// directory (edits show at once).
    #[arg(long, global = true, env = "NOTES_LIBRARY")]
    pub library: Option<PathBuf>,

    /// An extra font directory (repeatable).
    #[arg(long = "font-path", global = true, env = "NOTES_FONT_PATHS", value_delimiter = ':')]
    pub font_paths: Vec<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

/// `notes_parts::LIGHT` are the commands with fields, the rest are parts'
/// commands (`notes_parts::PARTS`); a test checks the two lists.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Documentation: how to write notes, the style library API.
    Docs {
        /// What to show.
        topic: Topic,
    },
    /// Vaults of the data directory; `new <name>` creates a new one.
    Vaults {
        #[command(subcommand)]
        action: Option<VaultsAction>,
    },
    /// Sync vaults with a storage server (`notes-hub`): sign in, link a vault,
    /// sync now. `notes serve` (and the app) then keep linked vaults in sync
    /// by themselves.
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    /// Autostart of `notes serve` for self-hosting: a systemd user service
    /// (Linux), no root. The `notes app` window does not need it.
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// The app window (the `notes-app` part).
    App,
    /// The storage server online (the `notes-hub` part): `hub serve`.
    Hub,
    /// Accounts of the storage server (the `notes-hub` part): add, list, passwd, remove.
    Users,
    /// The browser version / self-hosting: a local server with the client.
    Serve,
    /// A stub of a new note or book; prints the path of its file and, on the
    /// second line, the note path (for check, pdf, #see).
    New,
    /// Notes and books of the vault: path, kind, title, tags; under a book, its chapters with their own tags.
    List,
    /// Tags of the vault and the number of notes with each.
    Tags,
    /// Check the vault or one note: compile errors, warnings, broken links
    /// (exit code 1: errors or broken links).
    Check,
    /// A note or book as PDF.
    Pdf,
    /// Pages of a note or book as PNG images, exactly as in the PDF (for
    /// looking at the result without a PDF viewer). Prints one file per line.
    Png,
    /// Rename a note, book or folder as the app does: the new title goes into
    /// the file (a folder: `_folder.toml`), the file (folder) name follows it,
    /// and `#see` links to it in other notes are rewritten.
    Rename,
    /// Where the vault, settings and cache are, which library, how to open the app.
    Info,
}

#[derive(Debug, Subcommand)]
pub enum ServiceAction {
    /// Install and start: the browser server runs in the background, starts
    /// at login and restarts after a crash. Already installed: restart it
    /// (after updating notes). Runs `notes-typst` (the part, next to this
    /// `notes`); --data, --library and --font-path are passed to the service.
    Install {
        /// Address; 0.0.0.0:8421 for access from the network (needs --auth).
        #[arg(long, default_value_t = DEFAULT_ADDR)]
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
pub enum VaultsAction {
    /// A new empty vault: <data>/vaults/<name>/.
    New {
        /// Name, also the directory name: "Study", "Work 2026".
        name: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Topic {
    /// How to write notes: process, text, figures, checks.
    Writing,
    /// The baluk style library: templates, blocks, figures, interactivity.
    Library,
}

impl Topic {
    pub fn text(self) -> &'static str {
        match self {
            Self::Writing => include_str!("../../../docs/writing.md"),
            Self::Library => include_str!("../../../baluk/README.md"),
        }
    }
}
