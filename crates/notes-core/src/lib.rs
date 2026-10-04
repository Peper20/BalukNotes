//! The core of the Typst notes app.
//!
//! Everything that does not depend on how notes are shown (browser, Tauri, CLI):
//!
//! - [`vault`]: the vault - which files are notes and which are books;
//! - [`vaults`]: the user's vaults by name (`<data>/vaults/`);
//! - [`folders`]: vault folders - the title from `_folder.toml`;
//! - [`world`]: the Typst compiler over the vault (the virtual library
//!   `/_baluk/`, fonts, packages);
//! - [`vault_data`]: vault data for notes, `/_vault/...` (a registry of
//!   providers: the graph is [`vault_graph`]);
//! - [`themes`]: design themes from `baluk/theme.typ` -> CSS;
//! - [`render`]: theme HTML documents -> one page; processing is a chain of
//!   [`passes`] (joining figures, heading anchors, links between notes, ...);
//! - [`finish`]: passes after the cache, by the settings: [`figures`];
//! - [`book`]: a book by chapters - the page of one chapter;
//! - [`storage`]: vault files behind an interface (a directory, memory in tests);
//! - [`version`]: note versions by their files, a stable hash;
//! - [`pipeline`]: building a note - compiling -> rendering -> figures;
//! - [`page_cache`], [`cache`]: the page cache - memory (LRU) and disk;
//! - [`pages`]: a page on request over the build and the cache;
//! - [`warm`]: warming - everything is built ahead, to disk;
//! - [`notes`]: the core facade for the server and the CLI;
//! - [`check`]: checking a vault or a note - errors and broken links;
//! - [`new_note`]: a template for a new note or book (`notes new`);
//! - [`settings`]: the schema and storage of client settings.
//!
//! Decisions and their reasons: `docs/architecture.md`.

pub mod book;
pub mod cache;
pub mod check;
pub mod diag;
mod error;
pub mod figures;
pub mod finish;
pub mod folders;
pub mod fonts;
pub mod frames;
mod fsutil;
pub mod graph;
pub mod lint;
pub mod new_note;
pub mod notes;
pub mod outline;
pub mod packages;
pub mod page_cache;
pub mod pages;
pub mod passes;
pub mod pipeline;
pub mod rename;
pub mod render;
pub mod search;
pub mod settings;
pub mod storage;
pub mod themes;
pub mod vault;
pub mod vault_data;
pub mod vault_graph;
pub mod vaults;
pub mod version;
pub mod warm;
pub mod watch;
pub mod webfonts;
pub mod world;

pub use error::{Error, Result};
pub use notes::{NotePage, Notes, NotesConfig, SharedAssets};
pub use vault::{Entry, NoteId, NoteKind, Vault};
pub use vaults::{VaultName, Vaults};
pub use world::LibrarySource;
