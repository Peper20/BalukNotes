//! Измерение idle RSS после прогрева с коротким простоем (релиз по простою).
//!
//! Запуск вручную: `cargo test -p notes-core --features measure --test memory_release_idle --release -- --ignored --nocapture`.

use std::fs;
use std::time::Duration;

use notes_core::world::LibrarySource;
use notes_core::{NoteId, Notes, NotesConfig};

fn rss_hwm_kib() -> (u64, u64) {
    // /proc/self/status: строки VmRSS и VmHWM в КиБ.
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
#[ignore = "тяжёлое измерение памяти"]
fn measure_idle_after_warm_release() {
    // Папка данных: временная, пустой кэш.
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
    // Уменьшить шум: небольшой бюджет памяти страниц и ограничение memo.
    let device = notes_core::settings::Device {
        warm: notes_core::warm::WarmMode::All,
        builds: 2,
        memory: 8 * 1024 * 1024,
        disk: notes_core::cache::DiskLimits::default(),
        memo: 10,
    };
    notes.apply_device(&device);

    let (rss0, _hwm0) = rss_hwm_kib();

    // Полный проход прогрева: store() каждой сборки ставит таймер релиза.
    let stats = notes.warm_pass();
    assert!(stats.built > 0);
    // Дать простой больше порога (IDLE_RELEASE_DELAY = 5 с).
    std::thread::sleep(Duration::from_millis(5600));
    let (rss1, hwm1) = rss_hwm_kib();
    let (warm_pages, warm_bytes) = notes.memory();

    // Имитация одиночной пересборки: правка одной заметки.
    let tmp_vault = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp_vault.path()).unwrap();
    fs::write(tmp_vault.path().join("Новая.typ"), "= Новая\nтекст").unwrap();
    let cfg2 = NotesConfig { vault: tmp_vault.path().to_path_buf(), ..config.clone() };
    let notes2 = Notes::open(&cfg2).unwrap();
    notes2.apply_device(&device);
    let id_new = NoteId::new("Новая").unwrap();
    let _ = notes2.page(&id_new, notes_core::figures::FigureOptions::default()).unwrap();
    // Ещё простой больше порога для релиза после пересборки.
    std::thread::sleep(Duration::from_millis(5600));
    let (rss2, hwm2) = rss_hwm_kib();
    let (edit_pages, edit_bytes) = notes2.memory();

    println!(
        "idle_after_warm_rss_kib={rss1} idle_after_warm_hwm_kib={hwm1} idle_after_edit_rss_kib={rss2} idle_after_edit_hwm_kib={hwm2} baseline_rss_kib={rss0} warm_cache_pages={warm_pages} warm_cache_bytes={warm_bytes} edit_cache_pages={edit_pages} edit_cache_bytes={edit_bytes}"
    );
}
