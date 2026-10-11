//! A hub on `127.0.0.1:0` in its own thread, for tests of everything that
//! syncs (feature `testing`, or this crate's own tests).

use std::net::SocketAddr;
use std::thread::JoinHandle;
use std::time::Duration;

use notes_hub::Hub;
use tokio::sync::oneshot;

/// The password of the accounts [`TestHub::start`] makes.
pub const PASSWORD: &str = "correct horse";

/// A running hub with its own data directory; stopped on drop.
#[derive(Debug)]
pub struct TestHub {
    pub url: String,
    pub hub: Hub,
    pub dir: tempfile::TempDir,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl TestHub {
    /// Accounts `logins` with the password [`PASSWORD`] (Argon2 is slow in a
    /// debug build: few accounts per test file).
    ///
    /// # Panics
    /// The hub does not start.
    #[allow(clippy::unwrap_used, clippy::expect_used, reason = "a test helper fails the test by panicking")]
    pub fn start(logins: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::open(dir.path(), Duration::from_secs(3600)).unwrap();
        for login in logins {
            hub.auth().accounts().add(login, PASSWORD).unwrap();
        }
        let (stop, stopped) = oneshot::channel::<()>();
        let (addr_tx, addr_rx) = std::sync::mpsc::channel::<SocketAddr>();
        let served = hub.clone();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                addr_tx.send(listener.local_addr().unwrap()).unwrap();
                notes_hub::serve(listener, served, async move {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
            });
        });
        let addr = addr_rx.recv().expect("the hub did not start");
        Self { url: format!("http://{addr}"), hub, dir, stop: Some(stop), thread: Some(thread) }
    }

    /// Stops the server (the address then refuses connections).
    pub fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        self.stop();
    }
}
