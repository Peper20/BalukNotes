//! Server end-to-end tests: one binary with one core for the fixture.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

mod api;
mod common;
mod csp;
