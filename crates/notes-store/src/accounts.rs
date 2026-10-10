//! Users of the storage server: a login and an Argon2id password hash in
//! `<data>/users.json` (architecture §9). The password is never stored.
//!
//! The file is read again when another process changed it (`notes users add`
//! while the server runs), so a new user can sign in without a restart.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::fsutil::{FileStamp, write_private};

/// The longest login, in characters.
pub const MAX_LOGIN: usize = 32;
/// The shortest password, in characters.
pub const MIN_PASSWORD: usize = 8;
/// The longest password, in bytes (a bound on the hashing work).
pub const MAX_PASSWORD: usize = 1024;

/// Errors of [`Accounts`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid login \"{login}\": {reason}")]
    InvalidLogin { login: String, reason: &'static str },

    #[error("password: at least {MIN_PASSWORD} characters")]
    PasswordTooShort,

    #[error("password: at most {MAX_PASSWORD} bytes")]
    PasswordTooLong,

    #[error("user \"{0}\" already exists")]
    Exists(String),

    #[error("no user \"{0}\"")]
    NotFound(String),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{path}: not a users file: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("cannot hash the password: {0}")]
    Hash(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    login: String,
    /// An Argon2id PHC string.
    password: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    users: Vec<User>,
}

#[derive(Debug)]
struct State {
    users: Vec<User>,
    /// The version of the file `users` was read from (`None`: no file).
    stamp: Option<FileStamp>,
}

/// The users: list, add, remove, change a password, check a password. The
/// methods take `&self`, the state is behind a mutex.
#[derive(Debug)]
pub struct Accounts {
    path: PathBuf,
    state: Mutex<State>,
}

impl Accounts {
    /// Opens the users file; no file means no users (it appears on the first
    /// change).
    ///
    /// # Errors
    /// The file exists but cannot be read or is not a users file.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let state = Mutex::new(State { users: Vec::new(), stamp: None });
        let accounts = Self { path, state };
        accounts.refresh(&mut accounts.state.lock())?;
        Ok(accounts)
    }

    /// The logins, in the order they were added.
    ///
    /// # Errors
    /// The file changed on disk and cannot be read now.
    pub fn list(&self) -> Result<Vec<String>, Error> {
        let mut state = self.state.lock();
        self.refresh(&mut state)?;
        Ok(state.users.iter().map(|u| u.login.clone()).collect())
    }

    /// Whether there are no users at all.
    ///
    /// # Errors
    /// The file changed on disk and cannot be read now.
    pub fn is_empty(&self) -> Result<bool, Error> {
        let mut state = self.state.lock();
        self.refresh(&mut state)?;
        Ok(state.users.is_empty())
    }

    /// Adds a user. The login is lowercased.
    ///
    /// # Errors
    /// An invalid login or password, a user that exists, a failed write.
    pub fn add(&self, login: &str, password: &str) -> Result<(), Error> {
        let login = normalize(login)?;
        check_password(password)?;
        let hash = hash_password(password)?;
        let mut state = self.state.lock();
        self.refresh(&mut state)?;
        if state.users.iter().any(|u| u.login == login) {
            return Err(Error::Exists(login));
        }
        state.users.push(User { login, password: hash });
        self.save(&mut state)
    }

    /// Removes a user.
    ///
    /// # Errors
    /// No such user, a failed write.
    pub fn remove(&self, login: &str) -> Result<(), Error> {
        let login = normalize(login)?;
        let mut state = self.state.lock();
        self.refresh(&mut state)?;
        let Some(at) = state.users.iter().position(|u| u.login == login) else {
            return Err(Error::NotFound(login));
        };
        state.users.remove(at);
        self.save(&mut state)
    }

    /// Replaces the password of a user.
    ///
    /// # Errors
    /// An invalid password, no such user, a failed write.
    pub fn set_password(&self, login: &str, password: &str) -> Result<(), Error> {
        let login = normalize(login)?;
        check_password(password)?;
        let hash = hash_password(password)?;
        let mut state = self.state.lock();
        self.refresh(&mut state)?;
        let Some(user) = state.users.iter_mut().find(|u| u.login == login) else {
            return Err(Error::NotFound(login));
        };
        user.password = hash;
        self.save(&mut state)
    }

    /// Whether the login and the password match. For an unknown login a hash
    /// is still verified (against a dummy one), so the time does not tell
    /// whether the login exists. Any failure to read the file counts as "no"
    /// (the users known from before stay valid).
    #[must_use]
    pub fn verify(&self, login: &str, password: &str) -> bool {
        let Ok(login) = normalize(login) else {
            dummy_verify(password);
            return false;
        };
        let hash = {
            let mut state = self.state.lock();
            if let Err(e) = self.refresh(&mut state) {
                tracing::warn!("{e}");
            }
            state.users.iter().find(|u| u.login == login).map(|u| u.password.clone())
        };
        if let Some(hash) = hash {
            return verify_hash(&hash, password);
        }
        dummy_verify(password);
        false
    }

    /// Reads the file again if it is not the version the state came from.
    fn refresh(&self, state: &mut State) -> Result<(), Error> {
        let stamp = FileStamp::of(&self.path).map_err(|e| self.io(e))?;
        if stamp == state.stamp {
            return Ok(());
        }
        let users = match fs::read(&self.path) {
            Ok(data) => {
                serde_json::from_slice::<File>(&data)
                    .map_err(|source| Error::Parse { path: self.path.clone(), source })?
                    .users
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(self.io(e)),
        };
        *state = State { users, stamp };
        Ok(())
    }

    fn save(&self, state: &mut State) -> Result<(), Error> {
        let file = File { users: state.users.clone() };
        let data =
            serde_json::to_vec_pretty(&file).map_err(|source| Error::Parse { path: self.path.clone(), source })?;
        write_private(&self.path, &data).map_err(|e| self.io(e))?;
        state.stamp = FileStamp::of(&self.path).map_err(|e| self.io(e))?;
        Ok(())
    }

    fn io(&self, source: io::Error) -> Error {
        Error::Io { path: self.path.clone(), source }
    }

    /// The path of the users file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The login in its stored form (lowercase) if it is valid.
fn normalize(login: &str) -> Result<String, Error> {
    let invalid = |reason| Err(Error::InvalidLogin { login: login.to_owned(), reason });
    let lower = login.to_lowercase();
    if lower.is_empty() {
        return invalid("empty");
    }
    if lower.chars().count() > MAX_LOGIN {
        return invalid("longer than 32 characters");
    }
    if !lower.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')) {
        return invalid("only a-z, 0-9, . _ - are allowed");
    }
    if !lower.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        return invalid("must start with a letter or a digit");
    }
    Ok(lower)
}

fn check_password(password: &str) -> Result<(), Error> {
    if password.len() > MAX_PASSWORD {
        return Err(Error::PasswordTooLong);
    }
    if password.chars().count() < MIN_PASSWORD {
        return Err(Error::PasswordTooShort);
    }
    Ok(())
}

fn hash_password(password: &str) -> Result<String, Error> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let salt = SaltString::encode_b64(&salt).map_err(|e| Error::Hash(e.to_string()))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| Error::Hash(e.to_string()))
}

fn verify_hash(hash: &str, password: &str) -> bool {
    if password.len() > MAX_PASSWORD {
        return false;
    }
    let Ok(parsed) = PasswordHash::new(hash) else {
        tracing::warn!("a user has a broken password hash");
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

/// A verification that always fails, for the cost of a real one.
fn dummy_verify(password: &str) {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let hash = DUMMY.get_or_init(|| hash_password("dummy password").unwrap_or_default());
    let _ = verify_hash(hash, password);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/users.json");
        (dir, path)
    }

    #[test]
    fn users_round_trip() {
        let (_dir, path) = setup();
        let accounts = Accounts::open(&path).unwrap();
        assert!(accounts.is_empty().unwrap());
        assert!(!path.exists(), "no file until the first change");
        accounts.add("Ivan", "correct horse").unwrap();
        accounts.add("anna.k-2", "battery staple").unwrap();
        assert_eq!(accounts.list().unwrap(), ["ivan", "anna.k-2"]);
        assert!(accounts.verify("ivan", "correct horse"));
        assert!(accounts.verify("IVAN", "correct horse"), "the login is case-insensitive");
        assert!(!accounts.verify("ivan", "wrong password"));
        assert!(!accounts.verify("nobody", "correct horse"), "an unknown user");
        assert!(!accounts.verify("bad login!", "correct horse"), "an invalid login");

        // Reopened: the same users and passwords.
        let again = Accounts::open(&path).unwrap();
        assert_eq!(again.list().unwrap(), ["ivan", "anna.k-2"]);
        assert!(again.verify("anna.k-2", "battery staple"));
        again.set_password("ivan", "new password").unwrap();
        assert!(!again.verify("ivan", "correct horse"));
        assert!(again.verify("ivan", "new password"));
        again.remove("anna.k-2").unwrap();
        assert_eq!(Accounts::open(&path).unwrap().list().unwrap(), ["ivan"]);
        assert!(!again.is_empty().unwrap());
        again.remove("ivan").unwrap();
        assert!(again.is_empty().unwrap());
        assert!(!again.verify("ivan", "new password"));
    }

    #[test]
    fn rejects_bad_input() {
        let (_dir, path) = setup();
        let accounts = Accounts::open(&path).unwrap();
        let long = "a".repeat(MAX_LOGIN + 1);
        for bad in ["", "-ivan", ".ivan", "_ivan", "iv an", "иван", "a/b", "a@b", long.as_str()] {
            let err = accounts.add(bad, "long enough").unwrap_err();
            assert!(matches!(err, Error::InvalidLogin { .. }), "{bad:?}: {err}");
        }
        assert!(accounts.add(&"a".repeat(MAX_LOGIN), "long enough").is_ok());
        assert!(matches!(accounts.add("ivan", "short"), Err(Error::PasswordTooShort)));
        assert!(matches!(accounts.add("ivan", &"x".repeat(MAX_PASSWORD + 1)), Err(Error::PasswordTooLong)));
        accounts.add("ivan", "пароль 8 букв").unwrap();
        assert!(matches!(accounts.add("IVAN", "another one"), Err(Error::Exists(l)) if l == "ivan"));
        assert!(matches!(accounts.remove("nobody"), Err(Error::NotFound(_))));
        assert!(matches!(accounts.set_password("nobody", "long enough"), Err(Error::NotFound(_))));
        assert!(matches!(accounts.set_password("ivan", "short"), Err(Error::PasswordTooShort)));
        assert!(accounts.verify("ivan", "пароль 8 букв"));
        assert!(!accounts.verify("ivan", &"x".repeat(MAX_PASSWORD + 1)));
        assert_eq!(Accounts::open(&path).unwrap().list().unwrap().len(), 2);

        let messages = [
            (
                accounts.add("Bad Login", "long enough").unwrap_err(),
                r#"invalid login "Bad Login": only a-z, 0-9, . _ - are allowed"#,
            ),
            (
                accounts.add("-x", "long enough").unwrap_err(),
                r#"invalid login "-x": must start with a letter or a digit"#,
            ),
            (accounts.add("ivan", "long enough").unwrap_err(), r#"user "ivan" already exists"#),
            (accounts.remove("zed").unwrap_err(), r#"no user "zed""#),
            (accounts.add("zed", "short").unwrap_err(), "password: at least 8 characters"),
        ];
        for (err, text) in messages {
            assert_eq!(err.to_string(), text);
        }
    }

    #[test]
    fn file_holds_hashes_only() {
        let (_dir, path) = setup();
        let accounts = Accounts::open(&path).unwrap();
        accounts.add("ivan", "very secret pass").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("very secret pass"));
        let file: File = serde_json::from_str(&text).unwrap();
        assert_eq!(file.users[0].login, "ivan");
        assert!(file.users[0].password.starts_with("$argon2id$"), "{text}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn sees_changes_of_another_process() {
        let (_dir, path) = setup();
        let server = Accounts::open(&path).unwrap();
        assert!(!server.verify("ivan", "long enough"));
        // `notes users add` in another process: a second handle on the file.
        let cli = Accounts::open(&path).unwrap();
        cli.add("ivan", "long enough").unwrap();
        assert!(server.verify("ivan", "long enough"));
        assert_eq!(server.list().unwrap(), ["ivan"]);
        cli.set_password("ivan", "changed it now").unwrap();
        assert!(!server.verify("ivan", "long enough"));
        assert!(server.verify("ivan", "changed it now"));
        // The server changes the file too, on top of the other one's work.
        server.add("anna", "long enough").unwrap();
        cli.remove("ivan").unwrap();
        assert_eq!(server.list().unwrap(), ["anna"]);
    }

    #[test]
    fn broken_file() {
        let (_dir, path) = setup();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json").unwrap();
        let err = Accounts::open(&path).unwrap_err();
        assert!(matches!(err, Error::Parse { .. }));
        assert!(err.to_string().contains("users.json"), "{err}");
    }
}
