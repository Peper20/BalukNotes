//! Vault sync (architecture §9): the protocol, the server side of a vault, the
//! device side and the engine between them. Pure logic and files: no HTTP, no
//! async, so everything is tested without a network.
//!
//! # The model
//!
//! A vault is a plain folder of files (notes are `.typ`). The server keeps a
//! full copy; every device keeps a full copy too. A sync works with file
//! versions, there is no merge of contents: a version is the SHA-256 of the
//! file. "The computer writes first, the others receive": the writing
//! computer syncs with [`Prefer::Local`], the receiving devices with
//! [`Prefer::Remote`]; they differ only when both changed a file.
//!
//! - [`path`]: which paths are synced, one rule for both sides;
//! - protocol types: [`Entry`], [`Changes`], [`Base`], [`Conflict`], [`Error`];
//! - [`HubVault`]: one vault on the server, in a directory: the plain tree, a
//!   manifest of versions with a change counter `seq`, and a history of
//!   replaced content (nothing is ever lost on the server);
//! - [`Tree`] and [`DirTree`] / [`MemTree`]: a device's copy of the vault.
//!   Content that sync removes or overwrites is moved to a "removed" folder,
//!   never deleted;
//! - [`State`]: what was synced last time, per file (the base of the next
//!   round);
//! - [`Remote`]: what the engine needs from the server. [`HubVault`]
//!   implements it for a local mode and tests; the HTTP client implements it
//!   over the network, the server routes call [`HubVault`] methods;
//! - [`sync`] / [`Report`]: one round; its algorithm is described in
//!   [`engine`]. A round that would delete a large part of the vault stops
//!   first ([`Deletions`], [`Held`]).
//!
//! # The write protocol
//!
//! A write says which version it replaces ([`Base`]): `Hash(h)` - exactly
//! this one, `Absent` - the file must not exist, `Any` - no check. The server
//! answers with the new [`Entry`] or a [`Conflict`] carrying what it has now.
//! The text form of a base (`any`, `absent`, the hex hash) is for an HTTP
//! header. The device pulls `changes(after)` - the entries with a bigger
//! `seq`, ordered, with the vault's current `seq` to ask from next time.
//!
//! # Known limits
//!
//! - A change is detected by size and modification time, then by hash: an
//!   edit that keeps both (same-size edit within one timestamp tick) is
//!   missed until the file changes again.
//! - Tombstones and `history/` are kept forever; the server has no API to
//!   read the history yet.
//! - One process owns a vault directory; there is no lock between processes.
//! - Paths are compared byte by byte: names that differ only in case or in
//!   Unicode normalization are different files (they may collide on a
//!   case-insensitive disk).
//! - Files over [`MAX_FILE_SIZE`] are not synced; files are held in memory
//!   while they are read, hashed and sent.

pub mod engine;
mod error;
mod guard;
mod hub;
pub mod path;
mod protocol;
mod state;
mod tree;

#[cfg(test)]
mod tests;

pub use engine::{Options, Prefer, Remote, Report, sync, sync_with};
pub use error::{Error, Result};
pub use guard::{Deletions, Held, MIN_FILES, MIN_SHARE_PERCENT, Side};
pub use hub::{HubVault, MAX_FILE_SIZE};
pub use protocol::{Base, Changes, Conflict, Entry, Outcome, hash_hex, is_hash};
pub use state::{State, Synced};
pub use tree::{DirTree, FileInfo, MemTree, Tree};
