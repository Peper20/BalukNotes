//! Vaults: listing and creating (`GET`/`POST /api/vaults`), renaming and
//! moving to the trash (`PATCH`/`DELETE /api/vaults/{vault}`), and the vaults
//! the server has open.
//!
//! The note API lives under `/api/vaults/{vault}/...` (modules `notes`,
//! `graph`, `search`, `events`). A vault opens on first use: each has its own
//! core (`Notes`: cache, link index, watcher, warming) and its own change log
//! (`events`). Only the **active** vault is warmed - the last opened one, or
//! the one the client last sent a warming hint for (`POST .../warm`, at client
//! start); the others build notes on request. The server closes inactive
//! vaults with no requests and no waiting events after [`IDLE_CLOSE`]; the
//! next request opens them again.
//!
//! The vaults come from the data directory (`notes_core::vaults`, on request;
//! there may be none at all - the first is created in the client) or from one
//! vault opened ahead (`notes serve --vault ...`, tests): then no new ones are
//! created, and that one is neither renamed nor deleted. Themes and fonts are
//! shared by all (one library): the core without a vault serves them
//! ([`VaultSet::library`]).

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use notes_core::figures::FigureOptions;
use notes_core::settings::{Schema, SettingsStore, VaultSettings};
use notes_core::warm::WarmMode;
use notes_core::{Notes, NotesConfig, SharedAssets, VaultName, Vaults};
use tokio::sync::watch;

use crate::AppState;
use crate::api::{ChangeEvent, CreateVaultRequest, RenameVaultRequest, VaultsResponse};
use crate::error::{ApiError, ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/vaults", get(list).post(create))
        .route("/api/vaults/{vault}", patch(rename).delete(remove))
}

/// After how long the server closes an inactive vault.
pub const IDLE_CLOSE: Duration = Duration::from_secs(600);

/// How often the server checks whether to close inactive vaults.
const IDLE_POLL_MAX: Duration = Duration::from_secs(30);
const IDLE_POLL_MIN: Duration = Duration::from_millis(100);

/// Only for vaults of the data directory; a server on one vault answers 403.
fn registry_only(s: &AppState) -> ApiResult<()> {
    if s.vaults.can_create() {
        Ok(())
    } else {
        Err(ApiError(StatusCode::FORBIDDEN, "the server runs on a single vault (--vault <path>)".into()))
    }
}

async fn list(State(s): State<AppState>) -> ApiResult<Json<VaultsResponse>> {
    let vaults = s.vaults.clone();
    Ok(Json(blocking(move || vaults.describe()).await?))
}

async fn create(
    State(s): State<AppState>,
    Json(req): Json<CreateVaultRequest>,
) -> ApiResult<(StatusCode, Json<VaultsResponse>)> {
    registry_only(&s)?;
    let vaults = s.vaults.clone();
    let list = blocking(move || {
        vaults.create(&VaultName::new(req.name)?)?;
        vaults.describe()
    })
    .await?;
    Ok((StatusCode::CREATED, Json(list)))
}

async fn rename(
    State(s): State<AppState>,
    Path(vault): Path<String>,
    Json(req): Json<RenameVaultRequest>,
) -> ApiResult<Json<VaultsResponse>> {
    registry_only(&s)?;
    let vaults = s.vaults.clone();
    Ok(Json(
        blocking(move || {
            vaults.rename(&vault, &VaultName::new(req.name)?)?;
            vaults.describe()
        })
        .await?,
    ))
}

async fn remove(State(s): State<AppState>, Path(vault): Path<String>) -> ApiResult<Json<VaultsResponse>> {
    registry_only(&s)?;
    let vaults = s.vaults.clone();
    Ok(Json(
        blocking(move || {
            vaults.trash(&vault)?;
            vaults.describe()
        })
        .await?,
    ))
}

/// An open vault.
#[derive(Debug)]
pub struct OpenVault {
    pub name: VaultName,
    pub notes: Arc<Notes>,
    /// Recent vault changes for `GET .../events`.
    pub(crate) events: Arc<VaultEvents>,
    /// Settings set only for this vault (on top of the shared ones).
    pub settings: VaultSettings,
    /// The last API request to this vault.
    last_request: Mutex<Instant>,
    /// How many `GET .../events` requests wait for changes.
    waiting: AtomicUsize,
}

/// How many recent changes a vault keeps for `GET .../events?after=`.
const CHANGE_LOG: usize = 64;

/// Vault changes: a log of recent ones and the number of the last one (a new
/// number wakes the waiting `GET .../events`).
#[derive(Debug)]
pub(crate) struct VaultEvents {
    log: Mutex<ChangeLog>,
    seq: watch::Sender<u64>,
}

#[derive(Debug, Default)]
struct ChangeLog {
    /// The number of the last change (0 - none yet).
    latest: u64,
    /// The number of the last evicted change: a smaller `after` has lost changes.
    dropped: u64,
    items: VecDeque<ChangeEvent>,
}

impl VaultEvents {
    fn new() -> Self {
        Self { log: Mutex::default(), seq: watch::Sender::new(0) }
    }

    fn push(&self, change: ChangeEvent) {
        let seq = change.seq;
        {
            let mut log = lock(&self.log);
            log.latest = seq;
            log.items.push_back(change);
            if log.items.len() > CHANGE_LOG
                && let Some(old) = log.items.pop_front()
            {
                log.dropped = old.seq;
            }
        }
        self.seq.send_replace(seq);
    }

    /// The number of the last change.
    pub(crate) fn latest(&self) -> u64 {
        lock(&self.log).latest
    }

    /// Changes after `after`. Lost ones (older than the log) or a number ahead
    /// (the vault was reopened, the server restarted) give one "check everything".
    pub(crate) fn since(&self, after: u64) -> Vec<ChangeEvent> {
        let log = lock(&self.log);
        if after == log.latest {
            Vec::new()
        } else if after > log.latest || after < log.dropped {
            vec![ChangeEvent { seq: log.latest, paths: Vec::new() }]
        } else {
            log.items.iter().filter(|c| c.seq > after).cloned().collect()
        }
    }
}

/// A `GET .../events` request waits for changes: the vault stays open.
#[derive(Debug)]
pub(crate) struct EventsWait {
    vault: Arc<OpenVault>,
}

impl Drop for EventsWait {
    fn drop(&mut self) {
        self.vault.waiting.fetch_sub(1, Ordering::SeqCst);
        self.vault.touch();
    }
}

impl OpenVault {
    fn new(name: VaultName, notes: Arc<Notes>, schema: &Schema) -> Self {
        let path = notes.dir().map(|d| d.join(notes_core::vaults::SETTINGS_FILE));
        // A broken file is not overwritten: the vault settings stay in memory only.
        let settings = VaultSettings::open(path, schema).unwrap_or_else(|e| {
            tracing::warn!("settings of vault \"{name}\": {e}; going without them for now");
            VaultSettings::in_memory()
        });
        let events = Arc::new(VaultEvents::new());
        let log = events.clone();
        notes.on_change(move |c| log.push(ChangeEvent { seq: c.seq, paths: c.paths.clone() }));
        Self { name, notes, events, settings, last_request: Mutex::new(Instant::now()), waiting: AtomicUsize::new(0) }
    }

    /// Figure processing by the settings of this vault.
    pub fn figure_options(&self, shared: &SettingsStore) -> FigureOptions {
        notes_core::settings::figure_options(&self.settings.merged(shared.values()))
    }

    pub fn touch(&self) {
        *lock(&self.last_request) = Instant::now();
    }

    fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(*lock(&self.last_request))
    }

    fn has_waiting(&self) -> bool {
        self.waiting.load(Ordering::SeqCst) > 0
    }

    /// Waits for changes: a receiver of the number (subscribed before the log is
    /// checked, so no change is lost) and a "waiting" mark while it lives.
    pub(crate) fn wait_events(self: &Arc<Self>) -> (watch::Receiver<u64>, EventsWait) {
        self.waiting.fetch_add(1, Ordering::SeqCst);
        self.touch();
        (self.events.seq.subscribe(), EventsWait { vault: self.clone() })
    }
}

#[derive(Debug)]
enum Source {
    /// One vault opened ahead.
    Single(VaultName),
    /// The vaults of the data directory.
    Registry { vaults: Vaults, config: NotesConfig },
}

/// The server's vaults: where they come from and which are open.
#[derive(Debug)]
pub struct VaultSet {
    source: Source,
    /// The core for what all vaults share: themes, library fonts.
    library: Arc<Notes>,
    /// Shared fonts and themes for the vaults being opened.
    shared: SharedAssets,
    /// After how long without requests and waiting events an inactive vault closes.
    idle_close: Duration,
    settings: Arc<SettingsStore>,
    open: Mutex<BTreeMap<VaultName, Arc<OpenVault>>>,
    /// The vault being warmed.
    active: Mutex<Option<VaultName>>,
    /// The server is running ([`VaultSet::start_background`]): open vaults have
    /// warming and the watcher on.
    background: AtomicBool,
}

/// A lock without poisoning: a panic in another thread does not spoil the data.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl VaultSet {
    /// One vault opened ahead; device settings are applied right away.
    pub fn single(name: VaultName, notes: Arc<Notes>, settings: Arc<SettingsStore>) -> Self {
        let set = Self::with(Source::Single(name.clone()), notes.clone(), settings);
        set.insert(OpenVault::new(name, notes, set.settings.schema()));
        set
    }

    /// The vaults of the data directory (`config.vault` is each one's own
    /// folder); `library` is the core without a vault, for themes and fonts.
    pub fn registry(vaults: Vaults, config: NotesConfig, library: Arc<Notes>, settings: Arc<SettingsStore>) -> Self {
        Self::with(Source::Registry { vaults, config }, library, settings)
    }

    fn with(source: Source, library: Arc<Notes>, settings: Arc<SettingsStore>) -> Self {
        Self {
            source,
            shared: library.shared_assets(),
            library,
            idle_close: IDLE_CLOSE,
            settings,
            open: Mutex::default(),
            active: Mutex::default(),
            background: AtomicBool::new(false),
        }
    }

    /// The core for what all vaults share: themes, library fonts.
    pub fn library(&self) -> &Arc<Notes> {
        &self.library
    }

    pub fn settings(&self) -> &Arc<SettingsStore> {
        &self.settings
    }

    #[must_use]
    pub fn with_idle_close(mut self, idle_close: Duration) -> Self {
        self.idle_close = idle_close;
        self
    }

    pub fn can_create(&self) -> bool {
        matches!(self.source, Source::Registry { .. })
    }

    /// The vault list.
    pub fn describe(&self) -> notes_core::Result<VaultsResponse> {
        let vaults = match &self.source {
            Source::Single(name) => vec![name.clone()],
            Source::Registry { vaults, .. } => vaults.list()?,
        };
        Ok(VaultsResponse { vaults, can_create: self.can_create() })
    }

    /// Creates a new empty vault.
    pub fn create(&self, name: &VaultName) -> notes_core::Result<()> {
        match &self.source {
            Source::Single(_) => Err(notes_core::Error::VaultExists(name.to_string())),
            Source::Registry { vaults, .. } => {
                vaults.create(name)?;
                tracing::info!("vault \"{name}\" created");
                Ok(())
            }
        }
    }

    /// Renames a vault; an open one is closed (the next request opens it under
    /// the new name).
    pub fn rename(&self, from: &str, to: &VaultName) -> notes_core::Result<()> {
        let Source::Registry { vaults, .. } = &self.source else {
            return Err(notes_core::Error::VaultExists(to.to_string()));
        };
        // Under the lock of open vaults: no request opens the vault while it moves.
        let mut open = lock(&self.open);
        let from = vaults.find(from)?;
        Self::close(&mut open, &from);
        vaults.rename(&from, to)?;
        drop(open);
        self.forget_active(&from);
        tracing::info!("vault \"{from}\" renamed to \"{to}\"");
        Ok(())
    }

    /// Moves the whole vault to the system trash; an open one is closed.
    pub fn trash(&self, name: &str) -> notes_core::Result<()> {
        let Source::Registry { vaults, config } = &self.source else {
            return Err(notes_core::Error::VaultNotFound { name: name.to_owned(), known: self.describe()?.vaults });
        };
        let mut open = lock(&self.open);
        let name = vaults.find(name)?;
        Self::close(&mut open, &name);
        vaults.trash(&name, config.trash.as_deref())?;
        drop(open);
        self.forget_active(&name);
        tracing::info!("vault \"{name}\" moved to the trash");
        Ok(())
    }

    /// Closes an open vault: warming and the watcher stop, the event stream
    /// ends (clients reconnect to the new name).
    fn close(open: &mut BTreeMap<VaultName, Arc<OpenVault>>, name: &VaultName) {
        if let Some(vault) = open.remove(name) {
            vault.notes.close();
        }
    }

    fn forget_active(&self, name: &VaultName) {
        let mut active = lock(&self.active);
        if active.as_ref() == Some(name) {
            *active = None;
        }
    }

    /// The vault if it is already open (no blocking work).
    pub fn opened(&self, name: &str) -> Option<Arc<OpenVault>> {
        lock(&self.open).iter().find(|(n, _)| n.as_str() == name).map(|(_, v)| v.clone())
    }

    /// The vault by name; if it is not open yet, opens it (blocking work:
    /// fonts, themes) and makes it active.
    pub fn get(&self, name: &str) -> notes_core::Result<Arc<OpenVault>> {
        if let Some(open) = self.opened(name) {
            return Ok(open);
        }
        let Source::Registry { vaults, config, .. } = &self.source else {
            return Err(notes_core::Error::VaultNotFound { name: name.to_owned(), known: self.describe()?.vaults });
        };
        // Under the lock the whole time: two requests do not open one vault twice.
        let mut open = lock(&self.open);
        let name = vaults.find(name)?;
        if let Some(v) = open.get(&name) {
            return Ok(v.clone());
        }
        let started = Instant::now();
        let notes = Arc::new(Notes::open_with_shared(
            &NotesConfig { vault: vaults.path(&name), ..config.clone() },
            &self.shared,
        )?);
        tracing::info!(ms = started.elapsed().as_millis(), "vault \"{name}\" opened");
        let vault = Arc::new(OpenVault::new(name.clone(), notes, self.settings.schema()));
        open.insert(name.clone(), vault.clone());
        drop(open);
        if self.background.load(Ordering::SeqCst) {
            Self::background(&vault);
        }
        self.activate(&name);
        Ok(vault)
    }

    fn insert(&self, vault: OpenVault) {
        let name = vault.name.clone();
        lock(&self.open).insert(name.clone(), Arc::new(vault));
        self.activate(&name);
    }

    /// Makes the vault active: it is the one warmed, warming is off for the
    /// others. Device settings go to all open vaults.
    pub fn activate(&self, name: &VaultName) {
        let changed = lock(&self.active).replace(name.clone()).as_ref() != Some(name);
        if changed {
            self.apply_device();
        }
    }

    /// Applies the device settings to all open vaults (after a settings
    /// change).
    pub fn apply_device(&self) {
        let device = self.settings.device();
        let active = lock(&self.active).clone();
        for (name, vault) in lock(&self.open).iter() {
            let warm = if Some(name) == active.as_ref() { device.warm } else { WarmMode::Off };
            vault.notes.apply_device(&notes_core::settings::Device { warm, ..device.clone() });
        }
    }

    fn idle_poll(&self) -> Duration {
        self.idle_close.min(IDLE_POLL_MAX).max(IDLE_POLL_MIN)
    }

    /// Closes inactive vaults with no requests and no waiting events for longer
    /// than `idle_close`. A server on one vault (`--vault`) closes nothing.
    fn close_idle_once(&self) {
        if !self.can_create() || self.idle_close.is_zero() {
            return;
        }
        let now = Instant::now();
        let active = lock(&self.active).clone();
        let mut open = lock(&self.open);
        let to_close: Vec<_> = open
            .iter()
            .filter_map(|(name, vault)| {
                if Some(name) == active.as_ref() || vault.has_waiting() {
                    return None;
                }
                (vault.idle_for(now) >= self.idle_close).then(|| name.clone())
            })
            .collect();
        for name in to_close {
            Self::close(&mut open, &name);
            tracing::info!("vault \"{name}\" closed: not in use");
        }
    }

    pub fn close_idle_forever(self: Arc<Self>, closing: &watch::Sender<bool>) {
        let closing = closing.subscribe();
        while !*closing.borrow() {
            std::thread::sleep(self.idle_poll());
            if *closing.borrow() {
                break;
            }
            self.close_idle_once();
        }
    }

    /// The server is running: open vaults (and those opened later) get warming
    /// and the file watcher.
    pub fn start_background(&self) {
        if self.background.swap(true, Ordering::SeqCst) {
            return;
        }
        for vault in lock(&self.open).values() {
            Self::background(vault);
        }
    }

    fn background(vault: &OpenVault) {
        // Notes ahead: all of them, by priority, skipping built ones, into the
        // disk cache (figures are processed on opening, by the settings).
        let notes = vault.notes.clone();
        std::thread::spawn(move || notes.warm_forever());
        // The file watcher: the link index without rescans, warming, client events.
        if vault.notes.watch() {
            tracing::info!("watching the files of vault \"{}\"", vault.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use notes_core::figures::FigureOptions;
    use notes_core::settings::{Platform, Schema};
    use notes_core::storage::MemStorage;
    use notes_core::{LibrarySource, NoteId};

    use super::*;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn config(vault: PathBuf) -> NotesConfig {
        NotesConfig {
            vault,
            library: LibrarySource::Dir(repo().join("baluk")),
            font_dirs: vec![],
            cache: None,
            trash: None,
        }
    }

    fn open_library() -> Arc<Notes> {
        Arc::new(Notes::with_storage(Arc::new(MemStorage::new()), &config(PathBuf::new())).unwrap())
    }

    fn settings(notes: &Notes, dir: &tempfile::TempDir) -> Arc<SettingsStore> {
        let schema = Schema::new(notes.themes().themes(), Platform::Desktop);
        Arc::new(SettingsStore::open(dir.path().join("settings.json"), schema).unwrap())
    }

    fn registry(vaults: &[&str]) -> (VaultSet, tempfile::TempDir, tempfile::TempDir) {
        let data = tempfile::tempdir().unwrap();
        let root = Vaults::new(data.path());
        for name in vaults {
            let dir = root.create(&VaultName::new(*name).unwrap()).unwrap();
            fs::write(dir.join("A.typ"), "= A\ntext").unwrap();
        }
        let cfg_dir = tempfile::tempdir().unwrap();
        let library = open_library();
        let set = VaultSet::registry(root, config(PathBuf::new()), library.clone(), settings(&library, &cfg_dir))
            .with_idle_close(Duration::from_secs(1));
        (set, data, cfg_dir)
    }

    fn age(vault: &OpenVault) {
        *lock(&vault.last_request) = Instant::now().checked_sub(Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn closes_inactive_vault_after_idle() {
        let (set, _data, _cfg_dir) = registry(&["A", "B"]);
        let inactive = set.get("A").unwrap();
        let weak = Arc::downgrade(&inactive.notes);
        let _active = set.get("B").unwrap();
        age(&inactive);
        drop(inactive);

        set.close_idle_once();

        assert!(set.opened("A").is_none());
        assert!(weak.upgrade().is_none(), "the core of a closed vault must be freed");
    }

    #[test]
    fn change_log_answers_since_seq() {
        let events = VaultEvents::new();
        let change = |seq: u64| ChangeEvent { seq, paths: vec![format!("{seq}.typ")] };
        assert!(events.since(0).is_empty());
        events.push(change(2));
        events.push(change(5));
        let seqs = |v: Vec<ChangeEvent>| v.iter().map(|c| c.seq).collect::<Vec<_>>();
        assert_eq!(seqs(events.since(0)), [2, 5]);
        assert_eq!(seqs(events.since(2)), [5]);
        assert!(events.since(5).is_empty());
        // A number ahead: the vault was reopened, "check everything".
        assert_eq!(events.since(9)[0].paths, Vec::<String>::new());
        for seq in 6..6 + CHANGE_LOG as u64 {
            events.push(change(seq));
        }
        // 2 and 5 are evicted: changes since 2 are lost, since 5 all are there.
        assert!(events.since(2)[0].paths.is_empty());
        assert_eq!(events.since(5).len(), CHANGE_LOG);
        assert_eq!(events.latest(), 5 + CHANGE_LOG as u64);
    }

    #[test]
    fn does_not_close_while_events_wait() {
        let (set, _data, _cfg_dir) = registry(&["A", "B"]);
        let inactive = set.get("A").unwrap();
        let _active = set.get("B").unwrap();
        let (_rx, wait) = inactive.wait_events();
        age(&inactive);

        set.close_idle_once();
        assert!(set.opened("A").is_some(), "a vault must not close while an events request waits");

        drop(wait);
        let inactive = set.opened("A").unwrap();
        age(&inactive);
        drop(inactive);
        set.close_idle_once();
        assert!(set.opened("A").is_none());
    }

    #[test]
    fn keeps_active_vault_even_if_idle() {
        let (set, _data, _cfg_dir) = registry(&["A"]);
        let active = set.get("A").unwrap();
        age(&active);
        drop(active);

        set.close_idle_once();

        assert!(set.opened("A").is_some(), "the active vault does not close");
    }

    #[test]
    fn reopens_closed_vault_on_next_request() {
        let (set, _data, _cfg_dir) = registry(&["A", "B"]);
        let first = set.get("A").unwrap();
        let weak = Arc::downgrade(&first.notes);
        let _active = set.get("B").unwrap();
        age(&first);
        drop(first);

        set.close_idle_once();
        assert!(set.opened("A").is_none());
        assert!(weak.upgrade().is_none());

        let reopened = set.get("A").unwrap();
        let page = reopened.notes.page(&NoteId::new("A").unwrap(), FigureOptions::default()).unwrap();
        assert!(
            page.rendered.as_ref().is_some_and(|r| r.body.contains("text")),
            "the content is the same after reopening"
        );
    }

    #[test]
    fn never_closes_single_vault_mode() {
        let vault = tempfile::tempdir().unwrap();
        fs::write(vault.path().join("A.typ"), "= A\ntext").unwrap();
        let notes = Arc::new(Notes::open(&config(vault.path().to_path_buf())).unwrap());
        let cfg_dir = tempfile::tempdir().unwrap();
        let set = VaultSet::single(VaultName::new("single").unwrap(), notes.clone(), settings(&notes, &cfg_dir))
            .with_idle_close(Duration::from_secs(1));

        let open = set.opened("single").unwrap();
        age(&open);
        drop(open);
        *lock(&set.active) = None;
        set.close_idle_once();

        assert!(set.opened("single").is_some(), "in --vault mode the core does not close");
    }
}
