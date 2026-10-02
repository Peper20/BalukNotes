//! Хранилища: список и создание (`GET`/`POST /api/vaults`), переименование
//! и удаление в корзину (`PATCH`/`DELETE /api/vaults/{хранилище}`) и
//! хранилища, открытые сервером.
//!
//! API заметок — под `/api/vaults/{хранилище}/…` (модули `notes`, `graph`,
//! `search`, `events`). Хранилище открывается при первом обращении: у
//! каждого своё ядро (`Notes`: кэш, индекс ссылок, наблюдатель, прогрев) и
//! свой поток событий. Прогревается только **активное** — открытое последним
//! или то, которому клиент последним подсказал прогрев (`POST …/warm`, при
//! запуске клиента): остальные собирают заметки по запросу. Неактивные
//! хранилища без запросов и потоков событий сервер закрывает через
//! [`IDLE_CLOSE`]: следующий запрос откроет заново.
//!
//! Источник хранилищ — каталог данных (`notes_core::vaults`, по запросу;
//! хранилищ может не быть вовсе — первое создают в клиенте) или одно
//! хранилище, открытое заранее (`notes serve --vault …`, тесты): тогда
//! новые не создаются, а это не переименовывается и не удаляется. Темы и шрифты у всех общие (библиотека одна) — их
//! отдаёт ядро без хранилища ([`VaultSet::library`]).

use std::collections::BTreeMap;
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
use tokio::sync::{broadcast, watch};

use crate::AppState;
use crate::api::{ChangeEvent, CreateVaultRequest, RenameVaultRequest, VaultsResponse};
use crate::error::{ApiError, ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/vaults", get(list).post(create))
        .route("/api/vaults/{vault}", patch(rename).delete(remove))
}

/// Через сколько неактивное хранилище закрывается сервером.
pub const IDLE_CLOSE: Duration = Duration::from_secs(600);

/// Как часто сервер проверяет, не пора ли закрыть неактивные.
const IDLE_POLL_MAX: Duration = Duration::from_secs(30);
const IDLE_POLL_MIN: Duration = Duration::from_millis(100);

/// Хранилища каталога данных; сервер на одном хранилище — 403.
fn registry_only(s: &AppState) -> ApiResult<()> {
    if s.vaults.can_create() {
        Ok(())
    } else {
        Err(ApiError(StatusCode::FORBIDDEN, "сервер открыт на одном хранилище (--vault <путь>)".into()))
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

/// Открытое хранилище.
#[derive(Debug)]
pub struct OpenVault {
    pub name: VaultName,
    pub notes: Arc<Notes>,
    /// Изменения хранилища для `GET …/events`.
    pub events: broadcast::Sender<ChangeEvent>,
    /// Настройки, заданные только для этого хранилища (поверх общих).
    pub settings: VaultSettings,
    /// Последний запрос к API этого хранилища.
    last_request: Mutex<Instant>,
    /// Сколько открытых `GET …/events` держат SSE-поток.
    streams: AtomicUsize,
}

#[derive(Debug)]
pub(crate) struct EventStreamGuard {
    vault: Arc<OpenVault>,
}

impl Drop for EventStreamGuard {
    fn drop(&mut self) {
        self.vault.streams.fetch_sub(1, Ordering::SeqCst);
        self.vault.touch();
    }
}

impl OpenVault {
    fn new(name: VaultName, notes: Arc<Notes>, schema: &Schema) -> Self {
        let path = notes.dir().map(|d| d.join(notes_core::vaults::SETTINGS_FILE));
        // Испорченный файл не перезаписываем: настройки хранилища — только в памяти.
        let settings = VaultSettings::open(path, schema).unwrap_or_else(|e| {
            tracing::warn!("настройки хранилища «{name}»: {e} — пока без них");
            VaultSettings::open(None, schema).expect("без файла — без ошибок")
        });
        let (events, _) = broadcast::channel(64);
        let tx = events.clone();
        notes.on_change(move |c| {
            // Нет слушателей — не страшно.
            let _ = tx.send(ChangeEvent { seq: c.seq, paths: c.paths.clone() });
        });
        Self { name, notes, events, settings, last_request: Mutex::new(Instant::now()), streams: AtomicUsize::new(0) }
    }

    /// Обработка рисунков по настройкам этого хранилища.
    pub fn figure_options(&self, shared: &SettingsStore) -> FigureOptions {
        notes_core::settings::figure_options(&self.settings.merged(shared.values()))
    }

    pub fn touch(&self) {
        *lock(&self.last_request) = Instant::now();
    }

    fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(*lock(&self.last_request))
    }

    fn has_streams(&self) -> bool {
        self.streams.load(Ordering::SeqCst) > 0
    }

    pub(crate) fn subscribe_events(self: &Arc<Self>) -> (broadcast::Receiver<ChangeEvent>, EventStreamGuard) {
        self.streams.fetch_add(1, Ordering::SeqCst);
        self.touch();
        (self.events.subscribe(), EventStreamGuard { vault: self.clone() })
    }
}

#[derive(Debug)]
enum Source {
    /// Одно хранилище, открытое заранее.
    Single(VaultName),
    /// Хранилища каталога данных.
    Registry { vaults: Vaults, config: NotesConfig },
}

/// Хранилища сервера: откуда берутся и какие открыты.
#[derive(Debug)]
pub struct VaultSet {
    source: Source,
    /// Ядро для общего у всех хранилищ: темы, шрифты библиотеки.
    library: Arc<Notes>,
    /// Общие шрифты и темы для открываемых хранилищ.
    shared: SharedAssets,
    /// Через сколько без запросов и SSE-подписок закрывать неактивное.
    idle_close: Duration,
    settings: Arc<SettingsStore>,
    open: Mutex<BTreeMap<VaultName, Arc<OpenVault>>>,
    /// Хранилище, которое прогревается.
    active: Mutex<Option<VaultName>>,
    /// Сервер запущен ([`VaultSet::start_background`]): у открытых хранилищ
    /// работают прогрев и наблюдатель.
    background: AtomicBool,
}

/// Замок без «отравления»: паника в другом потоке данные не портит.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl VaultSet {
    /// Одно хранилище, открытое заранее; настройки устройства сразу применяются.
    pub fn single(name: VaultName, notes: Arc<Notes>, settings: Arc<SettingsStore>) -> Self {
        let set = Self::with(Source::Single(name.clone()), notes.clone(), settings);
        set.insert(OpenVault::new(name, notes, set.settings.schema()));
        set
    }

    /// Хранилища каталога данных (`config.vault` у каждого — своя папка);
    /// `library` — ядро без хранилища для тем и шрифтов.
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

    /// Ядро для общего у всех хранилищ: темы, шрифты библиотеки.
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

    /// Список хранилищ.
    pub fn describe(&self) -> notes_core::Result<VaultsResponse> {
        let vaults = match &self.source {
            Source::Single(name) => vec![name.clone()],
            Source::Registry { vaults, .. } => vaults.list()?,
        };
        Ok(VaultsResponse { vaults, can_create: self.can_create() })
    }

    /// Создать новое пустое хранилище.
    pub fn create(&self, name: &VaultName) -> notes_core::Result<()> {
        match &self.source {
            Source::Single(_) => Err(notes_core::Error::VaultExists(name.to_string())),
            Source::Registry { vaults, .. } => {
                vaults.create(name)?;
                tracing::info!("создано хранилище «{name}»");
                Ok(())
            }
        }
    }

    /// Переименовать хранилище: открытое — закрыть (следующий запрос
    /// откроет его под новым именем).
    pub fn rename(&self, from: &str, to: &VaultName) -> notes_core::Result<()> {
        let Source::Registry { vaults, .. } = &self.source else {
            return Err(notes_core::Error::VaultExists(to.to_string()));
        };
        // Под замком открытых: запрос не откроет хранилище, пока оно переезжает.
        let mut open = lock(&self.open);
        let from = vaults.find(from)?;
        Self::close(&mut open, &from);
        vaults.rename(&from, to)?;
        drop(open);
        self.forget_active(&from);
        tracing::info!("хранилище «{from}» переименовано в «{to}»");
        Ok(())
    }

    /// Хранилище целиком — в корзину системы; открытое — закрыть.
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
        tracing::info!("хранилище «{name}» удалено в корзину");
        Ok(())
    }

    /// Закрыть открытое хранилище: прогрев и наблюдатель — стоп, поток
    /// событий кончается (клиенты переподключатся к новому имени).
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

    /// Открытое хранилище, если уже открыто (без блокирующей работы).
    pub fn opened(&self, name: &str) -> Option<Arc<OpenVault>> {
        lock(&self.open).iter().find(|(n, _)| n.as_str() == name).map(|(_, v)| v.clone())
    }

    /// Хранилище по имени; ещё не открыто — открыть (блокирующая работа:
    /// шрифты, темы) и сделать активным.
    pub fn get(&self, name: &str) -> notes_core::Result<Arc<OpenVault>> {
        if let Some(open) = self.opened(name) {
            return Ok(open);
        }
        let Source::Registry { vaults, config, .. } = &self.source else {
            return Err(notes_core::Error::VaultNotFound { name: name.to_owned(), known: self.describe()?.vaults });
        };
        // Под замком целиком: два запроса не откроют одно хранилище дважды.
        let mut open = lock(&self.open);
        let name = vaults.find(name)?;
        if let Some(v) = open.get(&name) {
            return Ok(v.clone());
        }
        let started = std::time::Instant::now();
        let notes = Arc::new(Notes::open_with_shared(
            &NotesConfig { vault: vaults.path(&name), ..config.clone() },
            &self.shared,
        )?);
        tracing::info!(ms = started.elapsed().as_millis(), "открыто хранилище «{name}»");
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

    /// Сделать хранилище активным: прогревается оно, у остальных прогрев
    /// выключен. Настройки устройства — ко всем открытым.
    pub fn activate(&self, name: &VaultName) {
        let changed = lock(&self.active).replace(name.clone()).as_ref() != Some(name);
        if changed {
            self.apply_device();
        }
    }

    /// Применить настройки устройства ко всем открытым хранилищам (после
    /// изменения настроек).
    pub fn apply_device(&self) {
        let device = self.settings.device();
        let active = lock(&self.active).clone();
        for (name, vault) in lock(&self.open).iter() {
            let warm = if Some(name) == active.as_ref() { device.warm } else { WarmMode::Off };
            vault.notes.apply_device(&notes_core::settings::Device { warm, ..device });
        }
    }

    fn idle_poll(&self) -> Duration {
        self.idle_close.min(IDLE_POLL_MAX).max(IDLE_POLL_MIN)
    }

    /// Закрыть неактивные хранилища без запросов и потоков событий дольше
    /// `idle_close`. Сервер на одном хранилище (`--vault`) не закрывает.
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
                if Some(name) == active.as_ref() || vault.has_streams() {
                    return None;
                }
                (vault.idle_for(now) >= self.idle_close).then(|| name.clone())
            })
            .collect();
        for name in to_close {
            Self::close(&mut open, &name);
            tracing::info!("хранилище «{name}» закрыто: не используется");
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

    /// Сервер запущен: у открытых (и открываемых потом) хранилищ —
    /// прогрев и наблюдатель файлов; шрифты для браузера — заранее.
    pub fn start_background(&self) {
        if self.background.swap(true, Ordering::SeqCst) {
            return;
        }
        for vault in lock(&self.open).values() {
            Self::background(vault);
        }
    }

    fn background(vault: &OpenVault) {
        // Заметки — заранее: все, по приоритету, пропуская собранные, в
        // кэш на диске (рисунки обрабатываются при открытии — по настройкам).
        let notes = vault.notes.clone();
        std::thread::spawn(move || notes.warm_forever());
        // Наблюдатель файлов: индекс ссылок без обходов, прогрев и события клиенту.
        if vault.notes.watch() {
            tracing::info!("слежу за файлами хранилища «{}»", vault.name);
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
        assert!(weak.upgrade().is_none(), "ядро закрытого хранилища должно освободиться");
    }

    #[test]
    fn does_not_close_while_sse_stream_is_open() {
        let (set, _data, _cfg_dir) = registry(&["A", "B"]);
        let inactive = set.get("A").unwrap();
        let _active = set.get("B").unwrap();
        let (_rx, stream) = inactive.subscribe_events();
        age(&inactive);

        set.close_idle_once();
        assert!(set.opened("A").is_some(), "пока SSE-подписка открыта, закрывать нельзя");

        drop(stream);
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

        assert!(set.opened("A").is_some(), "активное хранилище не закрывается");
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
        assert!(page.rendered.as_ref().is_some_and(|r| r.body.contains("text")), "содержимое после переоткрытия то же");
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

        assert!(set.opened("single").is_some(), "в режиме --vault ядро не закрывается");
    }
}
