//! The light part of the core, without Typst (architecture §1, §9): what the
//! storage server (`notes-hub`) shares with the device core (`notes-core`).
//!
//! - [`config`]: the config file and the data directory, one rule for every
//!   part;
//! - [`fsutil`]: small file helpers (an atomic write);
//! - [`names`]: vault names;
//! - [`vaults`]: the vault list in the data directory;
//! - [`note_id`]: note paths (`NoteId`) and their rules;
//! - [`storage`]: vault files behind an interface (a directory, memory in
//!   tests) and [`watch`]: the file watcher over it;
//! - [`platform`]: the device kind and the sync preference;
//! - [`accounts`]: users and password hashes;
//! - [`sessions`]: sign-in sessions and the pause after a wrong password;
//! - [`serve_rules`]: the defaults and sign-in rules of `notes serve`, also
//!   checked by `notes service install`;
//! - [`sync`]: vault sync - the protocol, the server side of a vault, the
//!   device side and the engine between them.

pub mod accounts;
pub mod config;
pub mod fsutil;
pub mod names;
pub mod note_id;
pub mod platform;
pub mod serve_rules;
pub mod sessions;
pub mod storage;
pub mod sync;
pub mod vaults;
pub mod watch;
