//! One window app at a time: a Unix socket `$XDG_RUNTIME_DIR/baluk-notes/app.sock`
//! (`notes::app_socket`). The first `notes app` binds it; a later one connects,
//! writes one JSON line `{"path": "/v/X/n/Y", "token": "<activation token>" |
//! null}`, gets the line `ok` and exits - the running app opens the path
//! (`windows::open`). A socket file nobody answers on (the app was killed) is
//! removed. No Tauri types here.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a running app may take to answer a handover. The first instance
/// answers only after its core is up (up to `core::START_WAIT`).
const ANSWER_WAIT: Duration = Duration::from_secs(40);
/// How long a client may take to send its line.
const READ_WAIT: Duration = Duration::from_secs(5);
/// The longest line accepted.
const MAX_LINE: u64 = 64 * 1024;
/// The reply to a handed-over message.
const OK: &str = "ok";

/// What a second launch hands to the running app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The client's address: `/`, `/v/<vault>/` or `/v/<vault>/n/<note>`.
    pub path: String,
    /// `XDG_ACTIVATION_TOKEN` of the second launch: with it the window may take focus on Wayland.
    pub token: Option<String>,
}

impl Message {
    fn to_line(&self) -> String {
        serde_json::json!({ "path": self.path, "token": self.token }).to_string()
    }

    fn from_line(line: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let path = value.get("path").and_then(|p| p.as_str()).ok_or("no \"path\"")?;
        let token = value.get("token").and_then(|t| t.as_str()).filter(|t| !t.is_empty());
        Ok(Self { path: path.to_owned(), token: token.map(str::to_owned) })
    }
}

/// The result of `Instance::claim`.
#[derive(Debug)]
pub enum Claim {
    /// The message went to the running app; this process has nothing to do.
    Handed,
    /// Nobody was running: this process is the app.
    Owner(Instance),
}

/// The bound socket of the running app. The socket file is removed on drop.
#[derive(Debug)]
pub struct Instance {
    listener: UnixListener,
    path: PathBuf,
}

impl Instance {
    /// Either hands `message` to the app that holds `socket`, or becomes that
    /// app. Binding comes first, so two simultaneous launches cannot both win.
    pub fn claim(socket: &Path, message: &Message) -> io::Result<Claim> {
        if let Some(dir) = socket.parent() {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        for _ in 0..3 {
            match UnixListener::bind(socket) {
                Ok(listener) => {
                    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
                    return Ok(Claim::Owner(Self { listener, path: socket.to_owned() }));
                }
                Err(e) if e.kind() == io::ErrorKind::AddrInUse => match hand_over(socket, message) {
                    Ok(()) => return Ok(Claim::Handed),
                    Err(e) if is_stale(&e) => {
                        tracing::info!("removing the stale socket {}: {e}", socket.display());
                        match std::fs::remove_file(socket) {
                            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                            _ => {}
                        }
                    }
                    Err(e) => return Err(e),
                },
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other("another launch keeps taking the socket"))
    }

    /// Starts a thread that takes messages one at a time and calls `handler`
    /// for each, after answering `ok`.
    pub fn serve(&self, handler: impl Fn(Message) + Send + 'static) -> io::Result<()> {
        let listener = self.listener.try_clone()?;
        std::thread::Builder::new().name("instance".into()).spawn(move || {
            for stream in listener.incoming() {
                match stream.and_then(read_message) {
                    Ok((mut stream, message)) => {
                        if let Err(e) = writeln!(stream, "{OK}") {
                            tracing::warn!("no answer to the second launch: {e}");
                        }
                        handler(message);
                    }
                    Err(e) => tracing::warn!("bad message from a second launch: {e}"),
                }
            }
        })?;
        Ok(())
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Nobody answers on this socket file.
fn is_stale(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound)
}

fn read_message(stream: UnixStream) -> io::Result<(UnixStream, Message)> {
    stream.set_read_timeout(Some(READ_WAIT))?;
    let mut line = String::new();
    BufReader::new((&stream).take(MAX_LINE)).read_line(&mut line)?;
    let message = Message::from_line(line.trim()).map_err(io::Error::other)?;
    Ok((stream, message))
}

/// Sends `message` to the app on `socket` and waits for its `ok`. Errors of
/// kind `ConnectionRefused`/`NotFound` mean nobody runs there.
fn hand_over(socket: &Path, message: &Message) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(ANSWER_WAIT))?;
    stream.set_write_timeout(Some(READ_WAIT))?;
    writeln!(stream, "{}", message.to_line())?;
    let mut answer = String::new();
    BufReader::new(&stream).read_line(&mut answer)?;
    if answer.trim() == OK {
        Ok(())
    } else {
        Err(io::Error::other(format!("the running app answered \"{}\"", answer.trim())))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn message(path: &str, token: Option<&str>) -> Message {
        Message { path: path.into(), token: token.map(str::to_owned) }
    }

    fn owner(claim: io::Result<Claim>) -> Instance {
        match claim.unwrap() {
            Claim::Owner(instance) => instance,
            Claim::Handed => panic!("expected to become the owner"),
        }
    }

    #[test]
    fn message_line_round_trip() {
        for m in [message("/v/Конспекты/n/C++ и C#", Some("tok en")), message("/", None)] {
            assert_eq!(Message::from_line(&m.to_line()), Ok(m));
        }
        assert_eq!(Message::from_line(r#"{"path": "/", "token": ""}"#), Ok(message("/", None)));
        assert!(Message::from_line("{}").is_err());
        assert!(Message::from_line("not json").is_err());
    }

    #[test]
    fn second_launch_hands_over_and_first_receives() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("run").join("app.sock");
        let first = owner(Instance::claim(&socket, &message("/", None)));
        assert_eq!(std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(socket.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        let (tx, rx) = mpsc::channel();
        first.serve(move |m| tx.send(m).unwrap()).unwrap();
        // Messages sent before `serve` would have waited in the queue; here: two in a row.
        for m in [message("/v/A/n/First", Some("tok")), message("/v/B/", None)] {
            assert!(matches!(Instance::claim(&socket, &m).unwrap(), Claim::Handed));
            assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), m);
        }
        drop(first);
        assert!(!socket.exists(), "the socket is removed on exit");
    }

    #[test]
    fn message_sent_before_serve_waits_in_the_queue() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("app.sock");
        let first = owner(Instance::claim(&socket, &message("/", None)));
        let sender = {
            let socket = socket.clone();
            std::thread::spawn(move || {
                Instance::claim(&socket, &message("/v/A/", None)).map(|c| matches!(c, Claim::Handed))
            })
        };
        std::thread::sleep(Duration::from_millis(100));
        let (tx, rx) = mpsc::channel();
        first.serve(move |m| tx.send(m).unwrap()).unwrap();
        assert!(sender.join().unwrap().unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().path, "/v/A/");
    }

    #[test]
    fn stale_socket_is_removed_and_taken() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("app.sock");
        // A socket file left by a killed app: bound, then nobody listens.
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        assert!(is_stale(&hand_over(&socket, &message("/", None)).unwrap_err()));
        let instance = owner(Instance::claim(&socket, &message("/", None)));
        let (tx, rx) = mpsc::channel();
        instance.serve(move |m| tx.send(m).unwrap()).unwrap();
        assert!(matches!(Instance::claim(&socket, &message("/v/A/", None)).unwrap(), Claim::Handed));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().path, "/v/A/");
    }

    #[test]
    fn bad_line_does_not_stop_the_server() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("app.sock");
        let instance = owner(Instance::claim(&socket, &message("/", None)));
        let (tx, rx) = mpsc::channel();
        instance.serve(move |m| tx.send(m).unwrap()).unwrap();
        let mut stream = UnixStream::connect(&socket).unwrap();
        writeln!(stream, "garbage").unwrap();
        drop(stream);
        assert!(matches!(Instance::claim(&socket, &message("/v/A/", None)).unwrap(), Claim::Handed));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().path, "/v/A/");
    }
}
