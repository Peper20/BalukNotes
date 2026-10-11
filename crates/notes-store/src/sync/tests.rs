//! Engine tests: devices against a [`HubVault`] in a temp dir.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use super::*;

type Files = BTreeMap<String, Vec<u8>>;

fn files(items: &[(&str, &str)]) -> Files {
    items.iter().map(|(path, data)| ((*path).to_owned(), data.as_bytes().to_vec())).collect()
}

fn hub() -> (tempfile::TempDir, HubVault) {
    let dir = tempfile::tempdir().unwrap();
    let hub = HubVault::open(dir.path().join("hub")).unwrap();
    (dir, hub)
}

/// What the hub holds, as a device would see it.
fn hub_files(hub: &HubVault) -> Files {
    hub.changes(0)
        .entries
        .into_iter()
        .filter(|entry| entry.hash.is_some())
        .map(|entry| {
            let data = hub.read(&entry.path).unwrap().1;
            (entry.path, data)
        })
        .collect()
}

fn hub_put(hub: &HubVault, path: &str, data: &str) {
    hub.write(path, &Base::Any, data.as_bytes()).unwrap().unwrap();
}

/// A device: a tree, its state and its side of the rule.
struct Device {
    tree: MemTree,
    state: State,
    prefer: Prefer,
}

impl Device {
    fn new(prefer: Prefer) -> Self {
        Self { tree: MemTree::new(), state: State::default(), prefer }
    }

    fn with(prefer: Prefer, items: &[(&str, &str)]) -> Self {
        let device = Self::new(prefer);
        for (path, data) in items {
            device.tree.set(path, data.as_bytes());
        }
        device
    }

    fn sync(&mut self, remote: &dyn Remote) -> Report {
        sync(&self.tree, &mut self.state, remote, self.prefer).unwrap()
    }

    fn files(&self) -> Files {
        self.tree.snapshot()
    }

    fn removed(&self) -> Vec<(String, String)> {
        self.tree.removed().into_iter().map(|(path, data)| (path, String::from_utf8(data).unwrap())).collect()
    }
}

fn pair(items: &[(&str, &str)]) -> (tempfile::TempDir, HubVault, Device, Device) {
    let (dir, hub) = hub();
    let mut a = Device::with(Prefer::Local, items);
    a.sync(&hub);
    let mut b = Device::new(Prefer::Remote);
    b.sync(&hub);
    assert_eq!(a.files(), b.files());
    (dir, hub, a, b)
}

#[test]
fn first_sync_copies_both_ways() {
    let (_dir, hub) = hub();
    hub_put(&hub, "b.typ", "same");
    hub_put(&hub, "c.typ", "C");
    let mut a = Device::with(Prefer::Local, &[("a.typ", "A"), ("b.typ", "same")]);
    let report = a.sync(&hub);
    assert_eq!((report.uploaded, report.downloaded), (1, 1));
    assert!(report.conflicts.is_empty());
    let all = files(&[("a.typ", "A"), ("b.typ", "same"), ("c.typ", "C")]);
    assert_eq!((a.files(), hub_files(&hub)), (all.clone(), all.clone()));
    assert_eq!(hub.seq(), 3);
    // An empty folder linked to a filled server downloads everything, and a
    // file missing locally before the first sync comes back.
    let mut b = Device::new(Prefer::Remote);
    let report = b.sync(&hub);
    assert_eq!((report.downloaded, report.uploaded), (3, 0));
    assert_eq!(b.files(), all);
    // Different content on the first sync goes by the rule.
    let mut local = Device::with(Prefer::Local, &[("a.typ", "mine")]);
    let report = local.sync(&hub);
    assert_eq!((report.conflicts, report.uploaded), (vec!["a.typ".to_owned()], 1));
    assert_eq!(hub_files(&hub)["a.typ"], b"mine");
    let mut remote = Device::with(Prefer::Remote, &[("a.typ", "theirs")]);
    let report = remote.sync(&hub);
    assert_eq!(report.conflicts, ["a.typ"]);
    assert_eq!(remote.files()["a.typ"], b"mine");
    assert_eq!(remote.removed(), [("a.typ".to_owned(), "theirs".to_owned())]);
}

#[test]
fn edits_and_deletes_reach_other_devices() {
    let (_dir, hub, mut a, mut b) = pair(&[("a.typ", "a1"), ("d/b.typ", "b1"), ("keep.typ", "k")]);
    a.tree.set("a.typ", b"a2");
    a.tree.delete("d/b.typ");
    a.tree.set("new/n.typ", b"n");
    let report = a.sync(&hub);
    assert_eq!((report.uploaded, report.removed_remote), (2, 1));
    let report = b.sync(&hub);
    assert_eq!((report.downloaded, report.removed_local), (2, 1));
    assert_eq!(b.files(), a.files());
    assert_eq!(hub_files(&hub), a.files());
    assert_eq!(b.removed(), [("d/b.typ".to_owned(), "b1".to_owned()), ("a.typ".to_owned(), "a1".to_owned())]);
    // The server kept the replaced content.
    assert!(hub.changes(0).entries.iter().any(|e| e.path == "d/b.typ" && e.hash.is_none()));
}

#[test]
fn nothing_to_do_reads_nothing() {
    struct Counting<'a>(&'a MemTree, AtomicUsize);
    impl Tree for Counting<'_> {
        fn list(&self) -> io::Result<Vec<FileInfo>> {
            self.0.list()
        }
        fn read(&self, path: &str) -> io::Result<Vec<u8>> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.read(path)
        }
        fn write(&self, path: &str, data: &[u8]) -> io::Result<FileInfo> {
            self.0.write(path, data)
        }
        fn remove(&self, path: &str) -> io::Result<()> {
            self.0.remove(path)
        }
        fn stat(&self, path: &str) -> io::Result<Option<FileInfo>> {
            self.0.stat(path)
        }
    }
    let (_dir, hub) = hub();
    let tree = MemTree::new();
    for i in 0..5 {
        tree.set(&format!("n{i}.typ"), b"x");
    }
    let counting = Counting(&tree, AtomicUsize::new(0));
    let mut state = State::default();
    let report = sync(&counting, &mut state, &hub, Prefer::Local).unwrap();
    assert_eq!(report.uploaded, 5);
    let reads = counting.1.load(Ordering::Relaxed);
    assert!(reads >= 5);
    for _ in 0..2 {
        let report = sync(&counting, &mut state, &hub, Prefer::Local).unwrap();
        assert_eq!(report, Report::default());
        assert!(report.is_idle());
        assert_eq!(counting.1.load(Ordering::Relaxed), reads);
    }
    // Touched without a change: read once, nothing sent, the base follows.
    tree.set("n0.typ", b"x");
    let seq = hub.seq();
    let report = sync(&counting, &mut state, &hub, Prefer::Local).unwrap();
    assert!(report.is_idle());
    assert_eq!((counting.1.load(Ordering::Relaxed), hub.seq()), (reads + 1, seq));
    sync(&counting, &mut state, &hub, Prefer::Local).unwrap();
    assert_eq!(counting.1.load(Ordering::Relaxed), reads + 1);
}

#[test]
fn both_edited_the_same_file() {
    // The computer pushes after the phone did: the computer wins.
    let (_dir, hub, mut a, mut b) = pair(&[("f.typ", "v1")]);
    b.tree.set("f.typ", b"phone");
    a.tree.set("f.typ", b"computer");
    b.prefer = Prefer::Remote;
    assert_eq!(b.sync(&hub).uploaded, 1);
    let report = a.sync(&hub);
    assert_eq!((report.conflicts, report.uploaded), (vec!["f.typ".to_owned()], 1));
    assert_eq!(hub_files(&hub)["f.typ"], b"computer");
    // The phone's version is in the server's history.
    assert_eq!(fs::read(hub.files_path().join("../history/3/f.typ")).unwrap(), b"phone");
    let report = b.sync(&hub);
    assert_eq!((report.downloaded, report.conflicts.len()), (1, 0));
    assert_eq!(b.files()["f.typ"], b"computer");

    // The computer pushes first: the phone takes the server's version and
    // keeps its own aside.
    let (_dir, hub, mut a, mut b) = pair(&[("f.typ", "v1")]);
    b.tree.set("f.typ", b"phone");
    a.tree.set("f.typ", b"computer");
    a.sync(&hub);
    let report = b.sync(&hub);
    assert_eq!((report.conflicts, report.downloaded, report.uploaded), (vec!["f.typ".to_owned()], 1, 0));
    assert_eq!(b.files()["f.typ"], b"computer");
    assert_eq!(b.removed(), [("f.typ".to_owned(), "phone".to_owned())]);
    assert!(b.sync(&hub).is_idle());

    // Edit against delete.
    let (_dir, hub, mut a, mut b) = pair(&[("f.typ", "v1"), ("g.typ", "g1")]);
    a.tree.delete("f.typ");
    a.tree.set("g.typ", b"g2");
    a.sync(&hub);
    b.tree.set("f.typ", b"f-phone");
    b.tree.delete("g.typ");
    let report = b.sync(&hub);
    assert_eq!(report.conflicts, ["f.typ", "g.typ"]);
    assert_eq!(b.files(), files(&[("g.typ", "g2")]));
    assert_eq!(b.removed(), [("f.typ".to_owned(), "f-phone".to_owned())]);
    // The same, the computer is the receiver of nothing: it wins both.
    let (_dir, hub, mut a, mut b) = pair(&[("f.typ", "v1"), ("g.typ", "g1")]);
    b.tree.delete("f.typ");
    b.tree.set("g.typ", b"g2");
    b.sync(&hub);
    a.tree.set("f.typ", b"f-comp");
    a.tree.delete("g.typ");
    let report = a.sync(&hub);
    assert_eq!((report.conflicts.len(), report.uploaded, report.removed_remote), (2, 1, 1));
    assert_eq!(hub_files(&hub), files(&[("f.typ", "f-comp")]));
}

/// A remote that fails every call after `left` calls.
struct Flaky<'a> {
    inner: &'a HubVault,
    left: Cell<usize>,
}

impl Flaky<'_> {
    fn tick(&self) -> Result<()> {
        match self.left.get() {
            0 => Err(Error::Network("down".into())),
            n => {
                self.left.set(n - 1);
                Ok(())
            }
        }
    }
}

impl Remote for Flaky<'_> {
    fn changes(&self, after: u64) -> Result<Changes> {
        self.tick()?;
        self.inner.changes(after).pipe_ok()
    }
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.tick()?;
        self.inner.get(path)
    }
    fn put(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome> {
        self.tick()?;
        self.inner.put(path, base, data)
    }
    fn delete(&self, path: &str, base: &Base) -> Result<Outcome> {
        self.tick()?;
        Remote::delete(self.inner, path, base)
    }
}

trait PipeOk: Sized {
    fn pipe_ok(self) -> Result<Self> {
        Ok(self)
    }
}
impl PipeOk for Changes {}

/// A device synced with 4 files, then changed on both sides without overlap.
fn diverged(dir: &tempfile::TempDir) -> (HubVault, Device) {
    let hub = HubVault::open(dir.path().join("hub")).unwrap();
    for i in 1..=4 {
        hub_put(&hub, &format!("d/p{i}.typ"), &format!("p{i}"));
    }
    let mut device = Device::new(Prefer::Remote);
    device.state = State::open(dir.path().join("state.json")).unwrap();
    device.sync(&hub);
    hub_put(&hub, "d/p1.typ", "p1 server");
    hub.delete("d/p2.typ", &Base::Any).unwrap().unwrap();
    hub_put(&hub, "p5.typ", "p5");
    device.tree.set("d/p3.typ", b"p3 local");
    device.tree.delete("d/p4.typ");
    device.tree.set("p6.typ", b"p6");
    (hub, device)
}

#[test]
fn a_failed_round_is_finished_by_the_next() {
    let clean_dir = tempfile::tempdir().unwrap();
    let (hub, mut device) = diverged(&clean_dir);
    device.sync(&hub);
    let expected = files(&[("d/p1.typ", "p1 server"), ("d/p3.typ", "p3 local"), ("p5.typ", "p5"), ("p6.typ", "p6")]);
    assert_eq!((device.files(), hub_files(&hub)), (expected.clone(), expected.clone()));
    let clean_state = device.state.files.clone();

    let mut failures = 0;
    // `lost_state`: the process was killed, the state of the round is gone.
    for lost_state in [false, true] {
        for allowed in 0..40 {
            let dir = tempfile::tempdir().unwrap();
            let (hub, mut device) = diverged(&dir);
            let before = (device.state.remote_seq, device.state.files.clone());
            let flaky = Flaky { inner: &hub, left: Cell::new(allowed) };
            let first = sync(&device.tree, &mut device.state, &flaky, Prefer::Remote);
            failures += usize::from(first.is_err());
            // The state on disk is what the round knew when it stopped.
            let saved = State::open(dir.path().join("state.json")).unwrap();
            assert_eq!((saved.remote_seq, &saved.files), (device.state.remote_seq, &device.state.files));
            // Nothing is lost halfway: every file is on one side at least.
            let on_server = hub_files(&hub);
            for path in expected.keys() {
                assert!(
                    device.tree.get(path).is_some() || on_server.contains_key(path),
                    "{path} lost after {allowed} calls"
                );
            }
            if lost_state {
                (device.state.remote_seq, device.state.files) = before;
            }
            device.sync(&hub);
            let case = (lost_state, allowed);
            assert_eq!((device.files(), hub_files(&hub)), (expected.clone(), expected.clone()), "{case:?}");
            assert_eq!(device.state.files, clean_state, "{case:?}");
            assert!(device.sync(&hub).is_idle());
        }
    }
    assert!(failures > 10);
}

#[test]
fn local_edit_during_a_round_is_not_clobbered() {
    /// Edits a file right after the first listing: the user typing while sync
    /// runs.
    struct Typing<'a> {
        tree: &'a MemTree,
        edits: RefCell<Vec<(&'static str, &'static str)>>,
    }
    impl Tree for Typing<'_> {
        fn list(&self) -> io::Result<Vec<FileInfo>> {
            let listed = self.tree.list();
            for (path, data) in self.edits.borrow_mut().drain(..) {
                self.tree.set(path, data.as_bytes());
            }
            listed
        }
        fn read(&self, path: &str) -> io::Result<Vec<u8>> {
            self.tree.read(path)
        }
        fn write(&self, path: &str, data: &[u8]) -> io::Result<FileInfo> {
            self.tree.write(path, data)
        }
        fn remove(&self, path: &str) -> io::Result<()> {
            self.tree.remove(path)
        }
        fn stat(&self, path: &str) -> io::Result<Option<FileInfo>> {
            self.tree.stat(path)
        }
    }

    let (_dir, hub, mut a, mut b) = pair(&[("f.typ", "f1"), ("g.typ", "g1")]);
    a.tree.set("f.typ", b"f2");
    a.tree.delete("g.typ");
    a.sync(&hub);
    let typing = Typing { tree: &b.tree, edits: RefCell::new(vec![("f.typ", "f typed"), ("g.typ", "g typed")]) };
    let report = sync(&typing, &mut b.state, &hub, Prefer::Remote).unwrap();
    assert_eq!((report.downloaded, report.removed_local), (0, 0));
    assert_eq!(report.skipped, ["g.typ", "f.typ"]);
    assert_eq!(b.files(), files(&[("f.typ", "f typed"), ("g.typ", "g typed")]));
    assert!(b.removed().is_empty());
    // The server's changes are looked at again; now it is an ordinary
    // conflict, and the typed text is kept aside.
    assert_eq!(b.state.remote_seq, 2);
    let report = b.sync(&hub);
    assert_eq!(report.conflicts, ["f.typ", "g.typ"]);
    assert_eq!(b.files(), files(&[("f.typ", "f2")]));
    assert_eq!(b.removed(), [("g.typ".to_owned(), "g typed".to_owned()), ("f.typ".to_owned(), "f typed".to_owned())]);
}

type Race<'a> = Box<dyn FnOnce(&HubVault) + 'a>;

/// Changes the server once, just before the first write or delete.
struct Racing<'a> {
    inner: &'a HubVault,
    race: RefCell<Option<Race<'a>>>,
}

impl<'a> Racing<'a> {
    fn new(inner: &'a HubVault, race: impl FnOnce(&HubVault) + 'a) -> Self {
        Self { inner, race: RefCell::new(Some(Box::new(race))) }
    }
    fn fire(&self) {
        if let Some(race) = self.race.borrow_mut().take() {
            race(self.inner);
        }
    }
}

impl Remote for Racing<'_> {
    fn changes(&self, after: u64) -> Result<Changes> {
        Ok(self.inner.changes(after))
    }
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.inner.get(path)
    }
    fn put(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome> {
        self.fire();
        self.inner.put(path, base, data)
    }
    fn delete(&self, path: &str, base: &Base) -> Result<Outcome> {
        self.fire();
        Remote::delete(self.inner, path, base)
    }
}

#[test]
fn a_write_race_is_settled_in_the_same_round() {
    for prefer in [Prefer::Local, Prefer::Remote] {
        let (_dir, hub, mut a, _b) = pair(&[("f.typ", "v1")]);
        a.prefer = prefer;
        a.tree.set("f.typ", b"mine");
        let racing = Racing::new(&hub, |hub| hub_put(hub, "f.typ", "server"));
        let report = a.sync(&racing);
        assert_eq!(report.conflicts, ["f.typ"]);
        match prefer {
            Prefer::Local => {
                assert_eq!((report.uploaded, report.downloaded), (1, 0));
                assert_eq!(hub_files(&hub)["f.typ"], b"mine");
            }
            Prefer::Remote => {
                assert_eq!((report.uploaded, report.downloaded), (0, 1));
                assert_eq!(a.files()["f.typ"], b"server");
                assert_eq!(a.removed(), [("f.typ".to_owned(), "mine".to_owned())]);
            }
        }
        assert!(a.sync(&hub).is_idle());

        // A delete against a server edit.
        let (_dir, hub, mut a, _b) = pair(&[("g.typ", "g1")]);
        a.prefer = prefer;
        a.tree.delete("g.typ");
        let racing = Racing::new(&hub, |hub| hub_put(hub, "g.typ", "g server"));
        let report = a.sync(&racing);
        assert_eq!(report.conflicts, ["g.typ"]);
        match prefer {
            Prefer::Local => assert!(hub_files(&hub).is_empty()),
            Prefer::Remote => assert_eq!(a.files(), files(&[("g.typ", "g server")])),
        }
        assert!(a.sync(&hub).is_idle());
    }
    // The server got the same text meanwhile: not a conflict.
    let (_dir, hub, mut a, _b) = pair(&[("f.typ", "v1")]);
    a.tree.set("f.typ", b"same");
    let racing = Racing::new(&hub, |hub| hub_put(hub, "f.typ", "same"));
    let report = a.sync(&racing);
    assert!(report.conflicts.is_empty());
    assert!(a.sync(&hub).is_idle());
}

#[test]
fn strange_server_data_is_skipped_or_repaired() {
    struct Bogus<'a>(&'a HubVault);
    impl Remote for Bogus<'_> {
        fn changes(&self, after: u64) -> Result<Changes> {
            let mut changes = self.0.changes(after);
            for path in ["../evil.typ", ".git/config", "/abs.typ"] {
                changes.entries.push(Entry { path: path.into(), hash: Some(hash_hex(b"x")), size: 1, seq: 1 });
            }
            Ok(changes)
        }
        fn get(&self, path: &str) -> Result<Vec<u8>> {
            self.0.get(path)
        }
        fn put(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome> {
            self.0.put(path, base, data)
        }
        fn delete(&self, path: &str, base: &Base) -> Result<Outcome> {
            Remote::delete(self.0, path, base)
        }
    }
    let (_dir, hub) = hub();
    hub_put(&hub, "ok.typ", "ok");
    let mut device = Device::new(Prefer::Remote);
    let report = device.sync(&Bogus(&hub));
    assert_eq!(report.skipped.len(), 3);
    assert_eq!(device.files(), files(&[("ok.typ", "ok")]));

    // A server restored from an old backup: numbers went back. The device
    // keeps its files and uploads what the server lost.
    let (_dir, old, mut device, _b) = pair(&[("x.typ", "x"), ("y.typ", "y"), ("z.typ", "z")]);
    device.sync(&old);
    let (_dir2, fresh) = hub_with(&[("x.typ", "x")]);
    assert!(device.state.remote_seq > fresh.seq());
    let report = device.sync(&fresh);
    assert_eq!((report.uploaded, report.removed_local), (2, 0));
    assert_eq!(device.files().len(), 3);
    assert_eq!(hub_files(&fresh), device.files());
}

fn hub_with(items: &[(&str, &str)]) -> (tempfile::TempDir, HubVault) {
    let (dir, hub) = hub();
    for (path, data) in items {
        hub_put(&hub, path, data);
    }
    (dir, hub)
}

#[test]
fn big_files_are_left_alone() {
    let (_dir, hub) = hub();
    let mut device = Device::with(Prefer::Local, &[("small.typ", "s")]);
    let big = vec![1u8; usize::try_from(MAX_FILE_SIZE).unwrap() + 1];
    device.tree.set("big.bin", &big);
    let report = device.sync(&hub);
    assert_eq!((report.uploaded, report.skipped), (1, vec!["big.bin".to_owned()]));
    assert_eq!(hub_files(&hub), files(&[("small.typ", "s")]));
    // It is not a deletion either.
    device.tree.delete("small.typ");
    device.sync(&hub);
    assert!(hub_files(&hub).is_empty());
}

#[test]
fn a_directory_tree_round() {
    let dir = tempfile::tempdir().unwrap();
    let hub = HubVault::open(dir.path().join("hub")).unwrap();
    let root = dir.path().join("vault");
    let tree = DirTree::new(&root, dir.path().join("removed"));
    let mut state = State::open(dir.path().join("state.json")).unwrap();
    let edit = |path: &str, data: &str, secs: u64| {
        let abs = root.join(path);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, data).unwrap();
        File::options().write(true).open(abs).unwrap().set_modified(UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
    };
    edit("a.typ", "a1", 100);
    edit("img/p.png", "png", 100);
    edit("_folder.toml", "title = 'x'", 100);
    edit(".baluk/settings.json", "{}", 100);
    edit(".baluk/cache.bin", "c", 100);
    edit(".git/config", "g", 100);
    edit("a.typ.tmp", "t", 100);
    let report = sync(&tree, &mut state, &hub, Prefer::Local).unwrap();
    assert_eq!(report.uploaded, 4);
    assert_eq!(
        hub_files(&hub).keys().map(String::as_str).collect::<Vec<_>>(),
        [".baluk/settings.json", "_folder.toml", "a.typ", "img/p.png"]
    );
    assert!(sync(&tree, &mut state, &hub, Prefer::Local).unwrap().is_idle());
    // A server edit, then a local edit with a new modification time.
    hub_put(&hub, "a.typ", "a2");
    hub_put(&hub, "new/n.typ", "n");
    let report = sync(&tree, &mut state, &hub, Prefer::Remote).unwrap();
    assert_eq!(report.downloaded, 2);
    assert_eq!(fs::read_to_string(root.join("a.typ")).unwrap(), "a2");
    edit("a.typ", "a3", 200);
    assert_eq!(sync(&tree, &mut state, &hub, Prefer::Remote).unwrap().uploaded, 1);
    assert_eq!(hub_files(&hub)["a.typ"], b"a3");
    // The state survives a restart.
    let mut state = State::open(dir.path().join("state.json")).unwrap();
    assert!(sync(&tree, &mut state, &hub, Prefer::Remote).unwrap().is_idle());
    // A deletion on the disk reaches the server, the old file is kept aside.
    fs::remove_file(root.join("new/n.typ")).unwrap();
    let report = sync(&tree, &mut state, &hub, Prefer::Remote).unwrap();
    assert_eq!(report.removed_remote, 1);
    assert!(!hub_files(&hub).contains_key("new/n.typ"));
    // And one on the server removes the local file into the removed folder.
    hub.delete("img/p.png", &Base::Any).unwrap().unwrap();
    let report = sync(&tree, &mut state, &hub, Prefer::Remote).unwrap();
    assert_eq!(report.removed_local, 1);
    assert!(!root.join("img").exists());
    let moved: Vec<_> = fs::read_dir(dir.path().join("removed")).unwrap().collect();
    assert!(!moved.is_empty());
}

/// A deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n
    }
}

#[test]
fn random_edits_converge() {
    const PATHS: [&str; 6] = ["a.typ", "b.typ", "d/c.typ", "d/e.typ", "f/g/h.typ", "f/i.typ"];
    for seed in 1..=6u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let (_dir, hub) = hub();
        let mut devices = [Device::new(Prefer::Local), Device::new(Prefer::Remote)];
        for _ in 0..250 {
            let device = &mut devices[usize::try_from(rng.below(2)).unwrap()];
            let path = PATHS[usize::try_from(rng.below(PATHS.len() as u64)).unwrap()];
            match rng.below(8) {
                0..=3 => device.tree.set(path, format!("v{}", rng.below(4)).as_bytes()),
                4 => device.tree.delete(path),
                _ => {
                    device.sync(&hub);
                }
            }
        }
        for _ in 0..3 {
            for device in &mut devices {
                device.sync(&hub);
            }
        }
        let server = hub_files(&hub);
        assert_eq!(devices[0].files(), server, "seed {seed}");
        assert_eq!(devices[1].files(), server, "seed {seed}");
        assert!(devices.iter_mut().all(|d| d.sync(&hub).is_idle()), "seed {seed}");
    }
}
