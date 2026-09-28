//! Изменения хранилища: наблюдатель файлов вместо обходов по таймеру.
//!
//! [`Changes`] включает наблюдатель хранилища ([`Storage::watch`]) и раздаёт
//! изменения тем, кому они нужны:
//!
//! - **индекс ссылок** ([`crate::graph::SourceIndex`]) не обходит хранилище,
//!   пока [`Changes::seq`] тот же (счётчик растёт сразу при событии);
//! - **прогрев** ([`crate::warm`]) просыпается от изменения, а не раз в минуту;
//! - **сервер** шлёт клиенту событие (`GET /api/events`): клиент сверяет версию
//!   заметки, а не опрашивает раз в N секунд.
//!
//! Слушатели получают изменения пачкой, после паузы [`SETTLE`]: редактор
//! сохраняет файл в несколько шагов. Версии заметок по-прежнему — `stat` их
//! файлов (несколько вызовов на заметку, см. [`crate::version`]).
//!
//! Кроме хранилища, можно наблюдать и другой каталог ([`Changes::also`]):
//! библиотеку оформления на диске (`/_baluk/` отладочной сборки) — её пути
//! приходят с префиксом (`_baluk/theme.typ`).
//!
//! Наблюдатель — ускорение, а не источник правды: сломался (переполнение
//! очереди, сетевой диск) — [`Changes::watching`] станет ложью, и всё
//! работает как без него, обходом.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::Mutex;

use crate::storage::{ChangeSink, Storage, WatchGuard};

/// Сколько ждать тишины, прежде чем раздать пачку изменений.
pub const SETTLE: Duration = Duration::from_millis(100);

/// Пачка изменений.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// [`Changes::seq`] после этой пачки.
    pub seq: u64,
    /// Изменившиеся файлы (пути хранилища, без повторов). Пусто — неизвестно
    /// что (наблюдатель сломался): проверить всё.
    pub paths: Vec<String>,
}

type Listener = Box<dyn Fn(&Change) + Send + Sync>;

/// Изменения хранилища: счётчик и слушатели.
#[derive(Default)]
pub struct Changes {
    seq: AtomicU64,
    watching: AtomicBool,
    listeners: Mutex<Vec<Listener>>,
    guards: Mutex<Vec<WatchGuard>>,
    /// Куда наблюдатели пишут события (после [`Changes::start`]).
    sink: Mutex<Option<ChangeSink>>,
}

impl std::fmt::Debug for Changes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Changes").field("seq", &self.seq()).field("watching", &self.watching()).finish_non_exhaustive()
    }
}

impl Changes {
    /// Включить наблюдатель. `false` — хранилище не умеет (или не вышло:
    /// причина — в журнале); всё работает обходом.
    pub fn start(self: &Arc<Self>, storage: &dyn Storage) -> bool {
        let (tx, rx) = mpsc::channel::<Option<Vec<String>>>();
        let this = Arc::downgrade(self);
        let sink: ChangeSink = Arc::new(move |paths: Option<Vec<String>>| {
            if let Some(this) = this.upgrade() {
                // Сразу: индекс не должен отдать прежний список после события.
                this.seq.fetch_add(1, Ordering::SeqCst);
                if paths.is_none() {
                    this.watching.store(false, Ordering::SeqCst);
                }
            }
            let _ = tx.send(paths);
        });
        match storage.watch(sink.clone()) {
            Ok(Some(guard)) => {
                self.guards.lock().push(guard);
                *self.sink.lock() = Some(sink);
                self.watching.store(true, Ordering::SeqCst);
                let this = Arc::downgrade(self);
                let spawned = std::thread::Builder::new().name("notes-watch".into()).spawn(move || settle(&rx, &this));
                if let Err(e) = spawned {
                    tracing::warn!("наблюдатель хранилища: {e}");
                    self.stop();
                    return false;
                }
                true
            }
            Ok(None) => false,
            Err(e) => {
                tracing::warn!("наблюдатель хранилища не запустился: {e}; дальше — обход файлов");
                false
            }
        }
    }

    /// Наблюдать ещё и `storage` (после [`Self::start`]): его пути приходят
    /// с префиксом `prefix` (`_baluk`). Не вышло — только предупреждение:
    /// изменения там увидит сверка версий по кнопке и при возврате в окно.
    pub fn also(&self, storage: &dyn Storage, prefix: &str) -> bool {
        let Some(inner) = self.sink.lock().clone() else { return false };
        let prefix = prefix.to_owned();
        let sink: ChangeSink = Arc::new(move |paths: Option<Vec<String>>| {
            inner(paths.map(|paths| paths.into_iter().map(|p| format!("{prefix}/{p}")).collect()));
        });
        match storage.watch(sink) {
            Ok(Some(guard)) => {
                self.guards.lock().push(guard);
                true
            }
            Ok(None) => false,
            Err(e) => {
                tracing::warn!("наблюдатель {}: {e}", storage.display("").display());
                false
            }
        }
    }

    /// Выключить наблюдатель.
    pub fn stop(&self) {
        self.watching.store(false, Ordering::SeqCst);
        self.guards.lock().clear();
        self.sink.lock().take();
    }

    /// Наблюдатель работает: пока [`Self::seq`] тот же, файлы не менялись.
    pub fn watching(&self) -> bool {
        self.watching.load(Ordering::SeqCst)
    }

    /// Счётчик изменений: растёт с каждым событием.
    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    /// Звать `f` с каждой пачкой изменений (из потока наблюдателя).
    pub fn subscribe(&self, f: impl Fn(&Change) + Send + Sync + 'static) {
        self.listeners.lock().push(Box::new(f));
    }

    fn publish(&self, change: &Change) {
        for f in self.listeners.lock().iter() {
            f(change);
        }
    }
}

/// Поток наблюдателя: копит события до паузы [`SETTLE`] и раздаёт пачку.
fn settle(rx: &mpsc::Receiver<Option<Vec<String>>>, changes: &Weak<Changes>) {
    while let Ok(first) = rx.recv() {
        let mut paths: Option<Vec<String>> = first;
        while let Ok(more) = rx.recv_timeout(SETTLE) {
            match (&mut paths, more) {
                (Some(all), Some(more)) => all.extend(more),
                (all, _) => *all = None,
            }
        }
        let Some(changes) = changes.upgrade() else { return };
        let mut paths = paths.unwrap_or_default();
        paths.sort();
        paths.dedup();
        tracing::debug!(files = paths.len(), "изменения хранилища");
        changes.publish(&Change { seq: changes.seq(), paths });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemStorage;

    #[test]
    fn batches_changes_and_bumps_seq_at_once() {
        let storage = MemStorage::new();
        let changes = Arc::new(Changes::default());
        assert!(!changes.watching());
        assert!(changes.start(&storage));
        assert!(changes.watching());
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        changes.subscribe(move |c| tx.lock().send(c.clone()).unwrap());

        storage.write("b.typ", "1");
        storage.write("a.typ", "1");
        storage.write("b.typ", "2");
        assert_eq!(changes.seq(), 3, "счётчик — сразу, до пачки");
        let change = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(change, Change { seq: 3, paths: vec!["a.typ".into(), "b.typ".into()] });

        // Второй каталог — пути с префиксом, в ту же пачку.
        let library = MemStorage::new();
        assert!(changes.also(&library, "_baluk"));
        library.write("theme.typ", "1");
        storage.write("c.typ", "1");
        let change = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(change, Change { seq: 5, paths: vec!["_baluk/theme.typ".into(), "c.typ".into()] });

        changes.stop();
        assert!(!changes.watching());
        assert!(!changes.also(&library, "_baluk"), "после остановки — нет");
    }
}
