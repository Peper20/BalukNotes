//! The light part of the core, without Typst (architecture §1, §9): what the
//! storage server (`notes-hub`) shares with the device core (`notes-core`).
//!
//! - [`fsutil`]: small file helpers (an atomic write);
//! - [`names`]: vault names;
//! - [`accounts`]: users and password hashes;
//! - [`sessions`]: sign-in sessions and the pause after a wrong password;
//! - [`sync`]: vault sync - the protocol, the server side of a vault, the
//!   device side and the engine between them.

pub mod accounts;
pub mod fsutil;
pub mod names;
pub mod sessions;
pub mod sync;
