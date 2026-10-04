//! The core for the window: `notes-typst serve --socket` (default socket:
//! `notes::default_socket`). Something answers on the socket (the service,
//! another window): connect to it; otherwise start our own core from the parts
//! directory and wait for its socket.

use std::ffi::OsString;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// How long to wait for our own core's socket.
const START_WAIT: Duration = Duration::from_secs(30);
/// How long to wait for our own core to stop cleanly before killing it.
const STOP_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct Core {
    socket: PathBuf,
    /// Our own core; `None`: someone else's (the service), which we do not stop.
    child: Option<Child>,
}

impl Core {
    /// `args`: the core's global flags (`--data`, `--vault`, ...) for our own core.
    pub fn start(args: &[OsString]) -> Result<Self, String> {
        let socket = notes::default_socket().ok_or("no $XDG_RUNTIME_DIR")?;
        if UnixStream::connect(&socket).is_ok() {
            tracing::info!("core already running: {}", socket.display());
            return Ok(Self { socket, child: None });
        }
        let dir = notes::parts_dir().map_err(|e| format!("parts directory: {e}"))?;
        let bin = notes::TYPST.path(&dir);
        if !bin.is_file() {
            return Err(format!("no notes-typst part ({})", bin.display()));
        }
        let mut child = Command::new(&bin)
            .args(args)
            .arg("serve")
            .arg("--socket")
            .arg(&socket)
            .env(notes::VERSION_ENV, notes::VERSION)
            .spawn()
            .map_err(|e| format!("starting {}: {e}", bin.display()))?;
        let started = Instant::now();
        while UnixStream::connect(&socket).is_err() {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!("{} exited: {status}", bin.display()));
            }
            if started.elapsed() > START_WAIT {
                let _ = child.kill();
                return Err(format!(
                    "{} did not open the socket {} within {START_WAIT:?}",
                    bin.display(),
                    socket.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        tracing::info!("core started in {:?}: {}", started.elapsed(), socket.display());
        Ok(Self { socket, child: Some(child) })
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for Core {
    /// Stops our own core cleanly (SIGTERM: waiting requests get an answer,
    /// the cache is written) or kills it if it takes too long.
    fn drop(&mut self) {
        let Some(child) = &mut self.child else { return };
        let _ = Command::new("kill").args(["-TERM", &child.id().to_string()]).status();
        let started = Instant::now();
        while started.elapsed() < STOP_WAIT {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
