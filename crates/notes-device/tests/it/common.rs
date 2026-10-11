//! A hub for the whole test binary and a device on its own data directory.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use notes_core::VaultName;
use notes_device::testing::{PASSWORD, TestHub};
use notes_device::{Account, Paths, account};

/// Accounts of the tests: one per test that changes sessions or lists vaults.
const LOGINS: &[&str] =
    &["ivan", "anna", "pavel", "lena", "wera", "olga", "kira", "dana", "vera", "gleb", "mark", "nina"];

pub static HUB: LazyLock<TestHub> = LazyLock::new(|| TestHub::start(LOGINS));

pub struct Device {
    pub dir: tempfile::TempDir,
    pub paths: Paths,
}

impl Device {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        Self { dir, paths }
    }

    pub fn login(&self, login: &str) -> Account {
        let server = account::parse_server(&HUB.url).unwrap();
        notes_device::login(&self.paths, &server, login, PASSWORD).unwrap()
    }

    #[expect(clippy::unused_self, reason = "reads as the device's vault, like folder()")]
    pub fn vault(&self, name: &str) -> VaultName {
        VaultName::new(name).unwrap()
    }

    pub fn folder(&self, name: &str) -> PathBuf {
        self.paths.vault_dir(&self.vault(name))
    }

    pub fn write(&self, vault: &str, path: &str, text: &str) {
        let file = self.folder(vault).join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    pub fn read(&self, vault: &str, path: &str) -> Option<String> {
        std::fs::read_to_string(self.folder(vault).join(path)).ok()
    }

    /// The token in the account file (to use or break a session by hand).
    pub fn token(&self) -> String {
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(self.paths.account_file()).unwrap()).unwrap();
        json["token"].as_str().unwrap().to_owned()
    }
}

/// Waits until `check` is true.
pub fn wait_for(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}
