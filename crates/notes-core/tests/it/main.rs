//! Core end-to-end tests in one binary: one core for the fixture
//! (`common::NOTES`) builds its notes once for every module, and one binary
//! links instead of several (each is hundreds of MB).
//! One module: `cargo test -p notes-core --test it snapshots::`.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

mod common;
mod library;
mod packages;
mod sanitize;
mod snapshots;
mod vault;
