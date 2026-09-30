//! Ядро приложения заметок на Typst.
//!
//! Всё, что не зависит от способа показа (браузер, Tauri, CLI):
//!
//! - [`vault`] — хранилище: какие файлы — заметки, какие — книги;
//! - [`vaults`] — хранилища пользователя по именам (`<данные>/vaults/`);
//! - [`folders`] — папки хранилища: название из `_folder.toml`;
//! - [`world`] — компилятор Typst поверх хранилища (виртуальная библиотека
//!   `/_baluk/`, шрифты, пакеты);
//! - [`vault_data`] — данные хранилища для заметок `/_vault/…` (реестр
//!   поставщиков: граф — [`vault_graph`]);
//! - [`themes`] — темы оформления из `baluk/theme.typ` → CSS;
//! - [`render`] — HTML-документы тем → одна страница; обработка —
//!   цепочка проходов [`passes`] (склейка рисунков, якоря заголовков,
//!   ссылки между заметками, …);
//! - [`finish`] — проходы после кэша, под настройки: [`figures`] (рисунки);
//! - [`book`] — книга по главам: страница одной главы;
//! - [`storage`] — файлы хранилища за интерфейсом (каталог, в тестах — память);
//! - [`version`] — версии заметок по файлам, стабильный хэш;
//! - [`pipeline`] — сборка заметки: компиляция → отрисовка → рисунки;
//! - [`page_cache`], [`cache`] — кэш страниц: память (LRU) и диск;
//! - [`pages`] — страница по запросу поверх сборки и кэша;
//! - [`warm`] — прогрев: всё собирается заранее, на диск;
//! - [`notes`] — фасад ядра для сервера и CLI;
//! - [`check`] — проверка хранилища или заметки: ошибки и битые ссылки;
//! - [`new_note`] — заготовка новой заметки или книги (`notes new`);
//! - [`settings`] — схема и хранение настроек клиента.
//!
//! Решения и их причины — `docs/architecture.md`.

pub mod book;
pub mod cache;
pub mod check;
pub mod diag;
mod error;
pub mod figures;
pub mod finish;
pub mod folders;
pub mod fonts;
pub mod frames;
mod fsutil;
pub mod graph;
pub mod lint;
pub mod new_note;
pub mod notes;
pub mod outline;
pub mod page_cache;
pub mod pages;
pub mod passes;
pub mod pipeline;
pub mod rename;
pub mod render;
pub mod search;
pub mod settings;
pub mod storage;
pub mod themes;
pub mod vault;
pub mod vault_data;
pub mod vault_graph;
pub mod vaults;
pub mod version;
pub mod warm;
pub mod watch;
pub mod webfonts;
pub mod world;

pub use error::{Error, Result};
pub use notes::{NotePage, Notes, NotesConfig};
pub use vault::{Entry, NoteId, NoteKind, Vault};
pub use vaults::{VaultName, Vaults};
pub use world::LibrarySource;
