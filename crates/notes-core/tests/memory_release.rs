//! Измерение памяти и времени повторного открытия после сборок.
//!
//! Тест игнорируется по умолчанию: он тяжёлый и зависит от /proc.
//! Запуск вручную: `cargo test -p notes-core --features measure --test memory_release -- --ignored`.

use std::fs;
use std::time::{Duration, Instant};

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

fn reopen_time_ms(notes: &Notes, id: &NoteId) -> u128 {
    let start = Instant::now();
    // Повторное открытие (страница уже в кэше на диске).
    let _ = notes.page(id, notes_core::figures::FigureOptions::default()).unwrap();
    start.elapsed().as_millis()
}

#[test]
#[ignore = "тяжёлое измерение памяти"]
fn measure_memory_and_reopen_time() {
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
    // Ограничить память кэша страниц (меньше шума измерений): применим напрямую.
    let device = notes_core::settings::Device {
        warm: notes_core::warm::WarmMode::All,
        builds: 2,
        memory: 8 * 1024 * 1024,
        disk: notes_core::cache::DiskLimits::default(),
        memo: 10,
        packages: vec![],
    };
    notes.apply_device(&device);

    // Базовая точка: до сборок.
    let (rss0, hwm0) = rss_hwm_kib();

    // Полный проход прогрева.
    let stats = notes.warm_pass();
    assert!(stats.built > 0);
    // Подождать, пока фоновая запись на диск наверняка завершится.
    std::thread::sleep(Duration::from_millis(200));
    let (rss1, hwm1) = rss_hwm_kib();

    // Имитация обычной пересборки: правка одной заметки.
    // Копия в tmp: правки в репозитории не делаем.
    let tmp_vault = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp_vault.path()).unwrap();
    let ssh_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vault/Сеть/SSH.typ");
    fs::copy(ssh_path, tmp_vault.path().join("Сеть-SSH.typ")).unwrap();
    // Сборка новой заметки из временной папки.
    let cfg2 = NotesConfig { vault: tmp_vault.path().to_path_buf(), ..config.clone() };
    let notes2 = Notes::open(&cfg2).unwrap();
    notes2.apply_device(&device);
    // Создать простую заметку и собрать её (обычная правка одной страницы).
    fs::write(tmp_vault.path().join("Новая.typ"), "= Новая\nтекст").unwrap();
    let id_new = NoteId::new("Новая").unwrap();
    let _ = notes2.page(&id_new, notes_core::figures::FigureOptions::default()).unwrap();
    let (rss2, hwm2) = rss_hwm_kib();

    // Время повторного открытия (попадание в кэш на диске).
    let id = NoteId::new("Сеть/SSH").unwrap();
    let reopen_ms = reopen_time_ms(&notes, &id);

    // Печать результата: его собираем в docs/research/E5.md вручную.
    println!(
        "baseline_rss_kib={rss0} baseline_hwm_kib={hwm0} after_warm_rss_kib={rss1} after_warm_hwm_kib={hwm1} after_edit_rss_kib={rss2} after_edit_hwm_kib={hwm2} reopen_ms={reopen_ms} built={}",
        stats.built
    );
}
