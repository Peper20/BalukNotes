//! A copy of the `notes` binary in a temporary directory (its folder of parts)
//! and fake parts, shell scripts.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `notes` alone in a new directory: no part is installed.
pub fn setup() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes");
    std::fs::copy(env!("CARGO_BIN_EXE_notes"), &notes).unwrap();
    (dir, notes)
}

/// A part that prints its name, `NOTES_VERSION` and arguments, and exits with 7.
pub fn fake_part(dir: &Path, name: &str) {
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\necho \"$0 $NOTES_VERSION $*\"\nexit 7\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Runs `bin` with a clean environment: the exit code, stdout and stderr.
pub fn notes(bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    notes_with(bin, args, |_| {})
}

/// As [`notes`], with a hand to change the command (more environment).
pub fn notes_with(bin: &Path, args: &[&str], tune: impl Fn(&mut Command)) -> (Option<i32>, String, String) {
    // A file just written may be busy: a process started by a neighbouring
    // test inherited its write descriptor (ETXTBSY), so retry.
    let out = (0..50)
        .find_map(|_| {
            let mut command = Command::new(bin);
            command.args(args).env_remove("NOTES_VERSION").env_remove("NOTES_DATA");
            tune(&mut command);
            match command.output() {
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    None
                }
                out => Some(out.unwrap()),
            }
        })
        .expect("the file stays busy");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into(), String::from_utf8_lossy(&out.stderr).into())
}
