//! Сквозные тесты ядра — одним бинарником: одно ядро на фикстуру
//! (`common::NOTES`) собирает её заметки один раз на все модули, и
//! линкуется один бинарник вместо нескольких (каждый — сотни МБ).
//! Один модуль — `cargo test -p notes-core --test it snapshots::`.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

mod common;
mod library;
mod packages;
mod sanitize;
mod snapshots;
mod vault;
