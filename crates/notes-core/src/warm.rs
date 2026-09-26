//! Прогрев: все заметки собираются заранее, в фоне, чтобы открывались сразу.
//!
//! - **Порядок** ([`order`]): сначала подсказанные клиентом (открытые во
//!   вкладках, недавние), затем заметки от маленьких к большим, книги —
//!   последними (они собираются секундами).
//! - **Не дважды**: заметка, у которой в памяти или в кэше на диске есть
//!   запись для текущих файлов, пропускается — собранное в прошлый запуск
//!   не собирается снова ([`crate::cache::DiskCache::is_fresh`]).
//! - **Пользователь — вне очереди**: сборки идут по одной, и прогрев не
//!   берёт следующую заметку, пока кто-то ждёт страницу ([`Notes::page`]).
//!   Уже начатую сборку он не прерывает (см. `docs/tech-debt.md`).
//!
//! Проход повторяется раз в [`RESCAN`] и по новой подсказке: новые и
//! изменённые заметки тоже собираются заранее. Проверка неизменившейся
//! заметки — несколько `stat`.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use crate::figures::FigureOptions;
use crate::notes::Notes;
use crate::vault::{Entry, NoteId, NoteKind};

/// Как часто проверять, не появилось ли несобранное.
pub const RESCAN: Duration = Duration::from_secs(60);

/// Подсказки клиента и будильник фонового потока.
#[derive(Debug, Default)]
pub(crate) struct WarmQueue {
    hints: Mutex<Vec<NoteId>>,
    /// Растёт с каждой подсказкой: проход начинается заново.
    generation: AtomicU64,
    wake: Condvar,
}

impl WarmQueue {
    pub(crate) fn hint(&self, ids: Vec<NoteId>) {
        // Под замком: фоновый поток сверяет поколение под ним же перед сном.
        let mut hints = self.hints.lock();
        *hints = ids;
        self.generation.fetch_add(1, Ordering::SeqCst);
        drop(hints);
        self.wake.notify_all();
    }
}

/// Итог одного прохода.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WarmStats {
    pub built: usize,
    pub skipped: usize,
}

/// Порядок сборки: подсказки (в их порядке), затем заметки по размеру
/// исходников, затем книги по размеру.
pub fn order(entries: &[Entry], size: impl Fn(&Entry) -> u64, hints: &[NoteId]) -> Vec<NoteId> {
    let mut rest: Vec<_> = entries.iter().filter(|e| !hints.contains(&e.id)).collect();
    rest.sort_by_cached_key(|e| (e.kind == NoteKind::Book, size(e), e.id.as_str().to_owned()));
    let hinted = hints.iter().filter(|h| entries.iter().any(|e| &e.id == *h)).cloned();
    hinted.chain(rest.into_iter().map(|e| e.id.clone())).collect()
}

/// Размер исходников: у заметки — главный файл, у книги — все `.typ` её каталога.
fn source_size(root: &Path, entry: &Entry) -> u64 {
    let main = root.join(&entry.main);
    match entry.kind {
        NoteKind::Note => fs::metadata(&main).map_or(0, |m| m.len()),
        NoteKind::Book => main.parent().map_or(0, typ_size),
    }
}

fn typ_size(dir: &Path) -> u64 {
    let Ok(read) = fs::read_dir(dir) else { return 0 };
    read.flatten()
        .map(|e| {
            let path = e.path();
            if path.is_dir() {
                typ_size(&path)
            } else if path.extension().is_some_and(|x| x == "typ") {
                e.metadata().map_or(0, |m| m.len())
            } else {
                0
            }
        })
        .sum()
}

impl Notes {
    /// Подсказать прогреву, что собрать первым (заметки во вкладках, недавние).
    pub fn hint_warm(&self, ids: Vec<NoteId>) {
        self.warm_queue().hint(ids);
    }

    /// Один проход прогрева: собрать всё несобранное по порядку. Новая
    /// подсказка — проход начинается заново (собранное пропустится).
    pub fn warm_pass(&self, opts: impl Fn() -> FigureOptions) -> WarmStats {
        let queue = self.warm_queue();
        let mut stats = WarmStats::default();
        'pass: loop {
            let generation = queue.generation.load(Ordering::SeqCst);
            let Ok(entries) = self.entries() else { return stats };
            let hints = queue.hints.lock().clone();
            let root = self.vault().root().to_owned();
            for id in order(&entries, |e| source_size(&root, e), &hints) {
                if queue.generation.load(Ordering::SeqCst) != generation {
                    continue 'pass;
                }
                if self.is_built(&id) {
                    stats.skipped += 1;
                    continue;
                }
                self.wait_for_users();
                // Пока ждали, заметку мог собрать запрос пользователя.
                if self.is_built(&id) {
                    stats.skipped += 1;
                    continue;
                }
                match self.warm_one(&id, opts()) {
                    Ok(()) => stats.built += 1,
                    Err(e) => tracing::debug!(%id, "прогрев: {e}"),
                }
            }
            return stats;
        }
    }

    /// Прогревать бесконечно (фоновый поток сервера).
    pub fn warm_forever(&self, opts: impl Fn() -> FigureOptions) -> ! {
        let queue = self.warm_queue();
        loop {
            let started = Instant::now();
            let stats = self.warm_pass(&opts);
            if stats.built > 0 {
                tracing::info!(
                    built = stats.built,
                    skipped = stats.skipped,
                    ms = started.elapsed().as_millis(),
                    "прогрев: заметки собраны заранее"
                );
            }
            let generation = queue.generation.load(Ordering::SeqCst);
            let mut hints = queue.hints.lock();
            if queue.generation.load(Ordering::SeqCst) == generation {
                queue.wake.wait_for(&mut hints, RESCAN);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn entry(id: &str, kind: NoteKind) -> Entry {
        Entry { id: NoteId::new(id).unwrap(), kind, main: PathBuf::from(format!("{id}.typ")) }
    }

    #[test]
    fn hints_first_then_small_notes_then_books() {
        let entries = [
            entry("Большая", NoteKind::Note),
            entry("Книга", NoteKind::Book),
            entry("Малая", NoteKind::Note),
            entry("Открытая", NoteKind::Note),
            entry("Тонкая книга", NoteKind::Book),
        ];
        let size = |e: &Entry| match e.id.as_str() {
            "Большая" => 9000,
            "Книга" => 50_000,
            "Тонкая книга" => 10,
            _ => 100,
        };
        let hints = [NoteId::new("Открытая").unwrap(), NoteId::new("Удалённая").unwrap()];
        let ids: Vec<_> = order(&entries, size, &hints).into_iter().map(|id| id.to_string()).collect();
        assert_eq!(ids, ["Открытая", "Малая", "Большая", "Тонкая книга", "Книга"]);
    }
}
