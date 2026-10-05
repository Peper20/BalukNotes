//! Warming: all notes are built in advance, in the background, so they open
//! at once.
//!
//! Works on top of [`Pages`], only through its public methods.
//!
//! - **Order** ([`order`]): first the notes hinted by the client (open in
//!   tabs, recent); then notes never built (small to large - a new note is
//!   most likely the one being written); then built ones by the time of the
//!   last build (it is in the disk cache: CeTZ figures decide the time, not
//!   bytes); books never built go last.
//! - **Disk only**: a build goes to the disk cache and is not kept in memory
//!   ([`Pages::prebuild`]), only open notes take memory. Without a disk
//!   cache - in memory, within [`crate::page_cache::MEMORY_BUDGET`].
//! - **Not twice**: a note that has a build for its current files (an
//!   erroneous one too) is skipped - what was built in the last run is not
//!   built again.
//! - **The user goes first**: builds run one at a time, and warming does
//!   not take the next note while someone waits for a page. It does not
//!   interrupt a build already started (see `docs/tech-debt.md`).
//! - **Gently**: a warming build does the themes one by one in one thread
//!   with lowered priority ([`crate::world::Priority::Background`]). After a
//!   big pass (start, a library edit) Typst memory is released - rarely
//!   ([`should_release`]): an ordinary note edit leaves it alone, otherwise
//!   the rebuild after the edit would lose the memo.
//! - **Mode**: a device setting ([`WarmMode`]) - the whole vault, only what
//!   the client hinted (open notes) or off.
//!
//! A pass repeats on a new hint, on a change of vault files
//! ([`Warmer::poke`] from the watcher, [`crate::watch`]) and, just in case,
//! once per [`RESCAN`] (without the watcher - once per [`RESCAN_UNWATCHED`]):
//! new and changed notes get built in advance too. Checking an unchanged
//! note is a few `stat` calls. When the background thread starts, the disk
//! cache is cleaned ([`crate::cache::DiskCache::prune`]).

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use crate::pages::Pages;
use crate::vault::{Entry, NoteId, NoteKind};

/// How often to check for something unbuilt while the file watcher works
/// (insurance against a lost event).
pub const RESCAN: Duration = Duration::from_secs(600);

/// The same without the file watcher.
pub const RESCAN_UNWATCHED: Duration = Duration::from_secs(60);

/// What to warm (a device setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum WarmMode {
    /// The whole vault (a computer).
    #[default]
    All = 0,
    /// Only what the client hinted: notes in tabs, recent ones.
    Open = 1,
    /// Nothing: notes are built when opened (a phone).
    Off = 2,
}

impl WarmMode {
    const ALL: [Self; 3] = [Self::All, Self::Open, Self::Off];

    /// Setting value: `all`, `open`, `off`.
    pub fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Open => "open",
            Self::Off => "off",
        }
    }

    /// The mode by its setting value.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    /// The mode stored in [`Warmer`] as its discriminant; an unknown value is the default.
    fn from_u8(value: u8) -> Self {
        Self::ALL.into_iter().find(|m| *m as u8 == value).unwrap_or_default()
    }
}

/// Release memory after a pass that built at least this many notes.
pub const RELEASE_MIN_BUILT: usize = 5;

/// And not more often than once per this.
pub const RELEASE_EVERY: Duration = Duration::from_secs(600);

/// Whether to release Typst memory after a pass: only a big pass and not
/// more often than [`RELEASE_EVERY`] (`last` is the previous release).
pub fn should_release(stats: WarmStats, last: Option<Instant>, now: Instant) -> bool {
    stats.built >= RELEASE_MIN_BUILT && last.is_none_or(|t| now.duration_since(t) >= RELEASE_EVERY)
}

/// Warming: the client's hints and the alarm of the background thread.
#[derive(Debug, Default)]
pub struct Warmer {
    hints: Mutex<Vec<NoteId>>,
    /// [`WarmMode`] as its discriminant.
    mode: AtomicU8,
    /// Grows with every hint: the pass starts anew.
    generation: AtomicU64,
    wake: Condvar,
    /// The vault is closed ([`Warmer::stop`]): the background thread exits.
    stopped: AtomicBool,
}

/// The result of one pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WarmStats {
    /// Notes built.
    pub built: usize,
    /// Notes already built.
    pub skipped: usize,
}

/// Build order: hints (in their order); then notes never built, by source
/// size; then built ones by the time of the last build; then books never
/// built, by size.
pub fn order(
    entries: &[Entry],
    size: impl Fn(&Entry) -> u64,
    last_build: impl Fn(&Entry) -> Option<Duration>,
    hints: &[NoteId],
) -> Vec<NoteId> {
    let mut rest: Vec<_> = entries.iter().filter(|e| !hints.contains(&e.id)).collect();
    rest.sort_by_cached_key(|e| {
        let (tier, weight) = match (last_build(e), e.kind) {
            (None, NoteKind::Note) => (0, u128::from(size(e))),
            (Some(took), _) => (1, took.as_millis()),
            (None, NoteKind::Book) => (2, u128::from(size(e))),
        };
        (tier, weight, e.id.as_str().to_owned())
    });
    let hinted = hints.iter().filter(|h| entries.iter().any(|e| &e.id == *h)).cloned();
    hinted.chain(rest.into_iter().map(|e| e.id.clone())).collect()
}

impl Warmer {
    /// Tells what to build first (notes in tabs, recent ones).
    pub fn hint(&self, ids: Vec<NoteId>) {
        // Under the lock: the background thread checks the generation under it before sleeping.
        let mut hints = self.hints.lock();
        *hints = ids;
        self.generation.fetch_add(1, Ordering::SeqCst);
        drop(hints);
        self.wake.notify_all();
    }

    /// The current mode.
    pub fn mode(&self) -> WarmMode {
        WarmMode::from_u8(self.mode.load(Ordering::SeqCst))
    }

    /// Changes the mode; if it changed, passes anew.
    pub fn set_mode(&self, mode: WarmMode) {
        let value = mode as u8;
        if self.mode.swap(value, Ordering::SeqCst) != value {
            self.poke();
        }
    }

    /// Stops warming for good (the vault is closed): the pass ends, the
    /// background thread ([`Warmer::forever`]) exits.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.set_mode(WarmMode::Off);
        self.poke();
    }

    /// Vault files changed: pass anew (with the same hints).
    pub fn poke(&self) {
        let hints = self.hints.lock();
        self.generation.fetch_add(1, Ordering::SeqCst);
        drop(hints);
        self.wake.notify_all();
    }

    /// One pass: builds everything unbuilt in order (in the mode
    /// [`WarmMode::Open`] only what was hinted). A new hint restarts the
    /// pass (what is built gets skipped).
    pub fn pass(&self, pages: &Pages) -> WarmStats {
        let mut stats = WarmStats::default();
        'pass: loop {
            let mode = self.mode();
            if mode == WarmMode::Off {
                return stats;
            }
            let generation = self.generation.load(Ordering::SeqCst);
            let Ok(entries) = pages.vault().entries() else { return stats };
            let hints = self.hints.lock().clone();
            let vault = pages.vault();
            let mut ids = order(&entries, |e| vault.source_size(e), |e| pages.last_build(&e.id), &hints);
            if mode == WarmMode::Open {
                ids.retain(|id| hints.contains(id));
            }
            for id in ids {
                if self.generation.load(Ordering::SeqCst) != generation {
                    continue 'pass;
                }
                if pages.is_built(&id) {
                    stats.skipped += 1;
                    continue;
                }
                pages.wait_for_users();
                match pages.prebuild(&id) {
                    Ok(true) => stats.built += 1,
                    // While we waited, a user request built the note.
                    Ok(false) => stats.skipped += 1,
                    Err(e) => tracing::debug!(%id, "warming: {e}"),
                }
            }
            return stats;
        }
    }

    /// Warms forever (a server background thread). First cleans the disk
    /// cache. `rescan` is how long to sleep without hints and changes.
    pub fn forever(&self, pages: &Pages, rescan: impl Fn() -> Duration) {
        prune(pages);
        let mut released = None;
        while !self.stopped.load(Ordering::SeqCst) {
            let started = Instant::now();
            let stats = self.pass(pages);
            if should_release(stats, released, Instant::now()) {
                pages.release_memory();
                released = Some(Instant::now());
            }
            if stats.built > 0 {
                let (held, bytes) = pages.cache().memory();
                tracing::info!(
                    built = stats.built,
                    skipped = stats.skipped,
                    ms = started.elapsed().as_millis(),
                    held,
                    bytes,
                    "warming: notes built in advance"
                );
            }
            let generation = self.generation.load(Ordering::SeqCst);
            let mut hints = self.hints.lock();
            if self.generation.load(Ordering::SeqCst) == generation && !self.stopped.load(Ordering::SeqCst) {
                self.wake.wait_for(&mut hints, rescan());
            }
        }
    }
}

/// Cleans the disk cache: entries of notes that are gone, and leftovers.
pub fn prune(pages: &Pages) {
    let Ok(entries) = pages.vault().entries() else { return };
    let alive = |id: &str| entries.iter().any(|e| e.id.as_str() == id);
    if let Some(p) = pages.cache().prune(&alive) {
        tracing::info!(removed = p.removed, bytes = p.bytes, "disk cache cleaned");
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::pages::tests::Setup;

    fn entry(id: &str, kind: NoteKind) -> Entry {
        Entry { id: NoteId::new(id).unwrap(), kind, main: PathBuf::from(format!("{id}.typ")) }
    }

    #[test]
    fn hints_new_notes_known_by_time_then_new_books() {
        let entries = [
            entry("Большая", NoteKind::Note),
            entry("Книга", NoteKind::Book),
            entry("Малая", NoteKind::Note),
            entry("Открытая", NoteKind::Note),
            entry("Тонкая книга", NoteKind::Book),
            entry("Медленная", NoteKind::Note),
            entry("Собранная книга", NoteKind::Book),
        ];
        let size = |e: &Entry| match e.id.as_str() {
            "Большая" => 9000,
            "Книга" => 50_000,
            "Тонкая книга" => 10,
            _ => 100,
        };
        let last = |e: &Entry| match e.id.as_str() {
            "Медленная" => Some(Duration::from_secs(3)),
            "Собранная книга" => Some(Duration::from_millis(200)),
            _ => None,
        };
        let hints = [NoteId::new("Открытая").unwrap(), NoteId::new("Удалённая").unwrap()];
        let ids: Vec<_> = order(&entries, size, last, &hints).into_iter().map(|id| id.to_string()).collect();
        assert_eq!(ids, ["Открытая", "Малая", "Большая", "Собранная книга", "Медленная", "Тонкая книга", "Книга"]);
    }

    #[test]
    fn warms_everything_once_across_restarts() {
        let s = Setup::new(&[("A.typ", "a"), ("B.typ", "ошибка"), ("Книга/main.typ", "k")]);
        let (pages, pipeline) = s.pages(true, Duration::ZERO);
        let warmer = Warmer::default();
        warmer.hint(vec![NoteId::new("Книга").unwrap()]);
        assert_eq!(warmer.pass(&pages), WarmStats { built: 3, skipped: 0 });
        assert_eq!(pages.cache().memory(), (0, 0), "warming does not keep pages in memory");
        assert_eq!(warmer.pass(&pages), WarmStats { built: 0, skipped: 3 });

        // A new run: everything is on disk, the note with an error too.
        let (restarted, _) = s.pages(true, Duration::ZERO);
        assert_eq!(Warmer::default().pass(&restarted), WarmStats { built: 0, skipped: 3 });

        // An edit: only that note is built.
        s.mem.write("A.typ", "aa");
        assert_eq!(Warmer::default().pass(&restarted), WarmStats { built: 1, skipped: 2 });
        assert_eq!(pipeline.builds.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn modes_open_and_off() {
        let s = Setup::new(&[("A.typ", "a"), ("B.typ", "b")]);
        let (pages, _) = s.pages(true, Duration::ZERO);
        let warmer = Warmer::default();
        assert_eq!(warmer.mode(), WarmMode::All);
        warmer.set_mode(WarmMode::Off);
        warmer.hint(vec![NoteId::new("A").unwrap()]);
        assert_eq!(warmer.pass(&pages), WarmStats::default(), "off");
        warmer.set_mode(WarmMode::Open);
        assert_eq!(warmer.pass(&pages), WarmStats { built: 1, skipped: 0 }, "only the hinted");
        assert!(!pages.is_built(&NoteId::new("B").unwrap()));
        warmer.set_mode(WarmMode::All);
        assert_eq!(warmer.pass(&pages), WarmStats { built: 1, skipped: 1 });
        for m in WarmMode::ALL {
            assert_eq!(WarmMode::from_key(m.key()), Some(m));
        }
    }

    #[test]
    fn memory_is_released_rarely() {
        let t = Instant::now();
        let big = WarmStats { built: RELEASE_MIN_BUILT, skipped: 0 };
        let one = WarmStats { built: 1, skipped: 10 };
        assert!(should_release(big, None, t), "the first big pass (start)");
        assert!(!should_release(one, None, t), "not after editing one note");
        assert!(!should_release(big, Some(t), t + RELEASE_EVERY / 2), "not more often than RELEASE_EVERY");
        assert!(should_release(big, Some(t), t + RELEASE_EVERY));
    }

    #[test]
    fn poke_restarts_pass() {
        let warmer = Warmer::default();
        let before = warmer.generation.load(Ordering::SeqCst);
        warmer.hint(vec![NoteId::new("A").unwrap()]);
        warmer.poke();
        assert_eq!(warmer.generation.load(Ordering::SeqCst), before + 2);
        assert_eq!(warmer.hints.lock().len(), 1, "the same hints");
    }

    #[test]
    fn prune_forgets_deleted_notes() {
        let s = Setup::new(&[("A.typ", "a"), ("B.typ", "b")]);
        let (pages, _) = s.pages(true, Duration::ZERO);
        Warmer::default().pass(&pages);
        s.mem.remove("B.typ");
        prune(&pages);
        let (restarted, _) = s.pages(true, Duration::ZERO);
        assert!(restarted.is_built(&NoteId::new("A").unwrap()));
        assert!(!restarted.is_built(&NoteId::new("B").unwrap()));
    }
}
