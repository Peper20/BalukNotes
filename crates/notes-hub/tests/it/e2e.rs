//! A real listening server and two devices syncing through `HttpRemote`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notes_hub::Hub;
use notes_hub::client::{self, HttpRemote};
use notes_store::sync::{DirTree, Error, Prefer, Remote as _, State, sync};
use tokio::sync::oneshot;

use crate::common::PASSWORD;

/// The hub on `127.0.0.1:0` in its own thread with its own runtime; stopped on drop.
struct Server {
    url: String,
    hub: Hub,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    fn start(data: &Path, logins: &[&str]) -> Self {
        let hub = Hub::open(data, Duration::from_secs(3600)).unwrap();
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
        let addr = addr_rx.recv().unwrap();
        Self { url: format!("http://{addr}"), hub, stop: Some(stop), thread: Some(thread) }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A device: a vault folder, a place for what sync removes, the sync state.
struct Device {
    root: PathBuf,
    tree: DirTree,
    state: State,
    _dir: tempfile::TempDir,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("vault");
        std::fs::create_dir_all(&root).unwrap();
        let tree = DirTree::new(&root, dir.path().join("removed"));
        let state = State::open(dir.path().join("state.json")).unwrap();
        Self { root, tree, state, _dir: dir }
    }

    fn write(&self, path: &str, text: &str) {
        let abs = self.root.join(path);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(abs, text).unwrap();
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }

    fn sync(&mut self, remote: &HttpRemote, prefer: Prefer) -> notes_store::sync::Report {
        sync(&self.tree, &mut self.state, remote, prefer).unwrap()
    }
}

#[test]
fn two_devices_sync_through_the_server() {
    let data = tempfile::tempdir().unwrap();
    let server = Server::start(data.path(), &["ivan"]);
    let token = client::login(&server.url, "ivan", PASSWORD).unwrap();
    assert!(client::vaults(&server.url, &token).unwrap().is_empty());
    let created = client::create_vault(&server.url, &token, "My notes").unwrap();
    assert_eq!((created.name.as_str(), created.seq), ("My notes", 0));
    assert_eq!(client::create_vault(&server.url, &token, "My notes").unwrap(), created, "twice is fine");
    let remote = HttpRemote::new(&server.url, &token, "My notes");

    // The first device writes, the second receives.
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("a.typ", "= A\n");
    a.write("Сеть/SSH #1.typ", "= SSH\n");
    let report = a.sync(&remote, Prefer::Local);
    assert_eq!(report.uploaded, 2, "{report:?}");
    let report = b.sync(&remote, Prefer::Remote);
    assert_eq!(report.downloaded, 2, "{report:?}");
    assert_eq!(b.read("Сеть/SSH #1.typ").as_deref(), Some("= SSH\n"));
    assert!(b.sync(&remote, Prefer::Remote).is_idle(), "nothing more to do");

    // An edit and a delete travel.
    a.write("a.typ", "= A, edited\n");
    std::fs::remove_file(a.root.join("Сеть/SSH #1.typ")).unwrap();
    let report = a.sync(&remote, Prefer::Local);
    assert_eq!((report.uploaded, report.removed_remote), (1, 1), "{report:?}");
    let report = b.sync(&remote, Prefer::Remote);
    assert_eq!((report.downloaded, report.removed_local), (1, 1), "{report:?}");
    assert_eq!(b.read("a.typ").as_deref(), Some("= A, edited\n"));
    assert_eq!(b.read("Сеть/SSH #1.typ"), None);

    // Both edit one file: the device that writes first (Local) wins.
    a.write("c.typ", "c from A, first\n");
    a.sync(&remote, Prefer::Local);
    b.sync(&remote, Prefer::Remote);
    a.write("c.typ", "c from A, second\n");
    b.write("c.typ", "c from B\n");
    a.sync(&remote, Prefer::Local);
    let report = b.sync(&remote, Prefer::Remote);
    assert_eq!(report.conflicts, ["c.typ"], "{report:?}");
    assert_eq!(b.read("c.typ").as_deref(), Some("c from A, second\n"));
    // ... and with Local the device's version replaces the server's.
    b.write("c.typ", "c from B, final\n");
    a.write("c.typ", "c from A, lost\n");
    b.sync(&remote, Prefer::Local);
    let report = a.sync(&remote, Prefer::Remote);
    assert_eq!(report.conflicts, ["c.typ"], "{report:?}");
    assert_eq!(a.read("c.typ").as_deref(), Some("c from B, final\n"));
    assert_eq!(std::fs::read(data.path().join("hub/ivan/My notes/files/c.typ")).unwrap(), b"c from B, final\n");

    // The listing of the account knows the vault's number.
    let listed = client::vaults(&server.url, &token).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].seq, remote.changes(0).unwrap().seq);
}

#[test]
fn long_poll_over_http() {
    let data = tempfile::tempdir().unwrap();
    let server = Server::start(data.path(), &["ivan"]);
    let token = client::login(&server.url, "ivan", PASSWORD).unwrap();
    client::create_vault(&server.url, &token, "notes").unwrap();
    let remote = HttpRemote::new(&server.url, &token, "notes");

    let started = Instant::now();
    assert!(remote.wait_changes(0, Duration::from_secs(1)).unwrap().entries.is_empty());
    assert!(started.elapsed() >= Duration::from_millis(900));

    let writer = HttpRemote::new(&server.url, &token, "notes");
    let started = Instant::now();
    let changes = std::thread::scope(|scope| {
        let waiting = scope.spawn(|| remote.wait_changes(0, Duration::from_secs(20)).unwrap());
        std::thread::sleep(Duration::from_millis(300));
        writer.put("a.typ", &notes_store::sync::Base::Absent, b"x").unwrap().unwrap();
        waiting.join().unwrap()
    });
    assert_eq!(changes.entries.len(), 1);
    assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
}

#[test]
fn refused_sessions_and_unreachable_servers() {
    let data = tempfile::tempdir().unwrap();
    let server = Server::start(data.path(), &["ivan"]);
    let token = client::login(&server.url, "ivan", PASSWORD).unwrap();
    client::create_vault(&server.url, &token, "notes").unwrap();
    let remote = HttpRemote::new(&server.url, &token, "notes");
    let mut device = Device::new();
    device.write("a.typ", "x");
    assert_eq!(device.sync(&remote, Prefer::Local).uploaded, 1);

    // A token the server does not know.
    let fake = HttpRemote::new(&server.url, "no-such-token", "notes");
    assert!(matches!(fake.changes(0), Err(Error::Unauthorized)));
    assert!(matches!(client::vaults(&server.url, "no-such-token"), Err(Error::Unauthorized)));
    // An unknown vault.
    let other = HttpRemote::new(&server.url, &token, "other");
    assert!(matches!(other.changes(0), Err(Error::NotFound(_))));
    assert!(matches!(other.get("a.typ"), Err(Error::NotFound(_))));
    assert!(matches!(remote.get("none.typ"), Err(Error::NotFound(_))));
    assert!(matches!(remote.put("../x", &notes_store::sync::Base::Any, b"x"), Err(Error::InvalidPath { .. })));

    // Ended by the owner of the server: `notes users passwd`.
    let expired = client::login(&server.url, "ivan", PASSWORD).unwrap();
    server.hub.auth().sessions().revoke(&expired).unwrap();
    let lost = HttpRemote::new(&server.url, &expired, "notes");
    device.write("a.typ", "changed");
    let result = sync(&device.tree, &mut device.state, &lost, Prefer::Local);
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");

    // Signed out.
    client::logout(&server.url, &token).unwrap();
    assert!(matches!(remote.changes(0), Err(Error::Unauthorized)));
    assert!(matches!(client::create_vault(&server.url, &token, "x"), Err(Error::Unauthorized)));

    // A wrong password (the pause after it is why this comes last).
    assert!(matches!(client::login(&server.url, "ivan", "wrong password"), Err(Error::Unauthorized)));
    assert!(matches!(client::login(&server.url, "ivan", PASSWORD), Err(Error::Other(_))), "throttled: 429");

    // Nobody listens.
    let free = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let nowhere = HttpRemote::new(&format!("http://{free}"), &token, "notes");
    assert!(matches!(nowhere.changes(0), Err(Error::Network(_))));
    assert!(matches!(client::login(&format!("http://{free}"), "ivan", PASSWORD), Err(Error::Network(_))));
}
