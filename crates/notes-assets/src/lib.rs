//! Сборка клиента `app/` (`npm run build` → `app/dist`): страница-оболочка
//! и файлы `assets/` — общие для сервера (`notes-server`) и статического
//! сайта (`notes-site`).
//!
//! В отладочной сборке файлы читаются с диска (пересобрали клиент —
//! перезагрузите страницу), в релизной — встроены в бинарник (без
//! `app/dist` её не собрать, см. `build.rs`).

use std::borrow::Cow;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../app/dist/"]
#[allow_missing = true]
struct Dist;

/// Файл сборки клиента.
#[derive(Debug, Clone)]
pub struct File {
    pub data: Cow<'static, [u8]>,
    /// MIME-тип по расширению.
    pub mime: String,
}

fn get(path: &str) -> Option<File> {
    Dist::get(path).map(|f| File { mime: f.metadata.mimetype().to_owned(), data: f.data })
}

/// Страница-оболочка клиента (`index.html`); `None` — клиент не собран.
pub fn index_html() -> Option<File> {
    get("index.html")
}

/// Файл `assets/{path}` (`baluk.css`, `static.js`, `index-XXXXXXXX.js`, …).
pub fn asset(path: &str) -> Option<File> {
    get(&format!("assets/{path}"))
}

/// Имена файлов `assets/`, начинающихся с `prefix` (части скрипта сайта
/// `static*.js`), по алфавиту.
pub fn asset_names(prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = Dist::iter()
        .filter_map(|p| p.strip_prefix("assets/").filter(|n| n.starts_with(prefix)).map(str::to_owned))
        .collect();
    names.sort();
    names
}
