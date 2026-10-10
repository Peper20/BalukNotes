//! The `notes-hub` binary: accounts and the server process.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use notes_store::accounts::Accounts;
use notes_store::config::{hub_dir, sessions_file, users_file};
use notes_store::sessions::{self, Sessions};

fn command(data: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_notes-hub"));
    command.arg("--data").arg(data).env_remove("NOTES_DATA").env_remove(notes::VERSION_ENV).env("RUST_LOG", "info");
    command
}

/// Runs the command with `stdin`; the exit success, stdout and stderr.
fn run(mut command: Command, stdin: &str) -> (bool, String, String) {
    let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into(), String::from_utf8_lossy(&out.stderr).into())
}

fn users(data: &Path, args: &[&str], stdin: &str) -> (bool, String, String) {
    let mut command = command(data);
    command.arg("users").args(args);
    run(command, stdin)
}

#[test]
fn users_commands() {
    let data = tempfile::tempdir().unwrap();
    let data = data.path();
    assert_eq!(users(data, &["list"], ""), (true, String::new(), String::new()));

    let (ok, out, err) = users(data, &["add", "Alice", "--password-stdin"], "correct horse\n");
    assert!(ok, "{err}");
    assert_eq!(out, "user \"alice\" added\n");
    // `--data` goes before or after the command.
    let mut after = Command::new(env!("CARGO_BIN_EXE_notes-hub"));
    after.args(["users", "add", "bob", "--password-stdin", "--data"]).arg(data).env_remove("NOTES_DATA");
    let (ok, out, err) = run(after, "second secret\r\n");
    assert!(ok, "{err}");
    assert_eq!(out, "user \"bob\" added\n");
    assert_eq!(users(data, &["list"], "").1, "alice\nbob\n");
    let accounts = Accounts::open(users_file(data)).unwrap();
    assert!(accounts.verify("alice", "correct horse"));
    assert!(accounts.verify("bob", "second secret"), "a CRLF line ending is not a part of the password");

    // Mistakes are one line on stderr and a failure.
    let (ok, out, err) = users(data, &["add", "alice", "--password-stdin"], "correct horse\n");
    assert!((ok, out.as_str(), err.as_str()) == (false, "", "error: user \"alice\" already exists\n"), "{err}");
    let (ok, _, err) = users(data, &["add", "carol", "--password-stdin"], "short\n");
    assert!(!ok && err == "error: password: at least 8 characters\n", "{err}");
    let (ok, _, err) = users(data, &["add", "Bad Name", "--password-stdin"], "correct horse\n");
    assert!(!ok && err.starts_with("error: invalid login"), "{err}");
    let (ok, _, err) = users(data, &["passwd", "zed", "--password-stdin"], "correct horse\n");
    assert!(!ok && err == "error: no user \"zed\"\n", "{err}");
    let (ok, _, err) = users(data, &["remove", "zed"], "");
    assert!(!ok && err == "error: no user \"zed\"\n", "{err}");
    assert_eq!(users(data, &["list"], "").1, "alice\nbob\n", "nothing changed");
}

#[test]
fn passwd_and_remove_end_the_sessions() {
    let data = tempfile::tempdir().unwrap();
    let data = data.path();
    users(data, &["add", "alice", "--password-stdin"], "correct horse\n");
    users(data, &["add", "bob", "--password-stdin"], "second secret\n");
    let sessions = Sessions::open(sessions_file(data), Duration::from_secs(3600)).unwrap();
    let now = sessions::now();
    let (a1, a2, b) = (
        sessions.create("alice", now).unwrap(),
        sessions.create("alice", now).unwrap(),
        sessions.create("bob", now).unwrap(),
    );

    let (ok, out, err) = users(data, &["passwd", "Alice", "--password-stdin"], "a new password\n");
    assert!(ok, "{err}");
    assert_eq!(out, "password of \"alice\" changed; sessions ended: 2\n");
    let accounts = Accounts::open(users_file(data)).unwrap();
    assert!(accounts.verify("alice", "a new password") && !accounts.verify("alice", "correct horse"));
    let reopened = Sessions::open(sessions_file(data), Duration::from_secs(3600)).unwrap();
    assert!(reopened.check(&a1, now).is_none() && reopened.check(&a2, now).is_none());
    assert!(reopened.check(&b, now).is_some(), "other accounts stay signed in");

    std::fs::create_dir_all(hub_dir(data).join("bob/notes")).unwrap();
    let (ok, out, err) = users(data, &["remove", "bob"], "");
    assert!(ok, "{err}");
    assert_eq!(
        out,
        format!("user \"bob\" removed; sessions ended: 1; vaults kept in {}\n", hub_dir(data).join("bob").display())
    );
    assert!(hub_dir(data).join("bob/notes").is_dir(), "the vaults are not deleted");
    assert_eq!(users(data, &["list"], "").1, "alice\n");
    let (_, out, _) = users(data, &["remove", "alice"], "");
    assert_eq!(out, "user \"alice\" removed; sessions ended: 0\n");
}

#[test]
fn serve_needs_users() {
    let data = tempfile::tempdir().unwrap();
    let mut serve = command(data.path());
    serve.args(["hub", "serve", "--addr", "127.0.0.1:0"]);
    let (ok, out, err) = run(serve, "");
    assert!(!ok);
    assert_eq!(out, "");
    assert!(err.contains("error: no users: create one: notes users add <login>"), "{err}");
}

/// A free local port (found by binding and releasing it).
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// The answer of `GET path` to a server on `port`, as text; none while it is down.
fn get(port: u16, path: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").ok()?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer).ok()?;
    Some(answer)
}

#[test]
fn serve_runs_and_stops_on_sigterm() {
    let data = tempfile::tempdir().unwrap();
    users(data.path(), &["add", "alice", "--password-stdin"], "correct horse\n");
    let port = free_port();
    let mut serve = command(data.path());
    // Not localhost: the warning about HTTPS comes up (and the test still binds to the port).
    serve.args(["hub", "serve", "--addr", &format!("0.0.0.0:{port}"), "--session-days", "7"]);
    let mut child = serve.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
    let started = Instant::now();
    let answer = loop {
        if let Some(answer) = get(port, "/api/session") {
            break answer;
        }
        assert!(started.elapsed() < Duration::from_secs(20), "the server did not start");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(answer.starts_with("HTTP/1.1 401"), "{answer}");
    assert!(answer.contains("\"error\":\"sign-in required\"") && answer.contains("\"errors\":[]"), "{answer}");

    let killed = Command::new("kill").args(["-TERM", &child.id().to_string()]).status().unwrap();
    assert!(killed.success());
    let status = child.wait().unwrap();
    assert!(status.success(), "a stop by SIGTERM is a clean exit: {status:?}");
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert!(err.contains("warning: 0.0.0.0:") && err.contains("reverse proxy"), "{err}");
    assert!(err.contains("stopped"), "{err}");
}
