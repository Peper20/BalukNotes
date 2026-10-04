//! Прогрев: все заметки собираются заранее, в фоне, чтобы открывались сразу.
//!
//! Работает поверх [`Pages`] — только через его открытые методы.
//!
//! - **Порядок** ([`order`]): сначала подсказанные клиентом (открытые во
//!   вкладках, недавние); затем ещё не собиравшиеся заметки (от маленьких
//!   к большим — новая заметка, скорее всего, та, что сейчас пишут);
//!   затем собиравшиеся — по времени прошлой сборки (оно в кэше на диске:
//!   время решают рисунки CeTZ, а не байты); книги, которые ещё не
//!   собирались, — последними.
//! - **Только на диск**: собранное пишется в кэш на диске и в памяти не
//!   держится ([`Pages::prebuild`]) — память занимают лишь открытые
//!   заметки. Без кэша на диске — в памяти, в пределе
//!   [`crate::page_cache::MEMORY_BUDGET`].
//! - **Не дважды**: заметка, у которой есть сборка для текущих файлов (в том
//!   числе с ошибкой), пропускается — собранное в прошлый запуск не
//!   собирается снова.
//! - **Пользователь — вне очереди**: сборки идут по одной, и прогрев не
//!   берёт следующую заметку, пока кто-то ждёт страницу. Уже начатую сборку
//!   он не прерывает (см. `docs/tech-debt.md`).
//! - **Бережно**: сборка прогрева — темы по очереди в одном потоке с
//!   пониженным приоритетом ([`crate::world::Priority::Background`]). После
//!   большого прохода (запуск, правка библиотеки) память Typst
//!   освобождается — редко ([`should_release`]): обычная правка заметки её
//!   не трогает, иначе пересборка после правки потеряла бы memo.
//! - **Режим** — настройка устройства ([`WarmMode`]): всё хранилище, только
//!   подсказанное клиентом (открытое) или выключен.
//!
//! Проход повторяется по новой подсказке, по изменению файлов хранилища
//! ([`Warmer::poke`] от наблюдателя, [`crate::watch`]) и на всякий случай раз
//! в [`RESCAN`] (без наблюдателя — раз в [`RESCAN_UNWATCHED`]): новые и
//! изменённые заметки тоже собираются заранее. Проверка неизменившейся
//! заметки — несколько `stat`. При запуске фонового потока кэш на диске
//! чистится ([`crate::cache::DiskCache::prune`]).

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use crate::pages::Pages;
use crate::vault::{Entry, NoteId, NoteKind};

/// Как часто проверять, не появилось ли несобранное, если наблюдатель файлов
/// работает (страховка от потерянного события).
pub const RESCAN: Duration = Duration::from_secs(600);

/// То же без наблюдателя файлов.
pub const RESCAN_UNWATCHED: Duration = Duration::from_secs(60);

/// Что прогревать (настройка устройства).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum WarmMode {
    /// Всё хранилище (компьютер).
    #[default]
    All = 0,
    /// Только подсказанное клиентом: заметки во вкладках, недавние.
    Open = 1,
    /// Ничего: заметки собираются, когда их открыли (телефон).
    Off = 2,
}

impl WarmMode {
    const ALL: [Self; 3] = [Self::All, Self::Open, Self::Off];

    /// Значение настройки: `all`, `open`, `off`.
    pub fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Open => "open",
            Self::Off => "off",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    /// The mode stored in [`Warmer`] as its discriminant; an unknown value is the default.
    fn from_u8(value: u8) -> Self {
        Self::ALL.into_iter().find(|m| *m as u8 == value).unwrap_or_default()
    }
}

/// Освобождать память после прохода, собравшего хотя бы столько заметок.
pub const RELEASE_MIN_BUILT: usize = 5;

/// И не чаще, чем раз в столько.
pub const RELEASE_EVERY: Duration = Duration::from_secs(600);

/// Освободить ли память Typst после прохода: только большой проход и не
/// чаще [`RELEASE_EVERY`] (`last` — прошлое освобождение).
pub fn should_release(stats: WarmStats, last: Option<Instant>, now: Instant) -> bool {
    stats.built >= RELEASE_MIN_BUILT && last.is_none_or(|t| now.duration_since(t) >= RELEASE_EVERY)
}

/// Прогрев: подсказки клиента и будильник фонового потока.
#[derive(Debug, Default)]
pub struct Warmer {
    hints: Mutex<Vec<NoteId>>,
    /// [`WarmMode`] as its discriminant.
    mode: AtomicU8,
    /// Растёт с каждой подсказкой: проход начинается заново.
    generation: AtomicU64,
    wake: Condvar,
    /// Хранилище закрыто ([`Warmer::stop`]): фоновый поток выходит.
    stopped: AtomicBool,
}

/// Итог одного прохода.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WarmStats {
    pub built: usize,
    pub skipped: usize,
}

/// Порядок сборки: подсказки (в их порядке); затем не собиравшиеся заметки
/// по размеру исходников; затем собиравшиеся по времени прошлой сборки;
/// затем не собиравшиеся книги по размеру.
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
    /// Подсказать, что собрать первым (заметки во вкладках, недавние).
    pub fn hint(&self, ids: Vec<NoteId>) {
        // Под замком: фоновый поток сверяет поколение под ним же перед сном.
        let mut hints = self.hints.lock();
        *hints = ids;
        self.generation.fetch_add(1, Ordering::SeqCst);
        drop(hints);
        self.wake.notify_all();
    }

    pub fn mode(&self) -> WarmMode {
        WarmMode::from_u8(self.mode.load(Ordering::SeqCst))
    }

    /// Сменить режим; изменился — пройти заново.
    pub fn set_mode(&self, mode: WarmMode) {
        let value = mode as u8;
        if self.mode.swap(value, Ordering::SeqCst) != value {
            self.poke();
        }
    }

    /// Остановить прогрев навсегда (хранилище закрыто): проход кончается,
    /// фоновый поток ([`Warmer::forever`]) выходит.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.set_mode(WarmMode::Off);
        self.poke();
    }

    /// Файлы хранилища изменились: пройти заново (подсказки те же).
    pub fn poke(&self) {
        let hints = self.hints.lock();
        self.generation.fetch_add(1, Ordering::SeqCst);
        drop(hints);
        self.wake.notify_all();
    }

    /// Один проход: собрать всё несобранное по порядку (в режиме
    /// [`WarmMode::Open`] — только подсказанное). Новая подсказка — проход
    /// начинается заново (собранное пропустится).
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
                    // Пока ждали, заметку собрал запрос пользователя.
                    Ok(false) => stats.skipped += 1,
                    Err(e) => tracing::debug!(%id, "прогрев: {e}"),
                }
            }
            return stats;
        }
    }

    /// Прогревать бесконечно (фоновый поток сервера). Сначала — чистка
    /// кэша на диске. `rescan` — сколько спать без подсказок и изменений.
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
                    "прогрев: заметки собраны заранее"
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

/// Чистка кэша на диске: записи заметок, которых больше нет, и лишнее.
pub fn prune(pages: &Pages) {
    let Ok(entries) = pages.vault().entries() else { return };
    let alive = |id: &str| entries.iter().any(|e| e.id.as_str() == id);
    if let Some(p) = pages.cache().prune(&alive) {
        tracing::info!(removed = p.removed, bytes = p.bytes, "кэш на диске почищен");
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
        assert_eq!(pages.cache().memory(), (0, 0), "прогрев не держит страницы в памяти");
        assert_eq!(warmer.pass(&pages), WarmStats { built: 0, skipped: 3 });

        // Новый запуск: всё на диске, и заметка с ошибкой тоже.
        let (restarted, _) = s.pages(true, Duration::ZERO);
        assert_eq!(Warmer::default().pass(&restarted), WarmStats { built: 0, skipped: 3 });

        // Правка — собирается только она.
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
        assert_eq!(warmer.pass(&pages), WarmStats::default(), "выключен");
        warmer.set_mode(WarmMode::Open);
        assert_eq!(warmer.pass(&pages), WarmStats { built: 1, skipped: 0 }, "только подсказанное");
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
        assert!(should_release(big, None, t), "первый большой проход (запуск)");
        assert!(!should_release(one, None, t), "правка одной заметки — нет");
        assert!(!should_release(big, Some(t), t + RELEASE_EVERY / 2), "не чаще раза в RELEASE_EVERY");
        assert!(should_release(big, Some(t), t + RELEASE_EVERY));
    }

    #[test]
    fn poke_restarts_pass() {
        let warmer = Warmer::default();
        let before = warmer.generation.load(Ordering::SeqCst);
        warmer.hint(vec![NoteId::new("A").unwrap()]);
        warmer.poke();
        assert_eq!(warmer.generation.load(Ordering::SeqCst), before + 2);
        assert_eq!(warmer.hints.lock().len(), 1, "подсказки те же");
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
