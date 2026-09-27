//! Метка кода отрисовки для кэша на диске (`NOTES_RENDER_HASH`, см.
//! `src/cache.rs`): хэш исходников ядра, от которых зависит сырая
//! отрисовка, встроенных шрифтов `fonts/` и версий крейтов Typst из
//! `Cargo.lock`. Пересборка, не тронувшая их (сервер, клиент, CLI), кэш не
//! сбрасывает.
//!
//! Модули ядра, которые **не** влияют на сырую отрисовку, перечислены в
//! [`AFTER_CACHE`]: их правка кэш не сбрасывает. Всё остальное в `src/`
//! (и новые файлы) входит в метку — забыть файл безопасно, лишний прогрев
//! и только.

use std::hash::Hasher as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

/// Модули, работающие после кэша или вне сборки (пути от `src/`).
const AFTER_CACHE: &[&str] = &[
    "book.rs",    // нарезка готовой страницы на главы
    "cache.rs",   // сам кэш
    "check.rs",   // проверка хранилища
    "figures.rs", // обработка рисунков — после кэша, по настройкам
    "finish.rs",  // проходы после кэша
    "frames.rs",  // общие части кадров — после рисунков
    "notes.rs",   // фасад
    "page_cache.rs",
    "pages.rs",
    "search.rs",
    "settings.rs",
    "warm.rs",
    "watch.rs",    // наблюдатель файлов
    "webfonts.rs", // шрифты для браузера
];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.join("../..");
    let mut h = siphasher::sip::SipHasher13::new();
    let mut add = |name: &str, data: &[u8]| {
        h.write(&(name.len() as u64).to_le_bytes());
        h.write(name.as_bytes());
        h.write(&(data.len() as u64).to_le_bytes());
        h.write(data);
    };

    let src = manifest.join("src");
    for file in files(&src) {
        let rel = file.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        if !AFTER_CACHE.contains(&rel.as_str()) {
            add(&rel, &fs::read(&file).unwrap());
        }
    }
    let fonts = repo.join("fonts");
    for file in files(&fonts) {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        if file.extension().is_some_and(|e| e == "ttf" || e == "otf") {
            add(&name, &fs::read(&file).unwrap());
        }
    }
    // Версии Typst: блоки `[[package]]` крейтов typst*.
    let lock = fs::read_to_string(repo.join("Cargo.lock")).unwrap_or_default();
    for block in lock.split("[[package]]") {
        if block.trim_start().starts_with("name = \"typst") {
            add("Cargo.lock", block.as_bytes());
        }
    }

    println!("cargo:rustc-env=NOTES_RENDER_HASH={:016x}", h.finish());
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed={}", fonts.display());
    println!("cargo:rerun-if-changed={}", repo.join("Cargo.lock").display());
}

/// Все файлы каталога, по порядку путей.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(read) = fs::read_dir(dir) else { return out };
    for e in read.flatten() {
        let path = e.path();
        if path.is_dir() {
            out.extend(files(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}
