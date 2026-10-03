//! Ядро для окна: `notes-typst serve --socket` (сокет по умолчанию -
//! `notes::default_socket`). Отвечает на сокете (служба, другое окно) -
//! подключиться; нет - запустить своё из папки частей и дождаться сокета.

use std::ffi::OsString;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Сколько ждать сокета своего ядра.
const START_WAIT: Duration = Duration::from_secs(30);
/// Сколько ждать штатной остановки своего ядра, прежде чем убить.
const STOP_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct Core {
    socket: PathBuf,
    /// Своё ядро; `None` - чужое (служба), его не останавливаем.
    child: Option<Child>,
}

impl Core {
    /// `args` - общие флаги ядра (`--data`, `--vault`, ...): своему ядру.
    pub fn start(args: &[OsString]) -> Result<Self, String> {
        let socket = notes::default_socket().ok_or("нет $XDG_RUNTIME_DIR")?;
        if UnixStream::connect(&socket).is_ok() {
            tracing::info!("ядро уже работает: {}", socket.display());
            return Ok(Self { socket, child: None });
        }
        let dir = notes::parts_dir().map_err(|e| format!("папка частей: {e}"))?;
        let bin = notes::TYPST.path(&dir);
        if !bin.is_file() {
            return Err(format!("нет части notes-typst ({})", bin.display()));
        }
        let mut child = Command::new(&bin)
            .args(args)
            .arg("serve")
            .arg("--socket")
            .arg(&socket)
            .env(notes::VERSION_ENV, notes::VERSION)
            .spawn()
            .map_err(|e| format!("запуск {}: {e}", bin.display()))?;
        let started = Instant::now();
        while UnixStream::connect(&socket).is_err() {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!("{} завершился: {status}", bin.display()));
            }
            if started.elapsed() > START_WAIT {
                let _ = child.kill();
                return Err(format!("{} не открыл сокет {} за {START_WAIT:?}", bin.display(), socket.display()));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        tracing::info!("ядро запущено за {:?}: {}", started.elapsed(), socket.display());
        Ok(Self { socket, child: Some(child) })
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for Core {
    /// Своё ядро - штатно (SIGTERM: ждущие запросы получают ответ, кэш
    /// дописан), не успело - убить.
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
