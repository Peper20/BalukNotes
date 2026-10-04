//! Сквозные тесты сервера — одним бинарником с общим ядром на фикстуру.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

mod api;
mod common;
mod csp;
