//! Ядро приложения заметок на Typst.
//!
//! Всё, что не зависит от способа показа (браузер, Tauri, CLI):
//!
//! - [`vault`] — хранилище: какие файлы — заметки, какие — книги;
//! - [`world`] — компилятор Typst поверх хранилища (виртуальная библиотека
//!   `/_baluk/`, шрифты, пакеты);
//! - [`themes`] — темы оформления из `baluk/theme.typ` → CSS;
//! - [`render`] — HTML-документы тем → одна страница (склейка рисунков,
//!   якоря заголовков, ссылки между заметками);
//! - [`book`] — книга по главам: страница одной главы;
//! - [`storage`] — файлы хранилища за интерфейсом (каталог, в тестах — память);
//! - [`version`] — версии заметок по файлам, стабильный хэш;
//! - [`notes`] — ленивая компиляция с кэшем и версиями по файлам;
//! - [`check`] — проверка всего хранилища: ошибки и битые ссылки;
//! - [`settings`] — схема и хранение настроек клиента.
//!
//! Решения и их причины — `docs/architecture.md`.

pub mod book;
pub mod cache;
pub mod check;
pub mod diag;
mod error;
pub mod figures;
pub mod fonts;
mod fsutil;
pub mod graph;
pub mod lint;
pub mod notes;
pub mod outline;
pub mod render;
pub mod search;
pub mod settings;
pub mod storage;
pub mod themes;
pub mod vault;
pub mod vault_graph;
pub mod version;
pub mod warm;
pub mod webfonts;
pub mod world;

pub use error::{Error, Result};
pub use notes::{LinkStyle, NotePage, Notes, NotesConfig};
pub use vault::{Entry, NoteId, NoteKind, Vault};
pub use world::LibrarySource;
