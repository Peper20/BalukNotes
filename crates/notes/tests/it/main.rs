//! The built `notes` binary: its light commands and the calls of the parts.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

mod common;
#[cfg(unix)]
mod light;
#[cfg(unix)]
mod parts;
#[cfg(feature = "sync")]
mod sync;
