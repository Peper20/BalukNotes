//! Measures memory and the time of reopening after builds.
//!
//! The test is ignored by default: it is heavy and depends on /proc.
//! Run by hand: `cargo test -p notes-core --features measure --test memory_release -- --ignored`.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

use std::fs;
use std::time::{Duration, Instant};

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

fn reopen_time_ms(notes: &Notes, id: &NoteId) -> u128 {
    let start = Instant::now();
    // Reopening (the page is already in the disk cache).
    let _ = notes.page(id, notes_core::figures::FigureOptions::default()).unwrap();
    start.elapsed().as_millis()
}

#[test]
#[ignore = "a heavy memory measurement"]
fn measure_memory_and_reopen_time() {
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
    // Limit the page cache memory (less noise in measurements): applied directly.
    let device = notes_core::settings::Device {
        warm: notes_core::warm::WarmMode::All,
        builds: 2,
        memory: 8 * 1024 * 1024,
        disk: notes_core::cache::DiskLimits::default(),
        memo: 10,
        packages: vec![],
    };
    notes.apply_device(&device);

    // The baseline: before builds.
    let (rss0, hwm0) = rss_hwm_kib();

    // A full warming round.
    let stats = notes.warm_pass();
    assert!(stats.built > 0);
    // Wait until background disk writes surely finish.
    std::thread::sleep(Duration::from_millis(200));
    let (rss1, hwm1) = rss_hwm_kib();

    // Imitate an ordinary rebuild: an edit of one note.
    // A copy in tmp: no edits in the repository.
    let tmp_vault = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp_vault.path()).unwrap();
    let ssh_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vault/Сеть/SSH.typ");
    fs::copy(ssh_path, tmp_vault.path().join("Сеть-SSH.typ")).unwrap();
    // Building a new note from the temporary folder.
    let cfg2 = NotesConfig { vault: tmp_vault.path().to_path_buf(), ..config.clone() };
    let notes2 = Notes::open(&cfg2).unwrap();
    notes2.apply_device(&device);
    // Create a simple note and build it (an ordinary edit of one page).
    fs::write(tmp_vault.path().join("Новая.typ"), "= Новая\nтекст").unwrap();
    let id_new = NoteId::new("Новая").unwrap();
    let _ = notes2.page(&id_new, notes_core::figures::FigureOptions::default()).unwrap();
    let (rss2, hwm2) = rss_hwm_kib();

    // The time of reopening (a hit in the disk cache).
    let id = NoteId::new("Сеть/SSH").unwrap();
    let reopen_ms = reopen_time_ms(&notes, &id);

    // Print the result: it goes into docs/research/E5.md by hand.
    println!(
        "baseline_rss_kib={rss0} baseline_hwm_kib={hwm0} after_warm_rss_kib={rss1} after_warm_hwm_kib={hwm1} after_edit_rss_kib={rss2} after_edit_hwm_kib={hwm2} reopen_ms={reopen_ms} built={}",
        stats.built
    );
}
