//! The background worker: local edits are pushed and remote ones pulled with
//! nobody asking, errors are survived, a stop is prompt.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use notes_core::storage::DirStorage;
use notes_core::watch::Changes;
use notes_device::testing::{PASSWORD, TestHub};
use notes_device::{DeviceSync, Prefer, Timing, WorkState, link, sync_linked};

use crate::common::{Device, HUB, wait_for};

const LONG: Duration = Duration::from_secs(15);

fn fast() -> Timing {
    Timing {
        debounce: Duration::from_millis(100),
        held_recheck: Duration::from_millis(300),
        poll_wait: Duration::from_secs(3),
        backoff_min: Duration::from_millis(100),
        backoff_max: Duration::from_millis(400),
    }
}

fn service(device: &Device, timing: Timing) -> Arc<DeviceSync> {
    DeviceSync::with_timing(device.dir.path(), || Prefer::Local, timing)
}

fn state_of(sync: &DeviceSync, vault: &str) -> (WorkState, Option<String>) {
    let status = sync.status(false).unwrap();
    let row = status.vaults.iter().find(|v| v.name == vault).unwrap();
    (row.state, row.error.clone())
}

#[test]
fn pushes_local_edits_and_pulls_remote_ones() {
    let (a, b) = (Device::new(), Device::new());
    let sync = service(&a, fast());
    sync.login(&HUB.url, "wera", PASSWORD).unwrap();
    b.login("wera");
    let vault = a.vault("live");
    a.write("live", "a.typ", "= A");
    sync.link("live").unwrap();
    link(&b.paths, &vault, Prefer::Remote).unwrap();
    wait_for("idle", LONG, || state_of(&sync, "live").0 == WorkState::Idle);

    // A new file and an edit on this device reach the server by themselves.
    a.write("live", "new.typ", "= new");
    a.write("live", "a.typ", "= A, edited a little");
    wait_for("the push", LONG, || {
        sync_linked(&b.paths, &vault, Prefer::Remote).unwrap();
        b.read("live", "new.typ").is_some() && b.read("live", "a.typ").as_deref() == Some("= A, edited a little")
    });

    // And the other way round.
    b.write("live", "from-b.typ", "= from B");
    sync_linked(&b.paths, &vault, Prefer::Local).unwrap();
    wait_for("the pull", LONG, || a.read("live", "from-b.typ").is_some());
    assert_eq!(a.read("live", "from-b.typ").as_deref(), Some("= from B"));
    wait_for("idle again", LONG, || state_of(&sync, "live") == (WorkState::Idle, None));

    // Unlinking stops the worker; later edits stay here.
    assert!(sync.unlink("live").unwrap());
    a.write("live", "after.typ", "= after");
    std::thread::sleep(Duration::from_millis(600));
    sync_linked(&b.paths, &vault, Prefer::Remote).unwrap();
    assert!(b.read("live", "after.typ").is_none());
    sync.stop();
}

#[test]
fn pulled_files_reach_the_core_as_vault_changes() {
    let (a, b) = (Device::new(), Device::new());
    let sync = service(&a, fast());
    sync.login(&HUB.url, "dana", PASSWORD).unwrap();
    b.login("dana");
    let vault = a.vault("events");
    a.write("events", "a.typ", "= A");
    sync.link("events").unwrap();
    link(&b.paths, &vault, Prefer::Remote).unwrap();

    // The core's watcher over the vault, as `notes serve` has it.
    let storage = DirStorage::open(a.folder("events")).unwrap();
    let changes = Arc::new(Changes::default());
    assert!(changes.start(&storage));
    let (tx, rx) = mpsc::channel();
    changes.subscribe(move |change| {
        let _ = tx.send(change.clone());
    });

    b.write("events", "pulled.typ", "= pulled");
    sync_linked(&b.paths, &vault, Prefer::Local).unwrap();
    let started = Instant::now();
    loop {
        let left = LONG.saturating_sub(started.elapsed());
        let change = rx.recv_timeout(left).expect("a batch of changes with the pulled note");
        if change.paths.iter().any(|p| p == "pulled.typ") {
            break;
        }
    }
    assert_eq!(a.read("events", "pulled.typ").as_deref(), Some("= pulled"));
    sync.stop();
}

#[test]
fn stop_does_not_wait_for_the_long_poll() {
    let a = Device::new();
    // The default waits: a long poll of 20 s is in flight.
    let sync = service(&a, Timing { debounce: Duration::from_millis(100), ..Timing::default() });
    sync.login(&HUB.url, "nina", PASSWORD).unwrap();
    a.write("prompt", "a.typ", "= A");
    sync.link("prompt").unwrap();
    wait_for("idle", LONG, || state_of(&sync, "prompt").0 == WorkState::Idle);
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    sync.stop();
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
}

#[test]
fn a_dead_session_stops_the_worker_and_a_new_sign_in_restarts_it() {
    let (a, b) = (Device::new(), Device::new());
    let sync = service(&a, fast());
    sync.login(&HUB.url, "olga", PASSWORD).unwrap();
    a.write("session", "a.typ", "= A");
    sync.link("session").unwrap();
    wait_for("idle", LONG, || state_of(&sync, "session").0 == WorkState::Idle);

    HUB.hub.auth().sessions().revoke_user("olga").unwrap();
    a.write("session", "b.typ", "= B");
    wait_for("the sign-in state", LONG, || state_of(&sync, "session").0 == WorkState::SignIn);
    let (_, error) = state_of(&sync, "session");
    assert!(error.unwrap().starts_with("the session ended: sign in again"));

    sync.login(&HUB.url, "olga", PASSWORD).unwrap();
    b.login("olga");
    let vault = b.vault("session");
    wait_for("the edit after signing in again", LONG, || {
        link(&b.paths, &vault, Prefer::Remote).is_ok() && b.read("session", "b.typ").is_some()
    });
    wait_for("idle", LONG, || state_of(&sync, "session") == (WorkState::Idle, None));
    sync.stop();
}

#[test]
fn an_unreachable_server_is_retried_and_reported() {
    let mut hub = TestHub::start(&["olga"]);
    let a = Device::new();
    let sync = service(&a, fast());
    sync.login(&hub.url, "olga", PASSWORD).unwrap();
    a.write("offline", "a.typ", "= A");
    sync.link("offline").unwrap();
    wait_for("idle", LONG, || state_of(&sync, "offline").0 == WorkState::Idle);

    hub.stop();
    a.write("offline", "b.typ", "= B");
    wait_for("the offline state", LONG, || state_of(&sync, "offline").0 == WorkState::Offline);
    let (_, error) = state_of(&sync, "offline");
    assert!(error.unwrap().starts_with("cannot reach the server http://127.0.0.1:"));
    let started = Instant::now();
    sync.stop();
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// Twenty notes in a linked vault of a running worker, the worker idle.
fn twenty(device: &Device, sync: &DeviceSync, vault: &str) {
    for i in 0..20 {
        device.write(vault, &format!("n{i}.typ"), &format!("= {i}"));
    }
    sync.link(vault).unwrap();
    wait_for("idle", LONG, || state_of(sync, vault).0 == WorkState::Idle);
}

fn empty(device: &Device, vault: &str) {
    for i in 0..20 {
        std::fs::remove_file(device.folder(vault).join(format!("n{i}.typ"))).unwrap();
    }
}

#[test]
fn an_emptied_vault_waits_for_a_confirmation_or_a_restore() {
    let a = Device::new();
    let sync = service(&a, fast());
    a.sign_in_directly("fedor");
    twenty(&a, &sync, "emptied");

    // The folder is emptied by hand: the worker stops and says why.
    empty(&a, "emptied");
    wait_for("the held state", LONG, || state_of(&sync, "emptied").0 == WorkState::Held);
    let (_, error) = state_of(&sync, "emptied");
    let error = error.unwrap();
    assert!(error.contains("20 of 20 files are gone from this device"), "{error}");
    assert!(error.contains("notes sync confirm --vault \"emptied\""), "{error}");
    let status = sync.status(false).unwrap();
    let held = status.vaults[0].held.clone().unwrap();
    assert_eq!((held.count, held.total), (20, 20));

    // It stays held: looking again does not flip the state.
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1200) {
        assert_eq!(state_of(&sync, "emptied").0, WorkState::Held);
        std::thread::sleep(Duration::from_millis(50));
    }

    // Restored: the server still had everything, the worker is idle again.
    let round = sync.restore("emptied").unwrap();
    assert_eq!((round.report.downloaded, round.report.removed_remote), (20, 0));
    assert_eq!(a.read("emptied", "n7.typ").as_deref(), Some("= 7"));
    wait_for("idle", LONG, || state_of(&sync, "emptied") == (WorkState::Idle, None));

    // Emptied again, and now confirmed: the deletion goes through.
    empty(&a, "emptied");
    wait_for("the held state again", LONG, || state_of(&sync, "emptied").0 == WorkState::Held);
    let round = sync.confirm("emptied").unwrap();
    assert_eq!(round.report.removed_remote, 20);
    wait_for("idle", LONG, || state_of(&sync, "emptied") == (WorkState::Idle, None));
    assert_eq!(sync.confirm("emptied").unwrap_err().to_string(), "nothing of \"emptied\" waits for a confirmation");
    sync.stop();
}
