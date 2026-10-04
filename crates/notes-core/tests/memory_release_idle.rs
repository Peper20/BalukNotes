//! Measures idle RSS after warming with a short idle time (release on idle).
//!
//! Run by hand: `cargo test -p notes-core --features measure --test memory_release_idle --release -- --ignored --nocapture`.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

use std::fs;
use std::time::Duration;

use notes_core::world::LibrarySource;
use notes_core::{NoteId, Notes, NotesConfig};

fn rss_hwm_kib() -> (u64, u64) {
    // /proc/self/status: the VmRSS and VmHWM lines, in KiB.
    let status = fs::read_to_string("/proc/self/status").unwrap();
    let mut rss = 0u64;
    let mut hwm = 0u64;
    for line in status.lines() {
        if let Some(v) = line.strip_prefix("VmRSS:") {
            rss = v.split_whitespace().next().unwrap().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("VmHWM:") {
            hwm = v.split_whitespace().next().unwrap().parse().unwrap_or(0);
        }
    }
    (rss, hwm)
}

#[test]
#[ignore = "a heavy memory measurement"]
fn measure_idle_after_warm_release() {
    // The data directory: temporary, an empty cache.
    let cache_dir = tempfile::tempdir().unwrap();
    let vault_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vault");
    let config = NotesConfig {
        vault: vault_path,
        library: LibrarySource::Embedded,
        font_dirs: vec![],
        cache: Some(cache_dir.path().to_path_buf()),
        trash: None,
    };
    let notes = Notes::open(&config).unwrap();
    // Less noise: a small page memory budget and a memo limit.
    let device = notes_core::settings::Device {
        warm: notes_core::warm::WarmMode::All,
        builds: 2,
        memory: 8 * 1024 * 1024,
        disk: notes_core::cache::DiskLimits::default(),
        memo: 10,
        packages: vec![],
    };
    notes.apply_device(&device);

    let (rss0, _hwm0) = rss_hwm_kib();

    // A full warming round: store() of every build sets the release timer.
    let stats = notes.warm_pass();
    assert!(stats.built > 0);
    // Idle for longer than the threshold (IDLE_RELEASE_DELAY = 5 s).
    std::thread::sleep(Duration::from_millis(5600));
    let (rss1, hwm1) = rss_hwm_kib();
    let (warm_pages, warm_bytes) = notes.memory();

    // Imitate a single rebuild: an edit of one note.
    let tmp_vault = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp_vault.path()).unwrap();
    fs::write(tmp_vault.path().join("Новая.typ"), "= Новая\nтекст").unwrap();
    let cfg2 = NotesConfig { vault: tmp_vault.path().to_path_buf(), ..config.clone() };
    let notes2 = Notes::open(&cfg2).unwrap();
    notes2.apply_device(&device);
    let id_new = NoteId::new("Новая").unwrap();
    let _ = notes2.page(&id_new, notes_core::figures::FigureOptions::default()).unwrap();
    // Idle longer than the threshold again, for a release after the rebuild.
    std::thread::sleep(Duration::from_millis(5600));
    let (rss2, hwm2) = rss_hwm_kib();
    let (edit_pages, edit_bytes) = notes2.memory();

    println!(
        "idle_after_warm_rss_kib={rss1} idle_after_warm_hwm_kib={hwm1} idle_after_edit_rss_kib={rss2} idle_after_edit_hwm_kib={hwm2} baseline_rss_kib={rss0} warm_cache_pages={warm_pages} warm_cache_bytes={warm_bytes} edit_cache_pages={edit_pages} edit_cache_bytes={edit_bytes}"
    );
}
