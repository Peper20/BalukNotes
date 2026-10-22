//! The fonts of the browser core as files: the module carries none
//! (architecture §10). `fonts <dir>` writes every font file the native binary
//! embeds as `<dir>/fonts/font-<hash>.bin` (the hash is 8 hex characters of the
//! content, so the name never changes for the same bytes) and the list
//! `<dir>/fonts.json` = `{ "fonts": ["fonts/font-<hash>.bin", ...] }`, sorted,
//! with paths relative to the list. The host fetches the list and the files
//! and hands them to `WasmCore.addFont` (`app/src/worker/load.ts`).
//! `harness/build.sh` runs it after the module.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use notes_core::fonts::embedded_files;
use notes_core::version::StableHasher;
use serde_json::json;

/// The name of a font file below `fonts/`: 8 lowercase hex characters of the
/// hash of its bytes.
fn file_name(data: &[u8]) -> String {
    let hash = StableHasher::new().bytes(data).finish();
    format!("font-{:08x}.bin", hash >> 32)
}

/// Writes the fonts and their list into `dir`; the old `<dir>/fonts/` goes first
/// so no stale file stays.
fn write(dir: &Path) -> Result<usize, String> {
    let mut files = BTreeMap::new();
    for data in embedded_files() {
        let name = file_name(&data);
        if files.insert(name.clone(), data).is_some() {
            return Err(format!("two font files have the same name {name}: lengthen the hash"));
        }
    }
    let fonts = dir.join("fonts");
    match fs::remove_dir_all(&fonts) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: {e}", fonts.display())),
    }
    fs::create_dir_all(&fonts).map_err(|e| format!("{}: {e}", fonts.display()))?;
    for (name, data) in &files {
        let path = fonts.join(name);
        fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    // A `BTreeMap` is sorted by name.
    let list = json!({ "fonts": files.keys().map(|name| format!("fonts/{name}")).collect::<Vec<_>>() });
    let path = dir.join("fonts.json");
    fs::write(&path, list.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(files.len())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(dir), None) = (args.next(), args.next()) else {
        eprintln!("usage: fonts <dir>");
        return ExitCode::from(2);
    };
    match write(Path::new(&dir)) {
        Ok(count) => {
            println!("{count} font files -> {dir}/fonts, {dir}/fonts.json");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fonts: {e}");
            ExitCode::FAILURE
        }
    }
}
