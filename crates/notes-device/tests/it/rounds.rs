//! Rounds, linking, sign-in and status through the library functions.

use std::time::Duration;

use notes_device::round::Lock;
use notes_device::{Error, LastRound, Prefer, WorkState, confirm, link, logout, restore, status, sync_linked, unlink};

use crate::common::{Device, HUB};

#[test]
fn login_saves_a_private_account_and_logout_ends_the_session() {
    let a = Device::new();
    let account = a.login("ivan");
    assert_eq!((account.server.as_str(), account.login.as_str()), (HUB.url.as_str(), "ivan"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(a.paths.account_file()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "the token is a secret");
    }
    let token = a.token();
    assert!(notes_hub::client::vaults(&HUB.url, &token).is_ok());

    // A linked vault stays linked after the sign-out.
    a.write("out-vault", "a.typ", "= A");
    link(&a.paths, &a.vault("out-vault"), Prefer::Local).unwrap();
    let out = logout(&a.paths).unwrap().unwrap();
    assert_eq!(out.account.login, "ivan");
    assert!(out.server_error.is_none());
    assert!(!a.paths.account_file().exists());
    assert!(a.paths.is_linked(&a.vault("out-vault")));
    assert!(matches!(notes_hub::client::vaults(&HUB.url, &token), Err(notes_store::sync::Error::Unauthorized)));
    let error = sync_linked(&a.paths, &a.vault("out-vault"), Prefer::Local).unwrap_err();
    assert_eq!(error.to_string(), "not signed in: notes sync login <server> --login <name>");
    assert!(logout(&a.paths).unwrap().is_none(), "twice is fine");
}

#[test]
fn wrong_password_is_a_clear_error() {
    let a = Device::new();
    let server = notes_device::parse_server(&HUB.url).unwrap();
    let error = notes_device::login(&a.paths, &server, "pavel", "no such password").unwrap_err();
    assert!(matches!(error, Error::WrongLogin), "{error}");
    assert!(!a.paths.account_file().exists());
    let nowhere = notes_device::parse_server("http://127.0.0.1:1").unwrap();
    let error = notes_device::login(&a.paths, &nowhere, "pavel", "x").unwrap_err();
    assert!(error.is_offline(), "{error}");
    assert!(error.to_string().starts_with("cannot reach the server http://127.0.0.1:1: "), "{error}");
}

#[test]
fn two_devices_follow_each_other() {
    let (a, b) = (Device::new(), Device::new());
    a.login("vera");
    b.login("vera");
    let vault = a.vault("follow");

    // The first device has the vault: it is created on the server and uploaded.
    a.write("follow", "a.typ", "= A\n");
    a.write("follow", "Сеть/ssh.typ", "= SSH\n");
    let round = link(&a.paths, &vault, Prefer::Local).unwrap();
    assert_eq!((round.report.uploaded, round.report.downloaded), (2, 0), "{:?}", round.report);
    assert!(a.paths.is_linked(&vault));
    let last = LastRound::read(&a.paths, &vault).unwrap();
    assert!(last.error.is_none() && last.report.is_some());

    // The second has only the server's: the folder is made, everything arrives.
    assert!(!b.folder("follow").exists());
    let round = link(&b.paths, &vault, Prefer::Remote).unwrap();
    assert_eq!(round.report.downloaded, 2, "{:?}", round.report);
    assert_eq!(b.read("follow", "Сеть/ssh.typ").as_deref(), Some("= SSH\n"));
    assert_eq!(
        sync_linked(&b.paths, &vault, Prefer::Remote).unwrap().report.to_string(),
        "uploaded 0, downloaded 0, removed 0 local / 0 on the server"
    );

    // An edit and a delete travel.
    a.write("follow", "a.typ", "= A, edited\n");
    std::fs::remove_file(a.folder("follow").join("Сеть/ssh.typ")).unwrap();
    let round = sync_linked(&a.paths, &vault, Prefer::Local).unwrap();
    assert_eq!((round.report.uploaded, round.report.removed_remote), (1, 1));
    let round = sync_linked(&b.paths, &vault, Prefer::Remote).unwrap();
    assert_eq!((round.report.downloaded, round.report.removed_local), (1, 1));
    assert_eq!(b.read("follow", "a.typ").as_deref(), Some("= A, edited\n"));
    assert_eq!(b.read("follow", "Сеть/ssh.typ"), None);
    // What sync replaced is kept aside, not deleted.
    assert!(b.paths.removed_dir(&vault).is_dir());
}

#[test]
fn conflicts_follow_the_preference() {
    let (a, b) = (Device::new(), Device::new());
    a.login("gleb");
    b.login("gleb");
    let vault = a.vault("conflict");
    a.write("conflict", "c.typ", "c0");
    link(&a.paths, &vault, Prefer::Local).unwrap();
    link(&b.paths, &vault, Prefer::Remote).unwrap();

    // The server's wins on this device.
    a.write("conflict", "c.typ", "c from A, one");
    b.write("conflict", "c.typ", "c from B");
    sync_linked(&a.paths, &vault, Prefer::Local).unwrap();
    let round = sync_linked(&b.paths, &vault, Prefer::Remote).unwrap();
    assert_eq!(round.report.conflicts, ["c.typ"]);
    assert_eq!(b.read("conflict", "c.typ").as_deref(), Some("c from A, one"));

    // This device's wins on the server.
    a.write("conflict", "c.typ", "c from A, the second version");
    b.write("conflict", "c.typ", "B");
    sync_linked(&a.paths, &vault, Prefer::Local).unwrap();
    let round = sync_linked(&b.paths, &vault, Prefer::Local).unwrap();
    assert_eq!(round.report.conflicts, ["c.typ"]);
    assert_eq!(b.read("conflict", "c.typ").as_deref(), Some("B"));
    sync_linked(&a.paths, &vault, Prefer::Remote).unwrap();
    assert_eq!(a.read("conflict", "c.typ").as_deref(), Some("B"));
}

#[test]
fn link_cases_and_unlink() {
    let (a, b) = (Device::new(), Device::new());
    a.login("mark");
    b.login("mark");

    // Nowhere.
    let error = link(&a.paths, &a.vault("ghost"), Prefer::Local).unwrap_err();
    assert_eq!(error.to_string(), "vault \"ghost\" exists neither on this device nor on the server");
    assert!(!a.folder("ghost").exists());

    // On both sides with different files: merged.
    a.write("merge", "from-a.typ", "= A");
    b.write("merge", "from-b.typ", "= B");
    link(&a.paths, &a.vault("merge"), Prefer::Local).unwrap();
    link(&b.paths, &b.vault("merge"), Prefer::Remote).unwrap();
    sync_linked(&a.paths, &a.vault("merge"), Prefer::Local).unwrap();
    for device in [&a, &b] {
        assert_eq!(device.read("merge", "from-a.typ").as_deref(), Some("= A"));
        assert_eq!(device.read("merge", "from-b.typ").as_deref(), Some("= B"));
    }

    // Unlink forgets the link and keeps the files on both sides.
    let vault = a.vault("merge");
    assert!(unlink(&a.paths, &vault).unwrap());
    assert!(!unlink(&a.paths, &vault).unwrap());
    assert!(!a.paths.is_linked(&vault) && LastRound::read(&a.paths, &vault).is_none());
    assert!(a.read("merge", "from-b.typ").is_some());
    let error = sync_linked(&a.paths, &vault, Prefer::Local).unwrap_err();
    assert_eq!(error.to_string(), "vault \"merge\" is not linked: notes sync link --vault \"merge\"");
    assert!(!a.paths.state_file(&vault).exists(), "a refused round does not link again");
    let account = notes_device::Account::require(&a.paths).unwrap();
    assert!(notes_hub::client::vaults(&account.server, &a.token()).unwrap().iter().any(|v| v.name == "merge"));
}

#[test]
fn a_dead_session_asks_to_sign_in_again() {
    let a = Device::new();
    a.login("anna");
    a.write("dead", "a.typ", "= A");
    let vault = a.vault("dead");
    link(&a.paths, &vault, Prefer::Local).unwrap();

    HUB.hub.auth().sessions().revoke_user("anna").unwrap();
    a.write("dead", "b.typ", "= B");
    let error = sync_linked(&a.paths, &vault, Prefer::Local).unwrap_err();
    assert!(error.needs_sign_in());
    assert_eq!(
        error.to_string(),
        format!("the session ended: sign in again: notes sync login {} --login anna", HUB.url)
    );
    let last = LastRound::read(&a.paths, &vault).unwrap();
    assert_eq!(last.error.as_deref(), Some(error.to_string().as_str()));

    let now = status(&a.paths, |_| None, true).unwrap();
    assert!(now.signed_in && now.session_ended, "{now:?}");
    assert!(now.server_error.as_deref().is_some_and(|e| e.starts_with("the session ended")));
    assert_eq!(now.vaults[0].remote, None, "unknown");
    assert_eq!(now.vaults[0].state, WorkState::Error);

    // Signing in again: the same vault, the edit is pushed.
    a.login("anna");
    let round = sync_linked(&a.paths, &vault, Prefer::Local).unwrap();
    assert_eq!(round.report.uploaded, 1);
}

#[test]
fn status_lists_every_vault() {
    let a = Device::new();
    let b = Device::new();
    let empty = status(&a.paths, |_| None, true).unwrap();
    assert!(!empty.signed_in && empty.vaults.is_empty() && empty.server.is_none());

    a.login("lena");
    b.login("lena");
    // On the server only (made by the other device), here only, and linked.
    b.write("far", "a.typ", "= far");
    link(&b.paths, &b.vault("far"), Prefer::Local).unwrap();
    a.write("near", "a.typ", "= near");
    a.write("both", "a.typ", "= both");
    link(&a.paths, &a.vault("both"), Prefer::Local).unwrap();

    let now = status(&a.paths, |_| None, true).unwrap();
    assert_eq!(
        (now.server.as_deref(), now.login.as_deref(), now.signed_in),
        (Some(HUB.url.as_str()), Some("lena"), true)
    );
    let row = |name: &str| now.vaults.iter().find(|v| v.name == name).unwrap().clone();
    assert_eq!(now.vaults.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["both", "far", "near"]);
    let both = row("both");
    assert_eq!((both.local, both.remote, both.linked), (true, Some(true), true));
    assert!(both.last_sync.is_some() && both.report.as_ref().is_some_and(|r| r.uploaded == 1), "{both:?}");
    let far = row("far");
    assert_eq!((far.local, far.remote, far.linked), (false, Some(true), false));
    assert!(far.last_sync.is_none() && far.error.is_none());
    let near = row("near");
    assert_eq!((near.local, near.remote, near.linked), (true, Some(false), false));

    // Without asking the server the remote column is unknown.
    let offline = status(&a.paths, |_| None, false).unwrap();
    assert!(offline.vaults.iter().all(|v| v.remote.is_none()) && offline.server_error.is_none());
    let json = serde_json::to_value(&now).unwrap();
    assert_eq!(json["vaults"][0]["state"], "idle");
}

#[test]
fn a_running_round_makes_the_next_one_wait() {
    let a = Device::new();
    a.login("kira");
    a.write("busy", "a.typ", "= A");
    let vault = a.vault("busy");
    link(&a.paths, &vault, Prefer::Local).unwrap();
    a.write("busy", "b.typ", "= B");

    let lock = Lock::acquire(&a.paths, &vault, Duration::ZERO).unwrap();
    let round = std::thread::scope(|scope| {
        let waiting = scope.spawn(|| sync_linked(&a.paths, &vault, Prefer::Local));
        std::thread::sleep(Duration::from_millis(300));
        assert!(!waiting.is_finished(), "waits for the lock");
        drop(lock);
        waiting.join().unwrap()
    });
    assert_eq!(round.unwrap().report.uploaded, 1);
}

#[test]
fn a_mass_deletion_waits_for_confirm_or_restore() {
    let (a, b) = (Device::new(), Device::new());
    a.sign_in_directly("emil");
    b.sign_in_directly("emil");
    let vault = a.vault("guarded");
    for i in 0..12 {
        a.write("guarded", &format!("n{i}.typ"), &format!("= {i}"));
    }
    link(&a.paths, &vault, Prefer::Local).unwrap();
    link(&b.paths, &vault, Prefer::Remote).unwrap();

    // A few notes go as usual.
    std::fs::remove_file(a.folder("guarded").join("n0.typ")).unwrap();
    assert_eq!(sync_linked(&a.paths, &vault, Prefer::Local).unwrap().report.removed_remote, 1);

    // The rest vanishes: the round stops, says so, and the server keeps everything.
    for i in 1..12 {
        std::fs::remove_file(a.folder("guarded").join(format!("n{i}.typ"))).unwrap();
    }
    let error = sync_linked(&a.paths, &vault, Prefer::Local).unwrap_err();
    assert!(error.is_held());
    assert_eq!(
        error.to_string(),
        "sync of \"guarded\" is paused: 11 of 11 files are gone from this device and would be deleted on the \
         server; to delete them there: notes sync confirm --vault \"guarded\"; to get them back from the server \
         instead: notes sync restore --vault \"guarded\""
    );
    let last = LastRound::read(&a.paths, &vault).unwrap();
    assert_eq!(last.held.as_ref().map(|h| h.count), Some(11));
    assert_eq!(status(&a.paths, |_| None, false).unwrap().vaults[0].state, WorkState::Held);
    assert_eq!(sync_linked(&b.paths, &vault, Prefer::Remote).unwrap().report.removed_local, 1);
    assert_eq!(b.read("guarded", "n5.typ").as_deref(), Some("= 5"), "nothing was deleted on the server");

    // Restore: the files come back and nothing waits any more.
    let round = restore(&a.paths, &vault, Prefer::Local).unwrap();
    assert_eq!((round.report.downloaded, round.report.removed_remote), (11, 0));
    assert_eq!(a.read("guarded", "n5.typ").as_deref(), Some("= 5"));
    assert!(LastRound::read(&a.paths, &vault).unwrap().held.is_none());
    assert_eq!(
        confirm(&a.paths, &vault, Prefer::Local).unwrap_err().to_string(),
        "nothing of \"guarded\" waits for a confirmation"
    );

    // The same again, and this time the user confirms.
    for i in 1..12 {
        std::fs::remove_file(a.folder("guarded").join(format!("n{i}.typ"))).unwrap();
    }
    assert!(sync_linked(&a.paths, &vault, Prefer::Local).unwrap_err().is_held());
    let round = confirm(&a.paths, &vault, Prefer::Local).unwrap();
    assert_eq!(round.report.removed_remote, 11);
    assert!(LastRound::read(&a.paths, &vault).unwrap().held.is_none());
    // The mirror: the other device is asked before it loses its notes.
    let error = sync_linked(&b.paths, &vault, Prefer::Remote).unwrap_err();
    assert!(
        error.to_string().contains("11 of 11 files are gone from the server and would be removed from this device"),
        "{error}"
    );
    assert_eq!(b.read("guarded", "n5.typ").as_deref(), Some("= 5"));
    confirm(&b.paths, &vault, Prefer::Remote).unwrap();
    assert_eq!(b.read("guarded", "n5.typ"), None);
}
