//! Хранилище за интерфейсом: откуда ядро берёт файлы заметок.
//!
//! Весь доступ к файлам хранилища — через [`Storage`]: список файлов, чтение,
//! сведения о файле (размер, время изменения — из них версия заметки, см.
//! [`crate::version`]). Сейчас хранилище — каталог на диске ([`DirStorage`]);
//! позже (M5, синхронизация) рядом встанет база данных, и остальному ядру
//! меняться не придётся. Тесты слоёв ядра работают на [`MemStorage`] — в
//! памяти, без диска.
//!
//! Хранилище может сообщать об изменениях файлов ([`Storage::watch`]):
//! каталог — через наблюдатель ОС (`notify`), и тогда список файлов берётся
//! из памяти, пока в каталоге ничего не менялось. Кто не умеет — ядро
//! обходит файлы, как раньше.
//!
//! Пути — относительные, через `/`, без `/` в начале: `Сеть/SSH.typ`.
//! Библиотека оформления (`/_baluk/`) и пакеты Typst — не хранилище: их
//! читает компилятор ([`crate::world`]).

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;

/// Сведения о файле хранилища.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMeta {
    pub is_dir: bool,
    pub len: u64,
    /// Время изменения (у каталога и там, где его нет, — `None`).
    pub modified: Option<SystemTime>,
}

/// Куда хранилище сообщает об изменениях: пути изменившихся файлов (как в
/// [`Storage::list`], но и служебные на `_`) — или `None`: наблюдатель
/// сломался, изменения могли потеряться (дальше — без него).
pub type ChangeSink = Arc<dyn Fn(Option<Vec<String>>) + Send + Sync>;

/// Наблюдатель работает, пока жив этот объект.
pub type WatchGuard = Box<dyn Any + Send + Sync>;

/// Файлы хранилища.
pub trait Storage: Send + Sync + fmt::Debug {
    /// Где хранилище — для журнала и ключа кэша на диске (у разных хранилищ
    /// бывают заметки с одинаковыми путями).
    fn location(&self) -> String;

    /// Все файлы (не каталоги), кроме служебных: путь, в котором есть имя на
    /// `_` или `.`, в список не входит. Порядок — любой.
    fn list(&self) -> io::Result<Vec<String>>;

    /// Сведения о файле или каталоге. Нет такого — `NotFound`.
    fn stat(&self, path: &str) -> io::Result<FileMeta>;

    /// Содержимое файла.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;

    /// Путь для сообщений об ошибках: на диске — полный, иначе — как есть.
    fn display(&self, path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    /// Сообщать об изменениях файлов в `sink`. `Ok(None)` — хранилище так
    /// не умеет (по умолчанию).
    fn watch(&self, sink: ChangeSink) -> io::Result<Option<WatchGuard>> {
        let _ = sink;
        Ok(None)
    }
}

/// Исходник Typst (`.typ`).
pub fn is_typ(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| e == "typ")
}

/// Служебное имя (библиотека `_baluk`, `.git`): в список файлов не входит.
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('_') || name.starts_with('.')
}

/// Хранилище — каталог на диске.
#[derive(Debug, Clone)]
pub struct DirStorage {
    root: PathBuf,
    /// Список файлов, пока наблюдатель не сообщил об изменении.
    listed: Arc<Mutex<Listed>>,
}

#[derive(Debug, Default)]
struct Listed {
    /// Наблюдатель работает — список можно помнить.
    watching: bool,
    files: Option<Vec<String>>,
}

impl DirStorage {
    /// Каталог должен существовать; путь приводится к каноническому.
    pub fn open(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = fs::canonicalize(root.as_ref())?;
        if !root.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotADirectory, "хранилище — не каталог"));
        }
        Ok(Self { root, listed: Arc::default() })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Путь на диске; `..`, абсолютные пути и префиксы не допускаются.
    fn full(&self, path: &str) -> io::Result<PathBuf> {
        let rel = Path::new(path);
        if rel.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
            Ok(self.root.join(rel))
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, format!("путь вне хранилища: {path}")))
        }
    }

    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) -> io::Result<()> {
        for item in fs::read_dir(dir)? {
            let item = item?;
            let name = item.file_name();
            let Some(name) = name.to_str() else { continue }; // не UTF-8 — не заметка
            if is_hidden(name) {
                continue;
            }
            let child = if rel.is_empty() { name.to_owned() } else { format!("{rel}/{name}") };
            // Тип — из записи каталога, без перехода по ссылке (как раньше в `Vault`).
            if item.file_type()?.is_dir() {
                Self::walk(&item.path(), &child, out)?;
            } else {
                out.push(child);
            }
        }
        Ok(())
    }
}

impl Storage for DirStorage {
    fn location(&self) -> String {
        self.root.display().to_string()
    }

    fn list(&self) -> io::Result<Vec<String>> {
        // Под замком: изменение во время обхода ждёт его конца и сбросит
        // запомненный список.
        let mut listed = self.listed.lock();
        if let Some(files) = &listed.files {
            return Ok(files.clone());
        }
        let mut out = Vec::new();
        Self::walk(&self.root, "", &mut out)?;
        if listed.watching {
            listed.files = Some(out.clone());
        }
        Ok(out)
    }

    fn watch(&self, sink: ChangeSink) -> io::Result<Option<WatchGuard>> {
        use notify::event::{EventKind, ModifyKind};
        use notify::{RecursiveMode, Watcher};
        let root = self.root.clone();
        let listed = self.listed.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            // Чтение файлов (компиляция, сам обход) — не изменение.
            if matches!(&event, Ok(e) if e.kind.is_access()) {
                return;
            }
            let mut state = listed.lock();
            // Правка содержимого список файлов не меняет.
            if !matches!(&event, Ok(e) if matches!(e.kind, EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Metadata(_)))) {
                state.files = None;
            }
            match event {
                // Очередь событий ОС переполнилась (inotify): что поменялось — неизвестно.
                Ok(event) if event.need_rescan() => {
                    tracing::warn!("наблюдатель хранилища: события потеряны; дальше — обход файлов");
                    state.watching = false;
                    drop(state);
                    sink(None);
                }
                Ok(event) => {
                    drop(state);
                    let paths: Vec<String> = event
                        .paths
                        .iter()
                        .filter_map(|p| p.strip_prefix(&root).ok())
                        .filter_map(|p| p.to_str().map(|p| p.replace(std::path::MAIN_SEPARATOR, "/")))
                        .filter(|p| !p.is_empty() && !p.split('/').any(|name| name.starts_with('.')))
                        .collect();
                    if !paths.is_empty() {
                        sink(Some(paths));
                    }
                }
                Err(e) => {
                    tracing::warn!("наблюдатель хранилища: {e}; дальше — обход файлов");
                    state.watching = false;
                    drop(state);
                    sink(None);
                }
            }
        })
        .map_err(io::Error::other)?;
        watcher.watch(&self.root, RecursiveMode::Recursive).map_err(io::Error::other)?;
        let mut state = self.listed.lock();
        state.watching = true;
        state.files = None;
        Ok(Some(Box::new(Unwatch { _watcher: watcher, listed: self.listed.clone() })))
    }

    fn stat(&self, path: &str) -> io::Result<FileMeta> {
        let meta = fs::metadata(self.full(path)?)?;
        Ok(FileMeta { is_dir: meta.is_dir(), len: meta.len(), modified: meta.modified().ok() })
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        fs::read(self.full(path)?)
    }

    fn display(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

/// Остановить наблюдатель: список файлов больше не помнить.
struct Unwatch {
    _watcher: notify::RecommendedWatcher,
    listed: Arc<Mutex<Listed>>,
}

impl Drop for Unwatch {
    fn drop(&mut self) {
        let mut state = self.listed.lock();
        state.watching = false;
        state.files = None;
    }
}

/// Хранилище в памяти — для тестов. Время изменения — счётчик записей:
/// каждая запись меняет версию файла. Сообщает о записях ([`Storage::watch`]).
#[derive(Default)]
pub struct MemStorage {
    files: Mutex<MemFiles>,
    sink: Mutex<Option<ChangeSink>>,
}

impl fmt::Debug for MemStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemStorage").field("files", &self.files).finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct MemFiles {
    files: BTreeMap<String, (Vec<u8>, u64)>,
    clock: u64,
}

impl MemStorage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Записать файл (каталоги появляются сами).
    pub fn write(&self, path: &str, data: impl Into<Vec<u8>>) {
        let mut f = self.files.lock();
        f.clock += 1;
        let clock = f.clock;
        f.files.insert(path.to_owned(), (data.into(), clock));
        drop(f);
        self.changed(path);
    }

    pub fn remove(&self, path: &str) {
        self.files.lock().files.remove(path);
        self.changed(path);
    }

    /// Наблюдатель сломался: изменения могли потеряться (для тестов).
    pub fn lose_changes(&self) {
        let sink = self.sink.lock().clone();
        if let Some(sink) = sink {
            sink(None);
        }
    }

    fn changed(&self, path: &str) {
        let sink = self.sink.lock().clone();
        if let Some(sink) = sink {
            sink(Some(vec![path.to_owned()]));
        }
    }
}

impl Storage for MemStorage {
    fn location(&self) -> String {
        format!("память:{:p}", std::ptr::from_ref(self))
    }

    fn list(&self) -> io::Result<Vec<String>> {
        let f = self.files.lock();
        Ok(f.files.keys().filter(|p| !p.split('/').any(is_hidden)).cloned().collect())
    }

    fn stat(&self, path: &str) -> io::Result<FileMeta> {
        let f = self.files.lock();
        if let Some((data, clock)) = f.files.get(path) {
            let modified = SystemTime::UNIX_EPOCH + Duration::from_nanos(*clock);
            return Ok(FileMeta { is_dir: false, len: data.len() as u64, modified: Some(modified) });
        }
        let prefix = format!("{path}/");
        if path.is_empty() || f.files.keys().any(|p| p.starts_with(&prefix)) {
            return Ok(FileMeta { is_dir: true, len: 0, modified: None });
        }
        Err(io::Error::new(io::ErrorKind::NotFound, format!("нет файла {path}")))
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        match self.stat(path)? {
            FileMeta { is_dir: true, .. } => Err(io::Error::new(io::ErrorKind::IsADirectory, path.to_owned())),
            _ => Ok(self.files.lock().files[path].0.clone()),
        }
    }

    fn watch(&self, sink: ChangeSink) -> io::Result<Option<WatchGuard>> {
        *self.sink.lock() = Some(sink);
        Ok(Some(Box::new(())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(storage: &dyn Storage) {
        let mut list = storage.list().unwrap();
        list.sort();
        assert_eq!(list, ["a.typ", "Сеть/SSH.typ"]);
        assert_eq!(storage.read("Сеть/SSH.typ").unwrap(), b"ssh");
        assert!(storage.stat("Сеть").unwrap().is_dir);
        let meta = storage.stat("a.typ").unwrap();
        assert!(!meta.is_dir);
        assert_eq!(meta.len, 1);
        assert_eq!(storage.stat("нет.typ").unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(storage.read("_baluk/lib.typ").is_ok(), "служебное читается, но не перечисляется");
    }

    #[test]
    fn dir_and_memory_agree() {
        let dir = tempfile::tempdir().unwrap();
        for (f, text) in [("a.typ", "a"), ("Сеть/SSH.typ", "ssh"), ("_baluk/lib.typ", ""), (".git/x", "")] {
            let p = dir.path().join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        let disk = DirStorage::open(dir.path()).unwrap();
        check(&disk);
        assert!(disk.read("../x").is_err(), "за пределы каталога — нельзя");

        let mem = MemStorage::new();
        for (f, text) in [("a.typ", "a"), ("Сеть/SSH.typ", "ssh"), ("_baluk/lib.typ", ""), (".git/x", "")] {
            mem.write(f, text);
        }
        check(&mem);
    }

    /// Ждать, пока `ok()` не станет истиной (события ОС приходят не сразу).
    fn eventually(ok: impl Fn() -> bool) -> bool {
        (0..200).any(|_| {
            ok() || {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
        })
    }

    #[test]
    fn dir_watch_reports_changes_and_keeps_list() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.typ"), "a").unwrap();
        let disk = DirStorage::open(dir.path()).unwrap();
        let seen = Arc::new(Mutex::new(Vec::<Option<Vec<String>>>::new()));
        let sink_seen = seen.clone();
        let guard = disk.watch(Arc::new(move |paths| sink_seen.lock().push(paths))).unwrap().expect("каталог умеет");
        assert_eq!(disk.list().unwrap(), ["a.typ"]);
        assert!(disk.listed.lock().files.is_some(), "список запомнен");

        fs::create_dir(dir.path().join("Сеть")).unwrap();
        fs::write(dir.path().join("Сеть/SSH.typ"), "ssh").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/x"), "").unwrap();
        // Файл в только что созданной папке может прийти одним событием папки.
        assert!(eventually(|| seen.lock().iter().flatten().flatten().any(|p| p.starts_with("Сеть"))));
        fs::write(dir.path().join("a.typ"), "aa").unwrap();
        assert!(eventually(|| seen.lock().iter().flatten().flatten().any(|p| p == "a.typ")));
        assert!(!seen.lock().iter().flatten().flatten().any(|p| p.starts_with(".git")), "служебное на . — не событие");
        let mut list = disk.list().unwrap();
        list.sort();
        assert_eq!(list, ["a.typ", "Сеть/SSH.typ"], "изменение сбросило запомненный список");

        drop(guard);
        assert!(disk.listed.lock().files.is_none());
        disk.list().unwrap();
        assert!(disk.listed.lock().files.is_none(), "без наблюдателя список не помнится");
    }

    #[test]
    fn memory_write_changes_time() {
        let mem = MemStorage::new();
        mem.write("a.typ", "1");
        let first = mem.stat("a.typ").unwrap();
        mem.write("a.typ", "1");
        assert_ne!(first.modified, mem.stat("a.typ").unwrap().modified);
        mem.remove("a.typ");
        assert!(mem.stat("a.typ").is_err());
    }
}
