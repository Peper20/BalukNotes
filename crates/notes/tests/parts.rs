//! `notes` вызывает часть из своей папки: копия бинарника во временной папке
//! и поддельная часть - сценарий оболочки.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn setup() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes");
    std::fs::copy(env!("CARGO_BIN_EXE_notes"), &notes).unwrap();
    (dir, notes)
}

fn fake_part(dir: &Path, name: &str) {
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\necho \"$0 $NOTES_VERSION $*\"\nexit 7\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn notes(bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    // Только что записанный файл может быть занят: его дескриптор на запись
    // унаследовал процесс, запущенный соседним тестом (ETXTBSY) - повторить.
    let out = (0..50)
        .find_map(|_| match Command::new(bin).args(args).env_remove("NOTES_VERSION").output() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(20));
                None
            }
            out => Some(out.unwrap()),
        })
        .expect("файл занят");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into(), String::from_utf8_lossy(&out.stderr).into())
}

#[test]
fn passes_args_version_and_exit_code() {
    let (dir, bin) = setup();
    fake_part(dir.path(), "notes-typst");
    let (code, out, _) = notes(&bin, &["--vault", "app", "list", "-x"]);
    assert_eq!(code, Some(7));
    assert!(out.ends_with(&format!("notes-typst {} --vault app list -x\n", notes::VERSION)), "{out}");
}

#[test]
fn app_goes_to_its_part() {
    let (dir, bin) = setup();
    fake_part(dir.path(), "notes-app");
    let (_, out, _) = notes(&bin, &["app"]);
    assert!(out.contains("notes-app"), "{out}");
}

#[test]
fn missing_part_is_named() {
    let (_dir, bin) = setup();
    let (code, _, err) = notes(&bin, &["app"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("notes-app") && err.contains("не установлена"), "{err}");
}

#[test]
fn help_and_version_list_parts() {
    let (dir, bin) = setup();
    fake_part(dir.path(), "notes-typst");
    let (code, out, _) = notes(&bin, &["--help"]);
    assert_eq!(code, Some(7), "код справки - код части");
    assert!(out.contains("--help") && out.contains("есть  notes-typst") && out.contains("нет   notes-app"), "{out}");
    let (code, out, _) = notes(&bin, &["--version"]);
    assert_eq!(code, Some(0));
    assert!(out.starts_with(&format!("notes {}\n", notes::VERSION)), "{out}");
}
