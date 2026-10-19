//! The light commands: `notes` runs them itself, from a directory without any part.

use crate::common::{notes, notes_with, setup};

#[test]
fn vaults_lists_and_creates() {
    let (_dir, bin) = setup();
    let data = tempfile::tempdir().unwrap();
    let data = data.path().to_str().unwrap();
    let (code, out, err) = notes(&bin, &["--data", data, "vaults"]);
    assert_eq!((code, out.as_str()), (Some(0), ""), "{err}");
    assert!(err.contains("no vaults: create one"), "{err}");

    let (code, out, err) = notes(&bin, &["vaults", "new", "Учёба", "--data", data]);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.trim_end().ends_with("vaults/Учёба"), "{out}");
    assert!(err.contains("in the app: notes app --vault \"Учёба\""), "{err}");

    let (code, out, _) = notes(&bin, &["--data", data, "vaults"]);
    assert_eq!((code, out.as_str()), (Some(0), "Учёба\n"));
    let (code, _, err) = notes(&bin, &["--data", data, "vaults", "new", "Учёба"]);
    assert_eq!(code, Some(1), "an existing vault is an error");
    assert!(err.starts_with("error: "), "{err}");
}

#[test]
fn docs_print_the_texts() {
    let (_dir, bin) = setup();
    let (code, out, _) = notes(&bin, &["docs", "writing"]);
    assert_eq!(code, Some(0));
    assert_eq!(out, include_str!("../../../../docs/writing.md"));
    let (code, out, _) = notes(&bin, &["docs", "library"]);
    assert_eq!(code, Some(0));
    assert_eq!(out, include_str!("../../../../baluk/README.md"));
    let (code, _, err) = notes(&bin, &["docs"]);
    assert_eq!(code, Some(2), "clap's usage error");
    assert!(err.contains("Usage: notes docs <TOPIC>"), "{err}");
}

#[cfg(feature = "sync")]
#[test]
fn sync_status_needs_no_server() {
    let (_dir, bin) = setup();
    let data = tempfile::tempdir().unwrap();
    let data = data.path().to_str().unwrap();
    let (code, out, err) = notes(&bin, &["sync", "status", "--data", data]);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.contains("not signed in: notes sync login <server> --login <name>"), "{out}");
    let (code, out, _) = notes(&bin, &["--data", data, "sync", "status", "--json"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("\"signed_in\": false"), "{out}");
    let (code, _, err) = notes(&bin, &["--data", data, "sync", "link"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("give the vault: --vault"), "{err}");
}

#[test]
fn a_part_command_still_needs_its_part() {
    let (_dir, bin) = setup();
    let (code, out, err) = notes(&bin, &["list", "--vault", "x"]);
    assert_eq!(code, Some(1));
    assert!(out.is_empty());
    assert!(err.contains("belongs to the part notes-typst") && err.contains("not installed"), "{err}");
}

#[test]
fn service_checks_before_it_touches_systemd() {
    let (_dir, bin) = setup();
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let data = data.path().to_str().unwrap();
    let run = |args: &[&str]| {
        notes_with(&bin, args, |c| {
            c.env("XDG_CONFIG_HOME", home.path());
        })
    };
    // The rules of `notes serve` are checked first, with the same messages.
    let (code, _, err) = run(&["--data", data, "service", "install", "--addr", "0.0.0.0:8421"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("not a localhost address") && err.contains("add --auth"), "{err}");
    let (code, _, err) = run(&["--data", data, "service", "install", "--auth"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("no users: create one: notes users add <login>"), "{err}");
    // The unit runs the part `notes-typst`: without it there is nothing to install.
    let (code, _, err) = run(&["--data", data, "service", "install"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("runs the part notes-typst") && err.contains("not installed"), "{err}");
    assert!(!home.path().join("systemd").exists(), "no unit is written");
    // Status reads the unit file only.
    let (code, out, _) = run(&["service", "status"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("service: not installed"), "{out}");
}
