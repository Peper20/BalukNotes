//! A label of the rendering code for the disk cache (`NOTES_RENDER_HASH`, see
//! `src/cache.rs`): a hash of the core sources the raw rendering depends on,
//! of the embedded fonts in `fonts/` and of the Typst crate versions in
//! `Cargo.lock`. A rebuild that does not touch them (server, client, CLI) keeps
//! the cache.
//!
//! Core modules that do **not** affect the raw rendering are listed in
//! [`AFTER_CACHE`]: editing them keeps the cache. Everything else in `src/`
//! (new files too) is part of the label: forgetting a file is safe, it only
//! costs an extra warm-up.

use std::error::Error;
use std::hash::Hasher as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

/// Modules that work after the cache or outside the build (paths from `src/`).
const AFTER_CACHE: &[&str] = &[
    "book.rs",    // cutting a finished page into chapters
    "cache.rs",   // the cache itself
    "check.rs",   // vault check
    "figures.rs", // figure processing: after the cache, by the settings
    "finish.rs",  // passes after the cache
    "folders.rs", // folder titles: for the list, not for the build
    "frames.rs",  // shared parts of frames: after figures
    "notes.rs",   // the facade
    "page_cache.rs",
    "pages.rs",
    "search.rs",
    "settings.rs",
    "vaults.rs", // vaults by name: outside the build
    "warm.rs",
    "watch.rs",    // the file watcher
    "webfonts.rs", // fonts for the browser
];

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
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
        let rel = file.strip_prefix(&src)?.to_string_lossy().replace('\\', "/");
        if !AFTER_CACHE.contains(&rel.as_str()) {
            add(&rel, &fs::read(&file)?);
        }
    }
    let fonts = repo.join("fonts");
    for file in files(&fonts) {
        let Some(name) = file.file_name() else { continue };
        let name = name.to_string_lossy().into_owned();
        if file.extension().is_some_and(|e| e == "ttf" || e == "otf") {
            add(&name, &fs::read(&file)?);
        }
    }
    // Typst versions: the `[[package]]` blocks of the typst* crates.
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
    Ok(())
}

/// All files of a directory, sorted by path.
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
