//! Ядро приложения заметок на Typst.
//!
//! Всё, что не зависит от способа показа (браузер, Tauri, CLI):
//!
//! - [`vault`] — хранилище: какие файлы — заметки, какие — книги;
//! - [`world`] — компилятор Typst поверх хранилища (виртуальная библиотека
//!   `/_konspekt/`, шрифты, пакеты);
//! - [`themes`] — темы оформления из `konspekt/theme.typ` → CSS;
//! - [`render`] — HTML-документы тем → одна страница (склейка рисунков,
//!   якоря заголовков, ссылки между заметками);
//! - [`notes`] — ленивая компиляция с кэшем и версиями по файлам;
//! - [`check`] — проверка всего хранилища: ошибки и битые ссылки;
//! - [`settings`] — схема и хранение настроек клиента.
//!
//! Решения и их причины — `docs/architecture.md`.

pub mod check;
pub mod diag;
mod error;
pub mod figures;
pub mod fonts;
pub mod notes;
pub mod render;
pub mod settings;
pub mod themes;
pub mod vault;
pub mod world;

pub use error::{Error, Result};
pub use notes::{LinkStyle, NotePage, Notes, NotesConfig};
pub use vault::{Entry, NoteId, NoteKind, Vault};
