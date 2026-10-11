//! End-to-end tests of the hub: the router in memory (`tower::oneshot`), a real
//! listening server with the HTTP client, and the `notes-hub` binary.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

mod auth;
mod cli;
mod common;
#[cfg(feature = "client")]
mod e2e;
mod sync;
