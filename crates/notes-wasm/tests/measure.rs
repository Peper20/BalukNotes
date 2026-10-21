//! E4: the same builds as in the browser, natively (`docs/research/E4.md`).
//! The vault and packages are the ones `harness/run.mjs` prepared:
//!
//! ```sh
//! crates/notes-wasm/harness/run.mjs --prepare
//! E4_NOTES='демо/визуализация' cargo test --release -p notes-wasm --features measure --test measure -- --nocapture
//! ```
//!
//! One scenario per process: the peak RSS (`VmHWM`) is the process's own.
//! `E4_DIR` - another prepared directory (default `tests/.data/wasm`).

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "a measurement")]

use std::path::{Path, PathBuf};
use std::time::Instant;

use notes_wasm::Core;

/// The vault name in the API paths.
const VAULT: &str = "vault";

/// Percent-encodes a note path the way a browser sends it.
fn encode(path: &str) -> String {
    path.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-_.".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn add_dir(dir: &Path, add: &mut dyn FnMut(&str, &[u8])) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(dir).unwrap().to_str().unwrap().replace('\\', "/");
                add(&rel, &std::fs::read(&path).unwrap());
            }
        }
    }
}

fn peak_rss_mb() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches(" kB").trim().parse::<u64>().ok())
        .map_or(0, |kb| kb / 1024)
}

/// CPU time of the process (all threads), ms: what one thread would need.
fn cpu_ms() -> u64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat.rsplit(") ").next().unwrap_or_default().split(' ').collect();
    let ticks: u64 = fields.get(11..13).map_or(0, |f| f.iter().filter_map(|t| t.parse::<u64>().ok()).sum());
    ticks * 10
}

#[tokio::test]
async fn measure() {
    let dir = std::env::var_os("E4_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/.data/wasm"), PathBuf::from);
    let notes = std::env::var("E4_NOTES").unwrap_or_else(|_| "демо/визуализация".into());
    let mut core = Core::new(VAULT).unwrap();
    let started = Instant::now();
    add_dir(&dir.join("vault"), &mut |p, d| core.add_file(p, d));
    add_dir(&dir.join("packages"), &mut |p, d| core.add_package_file(p, d));
    core.open().unwrap();
    println!("open: {} ms, peak RSS {} MB", started.elapsed().as_millis(), peak_rss_mb());
    for note in notes.split(',') {
        let started = Instant::now();
        let cpu = cpu_ms();
        let response = core.request("GET", &format!("/api/vaults/{VAULT}/notes/{}", encode(note)), b"").await;
        assert_eq!(response.status, 200, "{note}");
        let page: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        let ms = started.elapsed().as_millis();
        let html = page["rendered"]["body"].as_str().map_or(0, str::len)
            + page["rendered"]["styles"].as_str().map_or(0, str::len);
        println!(
            "{note}: {ms} ms (CPU {} ms), html {} KB, errors {}, peak RSS {} MB",
            cpu_ms() - cpu,
            html / 1000,
            page["errors"].as_array().map_or(0, Vec::len),
            peak_rss_mb()
        );
        // The body, to compare with the browser build (`harness/run.mjs` writes `.wasm.html`).
        let html_dir = dir.join("html");
        std::fs::create_dir_all(&html_dir).unwrap();
        let body = page["rendered"]["body"].as_str().unwrap_or_default();
        std::fs::write(html_dir.join(format!("{}.native.html", note.replace('/', "_"))), body).unwrap();
        for e in page["errors"].as_array().into_iter().flatten().take(3) {
            println!("  {e}");
        }
    }
}
