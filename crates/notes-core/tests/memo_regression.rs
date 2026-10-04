//! Замер регрессии: потеря memo при релизе по простою.
//!
//! Меряем времена пересборок второй и третьей правки одной и той же большой
//! заметки без релиза (правки подряд) и с релизом (пауза ≥ idle между
//! правками). Числа печатаются; тест по умолчанию игнорируется. Запуск вручную:
//! `cargo test -p notes-core --features measure --test memo_regression --release -- --ignored --nocapture`.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

use std::fs;
use std::time::{Duration, Instant};

use notes_core::world::LibrarySource;
use notes_core::{NoteId, Notes, NotesConfig};

fn largest_note_id(vault: &std::path::Path) -> NoteId {
    fn walk(dir: &std::path::Path, acc: &mut Vec<(std::path::PathBuf, u64)>) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.is_dir() {
                walk(&p, acc);
            } else if p.extension().and_then(|s| s.to_str()) == Some("typ") {
                let len = fs::metadata(&p).unwrap().len();
                acc.push((p, len));
            }
        }
    }
    let mut files = Vec::new();
    walk(vault, &mut files);
    files.sort_by_key(|(_, len)| *len);
    let (path, _) = files.into_iter().rev().find(|(p, _)| !p.ends_with("main.typ")).expect("есть заметки");
    let rel = path.strip_prefix(vault).unwrap().to_string_lossy().replace('\\', "/");
    let id = rel.strip_suffix(".typ").unwrap();
    NoteId::new(id).unwrap()
}

fn load_vault_to_mem(mem: &notes_core::storage::MemStorage, dir: &std::path::Path, rel: &std::path::Path) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let next_rel = rel.join(entry.file_name());
        if path.is_dir() {
            load_vault_to_mem(mem, &path, &next_rel);
            continue;
        }
        let rel = next_rel.to_string_lossy().replace('\\', "/");
        let text = fs::read_to_string(path).unwrap_or_default();
        mem.write(&rel, text.as_str());
    }
}

#[test]
#[ignore = "тяжёлый замер пересборок"]
fn rebuild_times_with_and_without_idle_release() {
    // Хранилище — в памяти: правки не трогают репозиторий.
    let vault_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vault");
    let mem = std::sync::Arc::new(notes_core::storage::MemStorage::new());
    load_vault_to_mem(&mem, &vault_dir, std::path::Path::new(""));
    let cache_dir = tempfile::tempdir().unwrap();
    let cfg = NotesConfig {
        vault: vault_dir.clone(),
        library: LibrarySource::Embedded,
        font_dirs: vec![],
        cache: Some(cache_dir.path().to_path_buf()),
        trash: None,
    };
    let notes = Notes::with_storage(mem.clone(), &cfg).unwrap();
    // Настройки по умолчанию (memo=10).
    let id = largest_note_id(&vault_dir);

    // Первая сборка (заполняет кэш на диск и memo typst).
    let _ = notes.page(&id, notes_core::figures::FigureOptions::default()).unwrap();

    // Без релиза: 2-я и 3-я правка подряд.
    let path = format!("{id}.typ");
    mem.write(&path, "= Тест\nправка 1");
    let t2 = Instant::now();
    let _ = notes.page(&id, notes_core::figures::FigureOptions::default()).unwrap();
    let ms2_norel = t2.elapsed().as_millis();
    mem.write(&path, "= Тест\nправка 2");
    let t3 = Instant::now();
    let _ = notes.page(&id, notes_core::figures::FigureOptions::default()).unwrap();
    let ms3_norel = t3.elapsed().as_millis();

    // С релизом: пауза > idle (5 с) между правками.
    mem.write(&path, "= Тест\nправка 3");
    std::thread::sleep(Duration::from_millis(5600));
    let t2r = Instant::now();
    let _ = notes.page(&id, notes_core::figures::FigureOptions::default()).unwrap();
    let ms2_rel = t2r.elapsed().as_millis();
    mem.write(&path, "= Тест\nправка 4");
    std::thread::sleep(Duration::from_millis(5600));
    let t3r = Instant::now();
    let _ = notes.page(&id, notes_core::figures::FigureOptions::default()).unwrap();
    let ms3_rel = t3r.elapsed().as_millis();

    println!("largest_note={id} ms2_norel={ms2_norel} ms3_norel={ms3_norel} ms2_rel={ms2_rel} ms3_rel={ms3_rel}");
}
