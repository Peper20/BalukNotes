//! `notes-typst sync ...` on the binary, against a hub in this process.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test helpers fail the test by panicking")]

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::LazyLock;

use notes_device::testing::{PASSWORD, TestHub};

static HUB: LazyLock<TestHub> = LazyLock::new(|| TestHub::start(&["ivan", "anna", "pavel", "kira", "boris"]));

/// `notes-typst --data <data> <args>` with a clean environment; `stdin` is
/// what the command reads.
fn run(data: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_notes-typst"))
        .arg("--data")
        .arg(data)
        .args(args)
        .env_remove("NOTES_TOKEN")
        .env_remove("NOTES_VERSION")
        .env_remove("NOTES_DATA")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run notes-typst");
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn ok(o: &Output) -> String {
    assert!(o.status.success(), "{}", err(o));
    out(o)
}

fn login(data: &Path, who: &str) {
    let o = run(data, &["sync", "login", &HUB.url, "--login", who, "--password-stdin"], &format!("{PASSWORD}\n"));
    assert_eq!(ok(&o).trim(), format!("signed in to {} as {who}", HUB.url));
}

fn write(data: &Path, vault: &str, file: &str, text: &str) {
    let path = data.join("vaults").join(vault).join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn a_session_from_sign_in_to_sign_out() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (a.path(), b.path());

    // Nothing works before signing in, and the message says what to do.
    let o = run(a, &["sync", "now"], "");
    assert!(!o.status.success());
    assert!(err(&o).contains("error: not signed in: notes sync login <server> --login <name>"), "{}", err(&o));
    let o = run(a, &["sync", "link", "--vault", "cli"], "");
    assert!(err(&o).contains("not signed in: notes sync login"), "{}", err(&o));
    assert!(ok(&run(a, &["sync", "status"], "")).contains("not signed in: notes sync login <server> --login <name>"));

    login(a, "ivan");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(a.join("sync/account.json")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // Link: created on the server and uploaded.
    write(a, "cli", "a.typ", "= A\n");
    write(a, "cli", "Сеть/ssh.typ", "= SSH\n");
    let o = run(a, &["sync", "link", "--vault", "cli"], "");
    assert_eq!(ok(&o).trim(), "cli: linked; uploaded 2, downloaded 0, removed 0 local / 0 on the server");

    let status: serde_json::Value = serde_json::from_str(&ok(&run(a, &["sync", "status", "--json"], ""))).unwrap();
    assert_eq!((status["signed_in"].clone(), status["login"].clone()), (true.into(), "ivan".into()));
    let vault = &status["vaults"][0];
    assert_eq!(
        (vault["name"].as_str(), vault["linked"].as_bool(), vault["remote"].as_bool()),
        (Some("cli"), Some(true), Some(true))
    );
    assert_eq!(vault["report"]["uploaded"], 2);
    assert!(vault["last_sync"].as_u64().unwrap() > 1_700_000_000);
    let text = ok(&run(a, &["sync", "status"], ""));
    assert!(text.contains(&format!("server: {} (signed in as ivan)", HUB.url)), "{text}");
    assert!(text.contains("cli: linked, synced 20") && text.contains("uploaded 2, downloaded 0"), "{text}");

    // The other computer has only the server's copy.
    login(b, "ivan");
    let o = run(b, &["sync", "link", "--vault", "cli"], "");
    assert_eq!(ok(&o).trim(), "cli: linked; uploaded 0, downloaded 2, removed 0 local / 0 on the server");
    assert_eq!(std::fs::read_to_string(b.join("vaults/cli/Сеть/ssh.typ")).unwrap(), "= SSH\n");

    // An edit travels with `now`; with no --vault, every linked vault.
    write(a, "cli", "a.typ", "= A, edited\n");
    assert_eq!(
        ok(&run(a, &["sync", "now"], "")).trim(),
        "cli: uploaded 1, downloaded 0, removed 0 local / 0 on the server"
    );
    assert_eq!(
        ok(&run(b, &["sync", "now", "--vault", "cli"], "")).trim(),
        "cli: uploaded 0, downloaded 1, removed 0 local / 0 on the server"
    );

    // A conflict follows the device setting: here the server's wins.
    std::fs::write(b.join("settings.json"), r#"{"device.sync_prefer": "remote"}"#).unwrap();
    write(a, "cli", "a.typ", "= A, the third version\n");
    write(b, "cli", "a.typ", "= B\n");
    ok(&run(a, &["sync", "now"], ""));
    let line = ok(&run(b, &["sync", "now"], ""));
    assert!(line.trim().ends_with("conflicts: a.typ"), "{line}");
    assert_eq!(std::fs::read_to_string(b.join("vaults/cli/a.typ")).unwrap(), "= A, the third version\n");

    // Unlink keeps the files; a round no longer works for it.
    let o = run(b, &["sync", "unlink", "--vault", "cli"], "");
    assert!(ok(&o).contains("cli: unlinked"), "{}", out(&o));
    assert!(b.join("vaults/cli/a.typ").is_file());
    let o = run(b, &["sync", "now", "--vault", "cli"], "");
    assert!(err(&o).contains("vault \"cli\" is not linked"), "{}", err(&o));
    assert!(ok(&run(b, &["sync", "now"], "")).is_empty(), "no linked vaults is not an error");

    // Sign out: linked vaults stay linked, the session is gone on the server.
    let o = run(a, &["sync", "logout"], "");
    assert_eq!(ok(&o).trim(), format!("signed out of {} (ivan)", HUB.url));
    assert!(!a.join("sync/account.json").exists() && a.join("sync/vaults/cli.json").is_file());
    let o = run(a, &["sync", "now"], "");
    assert!(!o.status.success() && err(&o).contains("not signed in"), "{}", err(&o));
    assert!(err(&run(a, &["sync", "logout"], "")).contains("not signed in"));
}

#[test]
fn clear_errors() {
    let data = tempfile::tempdir().unwrap();
    let data = data.path();

    // A wrong password.
    let o = run(data, &["sync", "login", &HUB.url, "--login", "pavel", "--password-stdin"], "wrong\n");
    assert!(!o.status.success());
    assert!(err(&o).contains("error: wrong login or password"), "{}", err(&o));

    // Nobody listens; a server without a scheme is https.
    let o = run(data, &["sync", "login", "127.0.0.1:1", "--login", "pavel", "--password-stdin"], "x\n");
    assert!(err(&o).contains("error: cannot reach the server https://127.0.0.1:1: "), "{}", err(&o));
    let o = run(data, &["sync", "login", "ftp://x", "--login", "pavel", "--password-stdin"], "x\n");
    assert!(err(&o).contains("only http:// and https:// are supported"), "{}", err(&o));

    // Plain http to another computer is allowed with a warning (it fails here
    // at the empty password, before any connection).
    let o = run(data, &["sync", "login", "http://192.0.2.1:9", "--login", "pavel", "--password-stdin"], "\n");
    assert!(err(&o).contains("warning: http://192.0.2.1:9 is plain http to another computer"), "{}", err(&o));
    assert!(err(&o).contains("the password is empty"));
    let o = run(data, &["sync", "login", "http://localhost:1", "--login", "pavel", "--password-stdin"], "\n");
    assert!(!err(&o).contains("warning"), "this computer is fine");

    login(data, "anna");
    let o = run(data, &["sync", "link", "--vault", "ghost"], "");
    assert!(err(&o).contains("error: vault \"ghost\" exists neither on this device nor on the server"), "{}", err(&o));
    let o = run(data, &["sync", "link"], "");
    assert!(err(&o).contains("give the vault: --vault"), "{}", err(&o));

    // The session ended on the server (`notes users passwd` does this).
    write(data, "dead", "a.typ", "= A");
    ok(&run(data, &["sync", "link", "--vault", "dead"], ""));
    HUB.hub.auth().sessions().revoke_user("anna").unwrap();
    write(data, "dead", "b.typ", "= B");
    let o = run(data, &["sync", "now"], "");
    assert_eq!(o.status.code(), Some(1));
    assert!(
        err(&o)
            .contains(&format!("error: the session ended: sign in again: notes sync login {} --login anna", HUB.url)),
        "{}",
        err(&o)
    );
    let text = ok(&run(data, &["sync", "status"], ""));
    assert!(text.contains("the last sync failed") && text.contains("the session ended"), "{text}");
}

#[test]
fn now_goes_through_every_vault_and_fails_if_one_does() {
    let data = tempfile::tempdir().unwrap();
    let data = data.path();
    login(data, "kira");
    for vault in ["one", "two"] {
        write(data, vault, "a.typ", "= A");
        ok(&run(data, &["sync", "link", "--vault", vault], ""));
    }
    let o = run(data, &["sync", "now"], "");
    assert_eq!(
        ok(&o),
        "one: uploaded 0, downloaded 0, removed 0 local / 0 on the server\ntwo: uploaded 0, downloaded 0, removed 0 local / 0 on the server\n"
    );

    // One vault folder is gone: that one fails, the other is still synced.
    std::fs::remove_dir_all(data.join("vaults/one")).unwrap();
    write(data, "two", "b.typ", "= B");
    let o = run(data, &["sync", "now"], "");
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("one: failed: the folder of vault \"one\" is missing"), "{}", err(&o));
    assert!(out(&o).contains("two: uploaded 1"), "{}", out(&o));
}

#[test]
fn an_emptied_vault_waits_for_confirm_or_restore() {
    let data = tempfile::tempdir().unwrap();
    let data = data.path();
    login(data, "boris");
    for i in 0..12 {
        write(data, "wipe", &format!("n{i}.typ"), &format!("= {i}\n"));
    }
    ok(&run(data, &["sync", "link", "--vault", "wipe"], ""));
    let wipe = || {
        for i in 0..12 {
            std::fs::remove_file(data.join("vaults/wipe").join(format!("n{i}.typ"))).unwrap();
        }
    };
    let held =
        "sync of \"wipe\" is paused: 12 of 12 files are gone from this device and would be deleted on the server";

    // The folder is emptied by hand: the round stops, nothing is deleted.
    wipe();
    let o = run(data, &["sync", "now", "--vault", "wipe"], "");
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains(held) && err(&o).contains("notes sync confirm --vault \"wipe\""), "{}", err(&o));
    let text = ok(&run(data, &["sync", "status"], ""));
    assert!(text.contains("wipe: linked, PAUSED, nothing is deleted: sync of"), "{text}");
    let status: serde_json::Value = serde_json::from_str(&ok(&run(data, &["sync", "status", "--json"], ""))).unwrap();
    assert_eq!(status["vaults"][0]["state"], "held");
    assert_eq!(status["vaults"][0]["held"]["count"], 12);

    // Restore: the files come back.
    let o = run(data, &["sync", "restore", "--vault", "wipe"], "");
    assert_eq!(ok(&o).trim(), "wipe: uploaded 0, downloaded 12, removed 0 local / 0 on the server");
    assert!(data.join("vaults/wipe/n5.typ").is_file());
    let o = run(data, &["sync", "confirm", "--vault", "wipe"], "");
    assert!(err(&o).contains("nothing of \"wipe\" waits for a confirmation"), "{}", err(&o));

    // Emptied again, and this time confirmed.
    wipe();
    assert_eq!(run(data, &["sync", "now"], "").status.code(), Some(1));
    let o = run(data, &["sync", "confirm", "--vault", "wipe"], "");
    assert_eq!(ok(&o).trim(), "wipe: uploaded 0, downloaded 0, removed 0 local / 12 on the server");
    assert_eq!(
        ok(&run(data, &["sync", "now"], "")).trim(),
        "wipe: uploaded 0, downloaded 0, removed 0 local / 0 on the server"
    );
}
