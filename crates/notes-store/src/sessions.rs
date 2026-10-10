//! Sign-in sessions and the pause after a wrong password (architecture §9).
//!
//! A session is a random token the browser keeps in a cookie. The file
//! `<data>/sessions.json` holds only the SHA-256 of the token, so a leaked file
//! signs nobody in. The file is read again when another process changed it
//! (`notes users passwd` ends the sessions of a user while the server runs).
//! Time is always a parameter (`now`, unix seconds,
//! [`now`] gives the current one) so tests do not sleep.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::fsutil::{FileStamp, write_private};

/// The disk copy of `seen` of a session is refreshed at most this often (in
/// seconds): a request does not write a file. After a restart a session can
/// therefore look up to an hour less used than it was.
pub const SEEN_SAVE_EVERY: u64 = 3600;

/// Errors of [`Sessions`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{path}: not a sessions file: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// The current time in unix seconds, for the `now` parameters.
#[must_use]
pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// One session as it is stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    /// Hex SHA-256 of the token.
    hash: String,
    login: String,
    created: u64,
    seen: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    sessions: Vec<Stored>,
}

#[derive(Debug)]
struct Entry {
    login: String,
    created: u64,
    seen: u64,
    /// The `seen` that is on disk.
    saved: u64,
}

#[derive(Debug)]
struct Inner {
    /// By the hash of the token: a lookup compares hashes, never the secret.
    entries: HashMap<String, Entry>,
    /// The version of the file `entries` were read from or written to
    /// (`None`: no file).
    stamp: Option<FileStamp>,
}

/// The sessions of the signed-in users.
#[derive(Debug)]
pub struct Sessions {
    path: PathBuf,
    lifetime: u64,
    inner: Mutex<Inner>,
}

impl Sessions {
    /// Opens the sessions file (no file - no sessions) and drops the expired
    /// ones. A session lives `lifetime` after its last use.
    ///
    /// # Errors
    /// The file cannot be read or is not a sessions file.
    pub fn open(path: impl Into<PathBuf>, lifetime: Duration) -> Result<Self, Error> {
        let path = path.into();
        let file = match std::fs::read(&path) {
            Ok(data) => {
                serde_json::from_slice::<File>(&data).map_err(|source| Error::Parse { path: path.clone(), source })?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => File::default(),
            Err(source) => return Err(Error::Io { path, source }),
        };
        let stamp = FileStamp::of(&path).map_err(|source| Error::Io { path: path.clone(), source })?;
        let entries = file.sessions.into_iter().map(stored_entry).collect();
        let sessions = Self { path, lifetime: lifetime.as_secs(), inner: Mutex::new(Inner { entries, stamp }) };
        {
            let mut inner = sessions.inner.lock();
            let before = inner.entries.len();
            sessions.prune(&mut inner.entries, now());
            if inner.entries.len() != before
                && let Err(e) = sessions.save(&mut inner)
            {
                tracing::warn!("{e}");
            }
        }
        Ok(sessions)
    }

    /// Starts a session for `login` and returns its token (32 random bytes,
    /// base64url without padding). Expired sessions are dropped on the way.
    ///
    /// # Errors
    /// The file cannot be written; then there is no session.
    pub fn create(&self, login: &str, now: u64) -> Result<String, Error> {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let hash = hash_token(&token);
        let mut inner = self.inner.lock();
        self.refresh(&mut inner);
        self.prune(&mut inner.entries, now);
        inner.entries.insert(hash.clone(), Entry { login: login.to_owned(), created: now, seen: now, saved: now });
        if let Err(e) = self.save(&mut inner) {
            inner.entries.remove(&hash);
            return Err(e);
        }
        Ok(token)
    }

    /// The login of a valid session, and the session counts as used at `now`.
    /// An expired one is dropped.
    #[must_use]
    pub fn check(&self, token: &str, now: u64) -> Option<String> {
        let hash = hash_token(token);
        let mut inner = self.inner.lock();
        self.refresh(&mut inner);
        let entry = inner.entries.get_mut(&hash)?;
        if self.expired(entry, now) {
            inner.entries.remove(&hash);
            if let Err(e) = self.save(&mut inner) {
                tracing::warn!("{e}");
            }
            return None;
        }
        entry.seen = entry.seen.max(now);
        let login = entry.login.clone();
        if entry.seen - entry.saved >= SEEN_SAVE_EVERY
            && let Err(e) = self.save(&mut inner)
        {
            tracing::warn!("{e}");
        }
        Some(login)
    }

    /// Ends the session of a token (sign-out). Whether there was one.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn revoke(&self, token: &str) -> Result<bool, Error> {
        let mut inner = self.inner.lock();
        self.refresh(&mut inner);
        let found = inner.entries.remove(&hash_token(token)).is_some();
        if found {
            self.save(&mut inner)?;
        }
        Ok(found)
    }

    /// Ends all sessions of a user (the password changed, the user removed).
    /// How many there were.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn revoke_user(&self, login: &str) -> Result<usize, Error> {
        let mut inner = self.inner.lock();
        self.refresh(&mut inner);
        let before = inner.entries.len();
        inner.entries.retain(|_, e| e.login != login);
        let removed = before - inner.entries.len();
        if removed > 0 {
            self.save(&mut inner)?;
        }
        Ok(removed)
    }

    fn expired(&self, entry: &Entry, now: u64) -> bool {
        now.saturating_sub(entry.seen) > self.lifetime
    }

    fn prune(&self, entries: &mut HashMap<String, Entry>, now: u64) {
        entries.retain(|_, e| !self.expired(e, now));
    }

    /// Reads the file again if another process changed it: the file is the
    /// truth (a session it no longer lists is ended), except that a use seen
    /// here and not yet saved is kept. A file that cannot be read now leaves
    /// the sessions as they are.
    fn refresh(&self, inner: &mut Inner) {
        let stamp = match FileStamp::of(&self.path) {
            Ok(stamp) => stamp,
            Err(e) => {
                tracing::warn!("{}: {e}", self.path.display());
                return;
            }
        };
        if stamp == inner.stamp {
            return;
        }
        let file = match std::fs::read(&self.path) {
            Ok(data) => match serde_json::from_slice::<File>(&data) {
                Ok(file) => file,
                Err(source) => {
                    tracing::warn!("{}", Error::Parse { path: self.path.clone(), source });
                    return;
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => File::default(),
            Err(e) => {
                tracing::warn!("{}: {e}", self.path.display());
                return;
            }
        };
        let mut entries: HashMap<String, Entry> = file.sessions.into_iter().map(stored_entry).collect();
        for (hash, entry) in &mut entries {
            if let Some(known) = inner.entries.get(hash) {
                entry.seen = entry.seen.max(known.seen);
            }
        }
        *inner = Inner { entries, stamp };
    }

    /// Writes all sessions; on success `saved` of all of them is `seen`.
    fn save(&self, inner: &mut Inner) -> Result<(), Error> {
        let mut sessions: Vec<Stored> = inner
            .entries
            .iter()
            .map(|(hash, e)| Stored { hash: hash.clone(), login: e.login.clone(), created: e.created, seen: e.seen })
            .collect();
        sessions.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.hash.cmp(&b.hash)));
        let data = serde_json::to_vec_pretty(&File { sessions })
            .map_err(|source| Error::Parse { path: self.path.clone(), source })?;
        write_private(&self.path, &data).map_err(|source| Error::Io { path: self.path.clone(), source })?;
        for e in inner.entries.values_mut() {
            e.saved = e.seen;
        }
        inner.stamp = FileStamp::of(&self.path).map_err(|source| Error::Io { path: self.path.clone(), source })?;
        Ok(())
    }
}

fn stored_entry(s: Stored) -> (String, Entry) {
    (s.hash, Entry { login: s.login, created: s.created, seen: s.seen, saved: s.seen })
}

/// Hex SHA-256 of a token.
fn hash_token(token: &str) -> String {
    use std::fmt::Write as _;
    Sha256::digest(token.as_bytes()).iter().fold(String::with_capacity(64), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// How many logins [`Throttle`] tracks at most.
const MAX_LOGINS: usize = 10_000;
/// The longest pause.
const MAX_WAIT: Duration = Duration::from_secs(60);
/// Failures of a login are forgotten after this long without a new one.
const FORGET_AFTER: Duration = Duration::from_mins(15);
/// More failures than this across all logins within [`GLOBAL_WINDOW`] slow
/// everybody down.
const GLOBAL_LIMIT: usize = 20;
const GLOBAL_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug)]
struct Failures {
    count: u32,
    last: Instant,
}

#[derive(Debug, Default)]
struct ThrottleState {
    logins: HashMap<String, Failures>,
    /// When the recent failures (of any login) happened, oldest first.
    recent: VecDeque<Instant>,
}

/// Slows down guessing passwords, in memory: after the n-th wrong password of
/// a login the next attempt is allowed after 1, 2, 4 ... 60 seconds; a flood
/// of failures across logins slows down every attempt. `now` is a monotonic
/// [`Instant`] (the caller passes `Instant::now()`, tests add durations).
#[derive(Debug, Default)]
pub struct Throttle {
    state: Mutex<ThrottleState>,
}

impl Throttle {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// May `login` try a password now? If not, how long to wait.
    ///
    /// # Errors
    /// The wait (at most 60 s).
    pub fn check(&self, login: &str, now: Instant) -> Result<(), Duration> {
        let mut state = self.state.lock();
        state.trim(now);
        let login = login.to_lowercase();
        let mut wait = Duration::ZERO;
        if let Some(f) = state.logins.get(&login) {
            let allowed = f.last + delay(f.count);
            wait = wait.max(allowed.saturating_duration_since(now));
        }
        if state.recent.len() > GLOBAL_LIMIT {
            // Fewer than the limit are left once this many of the oldest expire.
            let blocker = state.recent[state.recent.len() - GLOBAL_LIMIT - 1];
            wait = wait.max((blocker + GLOBAL_WINDOW).saturating_duration_since(now));
        }
        if wait.is_zero() { Ok(()) } else { Err(wait.min(MAX_WAIT)) }
    }

    /// A wrong password for `login`.
    pub fn failed(&self, login: &str, now: Instant) {
        let mut state = self.state.lock();
        state.trim(now);
        state.recent.push_back(now);
        if state.recent.len() > MAX_LOGINS {
            state.recent.pop_front();
        }
        let login = login.to_lowercase();
        if !state.logins.contains_key(&login) && state.logins.len() >= MAX_LOGINS {
            // Drop the one whose last failure is the oldest.
            let oldest = state.logins.iter().min_by_key(|(_, f)| f.last).map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                state.logins.remove(&oldest);
            }
        }
        let f = state.logins.entry(login).or_insert(Failures { count: 0, last: now });
        if now.saturating_duration_since(f.last) >= FORGET_AFTER {
            f.count = 0;
        }
        f.count = f.count.saturating_add(1);
        f.last = now;
    }

    /// A right password: the failures of `login` are forgotten.
    pub fn succeeded(&self, login: &str) {
        self.state.lock().logins.remove(&login.to_lowercase());
    }
}

impl ThrottleState {
    /// Forgets what is too old.
    fn trim(&mut self, now: Instant) {
        while self.recent.front().is_some_and(|t| now.saturating_duration_since(*t) >= GLOBAL_WINDOW) {
            self.recent.pop_front();
        }
        // Only a full map is cleaned (a scan per call is not needed otherwise).
        if self.logins.len() >= MAX_LOGINS {
            self.logins.retain(|_, f| now.saturating_duration_since(f.last) < FORGET_AFTER);
        }
    }
}

/// The pause after `count` failures in a row: 1, 2, 4 ... seconds, at most 60.
fn delay(count: u32) -> Duration {
    Duration::from_secs(1u64 << count.saturating_sub(1).min(6)).min(MAX_WAIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_hours(24);

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/sessions.json");
        (dir, path)
    }

    #[test]
    fn session_lifecycle() {
        let (_dir, path) = setup();
        let sessions = Sessions::open(&path, 30 * DAY).unwrap();
        let t0 = 1_000_000;
        let token = sessions.create("ivan", t0).unwrap();
        assert_eq!(token.len(), 43, "32 bytes, base64url without padding");
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(token, sessions.create("ivan", t0).unwrap(), "tokens are random");
        assert_eq!(sessions.check(&token, t0 + 10).as_deref(), Some("ivan"));
        assert_eq!(sessions.check("no such token", t0), None);
        assert_eq!(sessions.check("", t0), None);

        // Use extends: seen at day 20, so it lives until day 50.
        let day = DAY.as_secs();
        assert!(sessions.check(&token, t0 + 20 * day).is_some());
        assert!(sessions.check(&token, t0 + 49 * day).is_some());
        // Not used for 30 days and a second: expired, and gone for good.
        assert!(sessions.check(&token, t0 + 49 * day + 30 * day + 1).is_none());
        assert!(sessions.check(&token, t0 + 49 * day + 1).is_none());
    }

    #[test]
    fn revoking() {
        let (_dir, path) = setup();
        let sessions = Sessions::open(&path, 30 * DAY).unwrap();
        let t = now();
        let a1 = sessions.create("ivan", t).unwrap();
        let a2 = sessions.create("ivan", t).unwrap();
        let b = sessions.create("anna", t).unwrap();
        assert!(sessions.revoke(&a1).unwrap());
        assert!(!sessions.revoke(&a1).unwrap(), "already gone");
        assert!(sessions.check(&a1, t + 1).is_none());
        assert!(sessions.check(&a2, t + 1).is_some());
        assert_eq!(sessions.revoke_user("ivan").unwrap(), 1);
        assert_eq!(sessions.revoke_user("ivan").unwrap(), 0);
        assert!(sessions.check(&a2, t + 1).is_none());
        assert!(sessions.check(&b, t + 1).is_some(), "other users stay signed in");
        // The disk agrees.
        let again = Sessions::open(&path, 30 * DAY).unwrap();
        assert!(again.check(&a2, t + 2).is_none());
        assert!(again.check(&b, t + 2).is_some());
    }

    #[test]
    fn stored_on_disk_without_tokens() {
        let (_dir, path) = setup();
        let t0 = now();
        let token = {
            let sessions = Sessions::open(&path, 30 * DAY).unwrap();
            sessions.create("ivan", t0).unwrap()
        };
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&token), "the token is never stored");
        assert!(text.contains(&hash_token(&token)));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // Reopened: the same session.
        let again = Sessions::open(&path, 30 * DAY).unwrap();
        assert_eq!(again.check(&token, t0 + 5).as_deref(), Some("ivan"));
        // Only a shorter lifetime drops it on open.
        assert!(Sessions::open(&path, Duration::from_secs(0)).unwrap().check(&token, t0 + 5).is_none());
        assert!(!std::fs::read_to_string(&path).unwrap().contains(&hash_token(&token)), "pruned on open");
    }

    #[test]
    fn seen_is_saved_rarely_and_pruned_on_create() {
        let (_dir, path) = setup();
        let sessions = Sessions::open(&path, Duration::from_hours(10)).unwrap();
        let token = sessions.create("ivan", 1000).unwrap();
        let seen_on_disk = |p: &PathBuf| {
            let file: File = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
            file.sessions[0].seen
        };
        assert!(sessions.check(&token, 1000 + 60).is_some());
        assert_eq!(seen_on_disk(&path), 1000, "a use within the hour does not write");
        assert!(sessions.check(&token, 1000 + SEEN_SAVE_EVERY).is_some());
        assert_eq!(seen_on_disk(&path), 1000 + SEEN_SAVE_EVERY);

        // A new session drops the expired ones.
        let old = sessions.create("anna", 1000 + SEEN_SAVE_EVERY).unwrap();
        let later = 1000 + SEEN_SAVE_EVERY + 11 * 3600;
        let fresh = sessions.create("anna", later).unwrap();
        let file: File = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(file.sessions.len(), 1);
        assert!(sessions.check(&old, later).is_none());
        assert!(sessions.check(&token, later).is_none());
        assert!(sessions.check(&fresh, later).is_some());
    }

    #[test]
    fn follows_changes_made_by_another_process() {
        let (_dir, path) = setup();
        let t = now();
        let server = Sessions::open(&path, 30 * DAY).unwrap();
        let ivan = server.create("ivan", t).unwrap();
        let anna = server.create("anna", t).unwrap();
        // `notes users passwd ivan` in another process.
        let cli = Sessions::open(&path, Duration::from_secs(u64::MAX)).unwrap();
        assert_eq!(cli.revoke_user("ivan").unwrap(), 1);
        assert!(server.check(&ivan, t + 1).is_none(), "ended by the other process");
        assert!(server.check(&anna, t + 1).is_some());
        // The server writes later and does not bring the session back.
        let fresh = server.create("anna", t + 2).unwrap();
        let again = Sessions::open(&path, 30 * DAY).unwrap();
        assert!(again.check(&ivan, t + 3).is_none());
        assert!(again.check(&fresh, t + 3).is_some());
        // A use not saved yet survives a reload; a session made elsewhere is seen.
        assert!(server.check(&anna, t + 100).is_some());
        let bob = Sessions::open(&path, 30 * DAY).unwrap().create("bob", t + 5).unwrap();
        assert!(server.check(&bob, t + 101).is_some());
        assert_eq!(server.inner.lock().entries.get(&hash_token(&anna)).map(|e| e.seen), Some(t + 100));
    }

    #[test]
    fn broken_file() {
        let (_dir, path) = setup();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[]").unwrap();
        assert!(matches!(Sessions::open(&path, DAY), Err(Error::Parse { .. })));
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn throttle_delays() {
        let throttle = Throttle::new();
        let t0 = Instant::now();
        assert_eq!(throttle.check("ivan", t0), Ok(()));
        let mut now = t0;
        for expected in [1, 2, 4, 8, 16, 32, 60, 60, 60] {
            throttle.failed("ivan", now);
            assert_eq!(throttle.check("ivan", now), Err(secs(expected)));
            // Half-way there it is the rest; at the moment the attempt is free.
            assert_eq!(
                throttle.check("ivan", now + secs(expected - 1) + Duration::from_millis(500)),
                Err(Duration::from_millis(500))
            );
            now += secs(expected);
            assert_eq!(throttle.check("ivan", now), Ok(()));
        }
        // Other logins are not affected; the login is case-insensitive.
        assert_eq!(throttle.check("anna", t0), Ok(()));
        throttle.failed("IVAN", now);
        assert_eq!(throttle.check("ivan", now), Err(secs(60)));
        // A success forgets the failures.
        throttle.succeeded("Ivan");
        assert_eq!(throttle.check("ivan", now), Ok(()));
        throttle.failed("ivan", now);
        assert_eq!(throttle.check("ivan", now), Err(secs(1)));
    }

    #[test]
    fn throttle_forgets() {
        let throttle = Throttle::new();
        let t0 = Instant::now();
        for _ in 0..5 {
            throttle.failed("ivan", t0);
        }
        assert_eq!(throttle.check("ivan", t0), Err(secs(16)));
        // 15 minutes after the last failure the count starts over.
        throttle.failed("ivan", t0 + secs(15 * 60));
        assert_eq!(throttle.check("ivan", t0 + secs(15 * 60)), Err(secs(1)));
        assert_eq!(throttle.check("ivan", t0 + secs(15 * 60 + 1)), Ok(()));
    }

    #[test]
    fn throttle_global_limit_and_bound() {
        let throttle = Throttle::new();
        let t0 = Instant::now();
        // 20 failures across different logins in a minute are tolerated ...
        for i in 0..20 {
            throttle.failed(&format!("user{i}"), t0 + Duration::from_millis(i));
        }
        assert_eq!(throttle.check("fresh", t0 + secs(1)), Ok(()));
        // ... the 21st slows everybody down until the oldest ones age out.
        throttle.failed("user20", t0 + secs(1));
        let wait = throttle.check("fresh", t0 + secs(2)).unwrap_err();
        assert!(wait > secs(55) && wait <= secs(60), "{wait:?}");
        assert_eq!(throttle.check("fresh", t0 + secs(61)), Ok(()));

        // Bounded memory: thousands of logins do not grow it, the oldest go.
        let big = Throttle::new();
        for i in 0..(MAX_LOGINS + 50) {
            big.failed(&format!("u{i}"), t0 + Duration::from_millis(i as u64));
        }
        let state = big.state.lock();
        assert_eq!(state.logins.len(), MAX_LOGINS);
        assert!(!state.logins.contains_key("u0") && state.logins.contains_key(&format!("u{}", MAX_LOGINS + 49)));
    }
}
