//! `notes-typst serve`: the sign-in rules of the TCP address, on the binary.

#![allow(clippy::expect_used, reason = "test helpers fail the test by panicking")]

use std::process::{Command, Output};

/// `notes-typst --data <data> <args>` with a clean environment.
fn run(data: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_notes-typst"))
        .arg("--data")
        .arg(data)
        .args(args)
        .env_remove("NOTES_TOKEN")
        .env_remove("NOTES_VERSION")
        .output()
        .expect("run notes-typst")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_network_address_needs_auth() {
    let data = tempfile::tempdir().unwrap();
    let out = run(data.path(), &["serve", "--addr", "0.0.0.0:8449"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("not a localhost address") && err.contains("add --auth"), "{err}");
    assert!(err.contains("HTTPS"), "{err}");
}

#[test]
fn auth_needs_users() {
    let data = tempfile::tempdir().unwrap();
    for addr in ["127.0.0.1:8449", "0.0.0.0:8449"] {
        let out = run(data.path(), &["serve", "--addr", addr, "--auth"]);
        assert!(!out.status.success());
        let err = stderr(&out);
        assert!(err.contains("error: no users: create one: notes users add <login>"), "{err}");
    }
}

#[test]
fn the_token_flag_is_gone() {
    let data = tempfile::tempdir().unwrap();
    let out = run(data.path(), &["serve", "--token", "x"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--token"));
}
