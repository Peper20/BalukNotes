//! Device sync against a hub in this process: two "devices" are two data
//! directories. Each test uses its own account (one hub per test binary).
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

mod common;
mod rounds;
mod worker;
