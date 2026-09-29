//! Хранилища: список и создание (`GET`/`POST /api/vaults`) и хранилища,
//! открытые сервером.
//!
//! API заметок — под `/api/vaults/{хранилище}/…` (модули `notes`, `graph`,
//! `search`, `events`). Хранилище открывается при первом обращении: у
//! каждого своё ядро (`Notes`: кэш, индекс ссылок, наблюдатель, прогрев) и
//! свой поток событий. Прогревается только **активное** — открытое
//! последним или то, которому клиент последним подсказал прогрев (`POST
//! …/warm`, при запуске клиента): остальные собирают заметки по запросу.
//!
//! Источник хранилищ — каталог данных (`notes_core::vaults`, по запросу;
//! хранилищ может не быть вовсе — первое создают в клиенте) или одно
//! хранилище, открытое заранее (`notes serve --vault …`, тесты): тогда
//! новые не создаются. Темы и шрифты у всех общие (библиотека одна) — их
//! отдаёт ядро без хранилища ([`VaultSet::library`]).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use notes_core::settings::SettingsStore;
use notes_core::warm::WarmMode;
use notes_core::{Notes, NotesConfig, VaultName, Vaults};
use tokio::sync::broadcast;

use crate::AppState;
use crate::api::{ChangeEvent, CreateVaultRequest, VaultsResponse};
use crate::error::{ApiError, ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/vaults", get(list).post(create))
}

async fn list(State(s): State<AppState>) -> ApiResult<Json<VaultsResponse>> {
    let vaults = s.vaults.clone();
    Ok(Json(blocking(move || vaults.describe()).await?))
}

async fn create(
    State(s): State<AppState>,
    Json(req): Json<CreateVaultRequest>,
) -> ApiResult<(StatusCode, Json<VaultsResponse>)> {
    if !s.vaults.can_create() {
        return Err(ApiError(StatusCode::FORBIDDEN, "сервер открыт на одном хранилище (--vault <путь>)".into()));
    }
    let vaults = s.vaults.clone();
    let list = blocking(move || {
        vaults.create(&VaultName::new(req.name)?)?;
        vaults.describe()
    })
    .await?;
    Ok((StatusCode::CREATED, Json(list)))
}

/// Открытое хранилище.
#[derive(Debug)]
pub struct OpenVault {
    pub name: VaultName,
    pub notes: Arc<Notes>,
    /// Изменения хранилища для `GET …/events`.
    pub events: broadcast::Sender<ChangeEvent>,
}

impl OpenVault {
    fn new(name: VaultName, notes: Arc<Notes>) -> Self {
        let (events, _) = broadcast::channel(64);
        let tx = events.clone();
        notes.on_change(move |c| {
            // Нет слушателей — не страшно.
            let _ = tx.send(ChangeEvent { seq: c.seq, paths: c.paths.clone() });
        });
        Self { name, notes, events }
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
        set.insert(OpenVault::new(name, notes));
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
            library,
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
        let notes = Arc::new(Notes::open(&NotesConfig { vault: vaults.path(&name), ..config.clone() })?);
        tracing::info!(ms = started.elapsed().as_millis(), "открыто хранилище «{name}»");
        let vault = Arc::new(OpenVault::new(name.clone(), notes));
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
