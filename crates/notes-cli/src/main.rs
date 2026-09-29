//! `notes` — заметки на Typst из командной строки. Работает из любой папки:
//! хранилище — в каталоге данных пользователя, библиотека, клиент и шрифты
//! встроены в бинарник (релизная сборка).
//!
//!   notes serve            локальный сервер с клиентом (http://127.0.0.1:8421)
//!   notes new <путь>       заготовка заметки (--book — книги)
//!   notes list | tags      заметки хранилища / теги
//!   notes check [путь]     ошибки компиляции и битые ссылки
//!   notes pdf <путь>       заметка в PDF (вид PDF из baluk)
//!   notes docs <тема>      как писать заметки, API библиотеки
//!   notes info             где хранилище, настройки, библиотека
//!   notes vaults [new <имя>]  хранилища / создать новое
//!
//! Каталог данных (`vaults/`, `settings.json`, `cache/`): `--data` или
//! `NOTES_DATA`, иначе `data` из `~/.config/baluk-notes/config.toml`, иначе
//! `~/.local/share/baluk-notes`. Хранилища — `<данные>/vaults/<имя>/`;
//! хранилища по умолчанию нет: команды заметок — всегда с `--vault <имя>`
//! (папку вне каталога данных — путём: `--vault tests/vault`), первое
//! хранилище создаёт пользователь (`notes vaults new`, приложение).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use notes_core::check::{Report, check, check_note};
use notes_core::new_note::NewNote;
use notes_core::settings::{Platform, Schema, SettingsStore};
use notes_core::vault::NoteKind;
use notes_core::{LibrarySource, NoteId, Notes, NotesConfig, VaultName, Vaults};

/// Имя каталогов приложения: `~/.config/<APP>`, `~/.local/share/<APP>`.
const APP: &str = "baluk-notes";
/// Адрес `notes serve` по умолчанию.
const ADDR: &str = "127.0.0.1:8421";

#[derive(Debug, Parser)]
#[command(version, about = "Заметки на Typst")]
struct Cli {
    /// Каталог данных: хранилища vaults/, settings.json, кэш. По умолчанию —
    /// `data` из ~/.config/baluk-notes/config.toml, иначе ~/.local/share/baluk-notes.
    #[arg(long, global = true, env = "NOTES_DATA")]
    data: Option<PathBuf>,

    /// Хранилище: имя (папка в <data>/vaults/) или путь к папке — со «/»
    /// (tests/vault, ./заметки). Нужно командам заметок (new, list, tags,
    /// check, pdf); список — `notes vaults`.
    #[arg(long, global = true)]
    vault: Option<String>,

    /// Куда уходят удалённые из приложения заметки: по умолчанию — корзина
    /// системы; каталог — в него (тесты).
    #[arg(long, global = true, env = "NOTES_TRASH", hide = true)]
    trash: Option<PathBuf>,

    /// Библиотека оформления (baluk/), видна заметкам как /_baluk/.
    /// По умолчанию — встроенная в бинарник; в отладочной сборке — каталог
    /// baluk/ репозитория (правки видны сразу).
    #[arg(long, global = true, env = "NOTES_LIBRARY")]
    library: Option<PathBuf>,

    /// Дополнительный каталог шрифтов (можно повторять).
    #[arg(long = "font-path", global = true, env = "NOTES_FONT_PATHS", value_delimiter = ':')]
    font_paths: Vec<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Локальный сервер с клиентом.
    Serve {
        /// Адрес; для доступа из сети — 0.0.0.0:8421.
        #[arg(long, default_value = ADDR)]
        addr: SocketAddr,
        /// Токен доступа: без него сервер отвечает 401. Передаётся заголовком
        /// `Authorization: Bearer …`, параметром `?token=` (сервер ставит
        /// cookie) или cookie `notes_token`. Нужен встроенному серверу Tauri.
        #[arg(long, env = "NOTES_TOKEN", hide_env_values = true)]
        token: Option<String>,
    },
    /// Заготовка новой заметки или книги; печатает путь её файла и, второй
    /// строкой, путь заметки (для check, pdf, #see).
    ///
    /// Имя файла — из названия (без / \ : * ? " < > |; занято — с номером),
    /// в папке --folder; или путь целиком — аргументом. Существующее не
    /// перезаписывается. Как писать дальше — `notes docs writing`.
    New {
        /// Путь от корня хранилища, без .typ: «Сеть/SSH», «Курсы/Матан»;
        /// без него — из названия.
        #[arg(required_unless_present = "title", conflicts_with = "folder")]
        id: Option<String>,
        /// Папка для заметки с именем из названия: «Сеть», «Курсы/Матан»; по умолчанию — корень.
        #[arg(long)]
        folder: Option<String>,
        /// Книга: папка с main.typ, главы — файлы рядом (иначе — заметка, один файл).
        #[arg(long)]
        book: bool,
        /// Название — любой текст; по умолчанию (с путём) — последний сегмент пути.
        #[arg(long)]
        title: Option<String>,
        /// Тег (можно повторять).
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Язык заметки (ISO 639: en, de…); по умолчанию — русский.
        #[arg(long)]
        lang: Option<String>,
    },
    /// Заметки и книги хранилища: путь, вид, название, теги.
    List {
        /// В JSON.
        #[arg(long)]
        json: bool,
    },
    /// Теги хранилища и число заметок с ними.
    Tags,
    /// Проверить хранилище или одну заметку: ошибки компиляции,
    /// предупреждения, битые ссылки (код выхода 1 — ошибки или битые ссылки).
    Check {
        /// Только эта заметка или книга (ссылки проверяются по всему хранилищу).
        id: Option<String>,
        /// Отчёт в JSON.
        #[arg(long)]
        json: bool,
    },
    /// Заметку или книгу — в PDF.
    Pdf {
        /// Путь заметки от корня хранилища: «Сеть/SSH», «Конспекты/Матан».
        id: String,
        /// Файл результата; по умолчанию — <имя заметки>.pdf здесь.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Тема; по умолчанию — первая (светлая).
        #[arg(long)]
        theme: Option<String>,
    },
    /// Документация: как писать заметки, API библиотеки оформления.
    Docs {
        /// Что показать.
        topic: Topic,
    },
    /// Где хранилище, настройки и кэш, какая библиотека, адрес сервера.
    Info,
    /// Хранилища каталога данных; `new <имя>` — создать новое.
    Vaults {
        #[command(subcommand)]
        action: Option<VaultsAction>,
    },
}

#[derive(Debug, Subcommand)]
enum VaultsAction {
    /// Новое пустое хранилище: <data>/vaults/<имя>/.
    New {
        /// Имя — оно же имя папки: «Учёба», «Работа 2026».
        name: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Topic {
    /// Как писать заметки: процесс, текст, рисунки, проверка.
    Writing,
    /// Библиотека оформления baluk: шаблоны, блоки, рисунки, интерактив.
    Library,
}

impl Topic {
    fn text(self) -> &'static str {
        match self {
            Self::Writing => include_str!("../../../docs/writing.md"),
            Self::Library => include_str!("../../../baluk/README.md"),
        }
    }
}

/// Файл настроек `notes` (`~/.config/baluk-notes/config.toml`).
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    /// Каталог данных; `~/` — домашний, относительный — от файла настроек.
    data: Option<PathBuf>,
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(APP).join("config.toml"))
}

/// Файл настроек: нет — пустые настройки.
fn load_config() -> Result<Config> {
    let Some(path) = config_path().filter(|p| p.is_file()) else { return Ok(Config::default()) };
    let text = std::fs::read_to_string(&path).with_context(|| format!("прочитать {}", path.display()))?;
    let mut config: Config = toml::from_str(&text).with_context(|| format!("настройки {}", path.display()))?;
    if let Some(data) = config.data.take() {
        let home = dirs::home_dir().unwrap_or_default();
        config.data = Some(match data.strip_prefix("~") {
            Ok(rest) => home.join(rest),
            Err(_) => path.parent().unwrap_or(Path::new("")).join(data),
        });
    }
    Ok(config)
}

/// Каталог данных: явный, из файла настроек или стандартный.
fn data_dir(explicit: Option<&PathBuf>, config: &Config) -> Result<PathBuf> {
    if let Some(dir) = explicit.or(config.data.as_ref()) {
        return Ok(dir.clone());
    }
    match dirs::data_dir() {
        Some(dir) => Ok(dir.join(APP)),
        None => bail!("не найден каталог данных пользователя — укажите --data"),
    }
}

/// Библиотека оформления: явно указанная, в отладочной сборке — из
/// репозитория, иначе — встроенная.
fn library(explicit: Option<&PathBuf>) -> LibrarySource {
    if let Some(dir) = explicit {
        return LibrarySource::Dir(dir.clone());
    }
    let repo = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../baluk"));
    if cfg!(debug_assertions) && repo.join("lib.typ").is_file() {
        LibrarySource::Dir(repo)
    } else {
        LibrarySource::Embedded
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,notes=info".into()),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ошибка: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Какое хранилище: `--vault` (или `NOTES_VAULT`) и `vault` из config.toml.
#[derive(Debug, Clone)]
enum VaultArg {
    /// Хранилище каталога данных по имени.
    Name(String),
    /// Папка где угодно (в аргументе есть `/`).
    Path(PathBuf),
}

impl VaultArg {
    fn parse(raw: &str) -> Self {
        if raw.contains(['/', '\\']) || raw == "." || raw == ".." {
            Self::Path(PathBuf::from(raw))
        } else {
            Self::Name(raw.to_owned())
        }
    }
}

/// Хранилище для команды: имя и папка. Хранилища по умолчанию нет: не
/// названо — ошибка со списком. Папку по пути создаёт, если её нет (тесты);
/// по имени — только существующее.
fn pick_vault(vaults: &Vaults, arg: Option<&VaultArg>) -> Result<(VaultName, PathBuf)> {
    match arg {
        Some(VaultArg::Path(dir)) => {
            if !dir.exists() {
                std::fs::create_dir_all(dir).with_context(|| format!("создать {}", dir.display()))?;
                tracing::info!("создано пустое хранилище {}", dir.display());
            }
            let dir = dir.canonicalize().with_context(|| format!("хранилище {}", dir.display()))?;
            let name = dir.file_name().and_then(|n| n.to_str()).map(VaultName::new);
            let Some(Ok(name)) = name else {
                bail!("имя папки {} не годится для хранилища", dir.display())
            };
            Ok((name, dir))
        }
        Some(VaultArg::Name(name)) => {
            let name = vaults.find(name)?;
            Ok((name.clone(), vaults.path(&name)))
        }
        None => Err(notes_core::Error::VaultRequired(vaults.list()?).into()),
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    // Без хранилища: документация.
    if let Command::Docs { topic } = cli.command {
        print!("{}", topic.text());
        return Ok(ExitCode::SUCCESS);
    }
    let data = data_dir(cli.data.as_ref(), &load_config()?)?;
    let vaults = Vaults::new(&data);
    let arg = cli.vault.as_deref().map(VaultArg::parse);
    let config = NotesConfig {
        vault: PathBuf::new(),
        library: library(cli.library.as_ref()),
        font_dirs: cli.font_paths.clone(),
        cache: Some(notes_core::cache::default_dir(&data)),
        trash: cli.trash.clone(),
    };

    // Без открытого хранилища: список хранилищ, сведения, сервер.
    match cli.command {
        Command::Vaults { action } => return vaults_command(&vaults, action),
        Command::Info => return info(&vaults, &data, arg.as_ref(), cli.library.as_ref()),
        Command::Serve { addr, token } => return serve(vaults, config, &data, arg, addr, token),
        _ => {}
    }

    let (name, vault) = pick_vault(&vaults, arg.as_ref())?;
    let started = std::time::Instant::now();
    let notes = Notes::open(&NotesConfig { vault: vault.clone(), ..config }).context("открыть хранилище")?;
    tracing::debug!(ms = started.elapsed().as_millis(), "хранилище «{name}»: {}", notes.vault().location());

    match cli.command {
        Command::New { id, folder, book, title, tags, lang } => {
            let kind = if book { NoteKind::Book } else { NoteKind::Note };
            let note = NewNote { kind, title, tags, lang };
            let id = if let Some(id) = id {
                new_note_id(&id, &vault)?
            } else {
                // Имя файла — из названия, в папке --folder (нет — в корне).
                let folder = match folder.as_deref().map(|f| f.trim_matches('/')) {
                    Some(f) if !f.is_empty() => new_note_id(f, &vault)?.to_string(),
                    _ => String::new(),
                };
                note.id_in(notes.vault(), &folder)?
            };
            let main = note.create(notes.vault(), &id)?;
            println!("{}", notes.vault().storage().display(&main).display());
            println!("{id}");
            eprintln!("в приложении: http://{ADDR}/v/{name}/n/{id} (если запущен notes serve)");
            Ok(ExitCode::SUCCESS)
        }
        Command::List { json } => list(&notes, json),
        Command::Tags => tags(&notes),
        Command::Check { id, json } => {
            // Сборок одновременно и память Typst — как в приложении.
            notes.apply_device(&open_settings(&notes, &data)?.device());
            let report = match id {
                Some(id) => check_note(&notes, &note_id(&id, &vault)?)?,
                None => check(&notes)?,
            };
            print_check(&report, json)
        }
        Command::Pdf { id, out, theme } => pdf(&notes, &note_id(&id, &vault)?, out, theme),
        Command::Docs { .. } | Command::Info | Command::Vaults { .. } | Command::Serve { .. } => {
            unreachable!("обработано выше")
        }
    }
}

fn vaults_command(vaults: &Vaults, action: Option<VaultsAction>) -> Result<ExitCode> {
    if let Some(VaultsAction::New { name }) = action {
        let name = VaultName::new(name)?;
        let path = vaults.create(&name)?;
        println!("{}", path.display());
        eprintln!("в приложении: http://{ADDR}/v/{name}/ (если запущен notes serve)");
        return Ok(ExitCode::SUCCESS);
    }
    let list = vaults.list()?;
    for name in &list {
        println!("{name}");
    }
    if list.is_empty() {
        eprintln!("хранилищ нет — создайте: notes vaults new \"Имя\" (или в приложении)");
    }
    Ok(ExitCode::SUCCESS)
}

fn info(vaults: &Vaults, data: &Path, arg: Option<&VaultArg>, library_dir: Option<&PathBuf>) -> Result<ExitCode> {
    let config = config_path().map(|p| p.display().to_string()).unwrap_or_default();
    let library = match library(library_dir) {
        LibrarySource::Dir(dir) => dir.display().to_string(),
        LibrarySource::Embedded => "встроенная в бинарник".into(),
    };
    // Хранилище не названо — не ошибка: показать список.
    match arg {
        Some(arg) => {
            let (name, path) = pick_vault(vaults, Some(arg))?;
            println!("хранилище: {} («{name}»)", path.display());
        }
        None => println!("хранилище: не выбрано (--vault \"Имя\")"),
    }
    let names: Vec<String> = vaults.list()?.iter().map(ToString::to_string).collect();
    let names = if names.is_empty() { "нет".to_owned() } else { names.join(", ") };
    println!("хранилища: {names} ({})", vaults.root().display());
    println!("данные:    {} (settings.json, cache/)", data.display());
    println!("настройки: {config} (data = \"…\" — другой каталог данных)");
    println!("библиотека: {library}");
    println!("сервер:    http://{ADDR}/ (notes serve)");
    Ok(ExitCode::SUCCESS)
}

/// Путь заметки из командной строки. Частая ошибка (особенно у агентов) —
/// путь на диске вместо пути от корня хранилища: подсказать нужный.
fn note_id(raw: &str, vault: &Path) -> Result<NoteId> {
    let path = Path::new(raw);
    if path.is_absolute() {
        let inside = std::iter::once(vault.to_path_buf())
            .chain(vault.canonicalize().ok())
            .find_map(|v| path.strip_prefix(v).ok().map(|rest| rest.to_string_lossy().into_owned()))
            .filter(|rest| !rest.is_empty());
        match inside {
            Some(rest) => {
                let rest = rest.strip_suffix(".typ").unwrap_or(&rest);
                bail!("«{raw}» — путь на диске; нужен путь от корня хранилища: «{rest}»")
            }
            None => bail!(
                "«{raw}» — путь на диске; нужен путь от корня хранилища ({}), например «Папка/Название»",
                vault.display()
            ),
        }
    }
    Ok(NoteId::new(raw)?)
}

/// Путь новой заметки: вдобавок к `note_id` — не начинается с имени
/// каталога хранилища («vault/Тема» из каталога данных создало бы
/// `vault/vault/Тема.typ`), если такой папки в хранилище нет.
fn new_note_id(raw: &str, vault: &Path) -> Result<NoteId> {
    let id = note_id(raw, vault)?;
    if let Some(name) = vault.file_name().and_then(|n| n.to_str())
        && let Some(rest) = id.as_str().strip_prefix(&format!("{name}/"))
        && !vault.join(name).is_dir()
    {
        bail!("«{raw}» начинается с имени каталога хранилища; путь — от его корня: «{rest}»");
    }
    Ok(id)
}

fn open_settings(notes: &Notes, data: &Path) -> Result<SettingsStore> {
    let schema = Schema::new(notes.themes().themes(), Platform::current());
    SettingsStore::open(data.join("settings.json"), schema).context("настройки")
}

/// Сервер: все хранилища каталога данных (открываются по запросу;
/// хранилищ может не быть — первое создают в приложении) или одно
/// (`--vault <имя или путь>`).
fn serve(
    vaults: Vaults,
    config: NotesConfig,
    data: &Path,
    arg: Option<VaultArg>,
    addr: SocketAddr,
    token: Option<String>,
) -> Result<ExitCode> {
    let set = if let Some(arg) = arg {
        let (name, vault) = pick_vault(&vaults, Some(&arg))?;
        let notes = Notes::open(&NotesConfig { vault, ..config }).context("открыть хранилище")?;
        let settings = Arc::new(open_settings(&notes, data)?);
        tracing::info!("хранилище «{name}»: {}", notes.vault().location());
        notes_server::VaultSet::single(name, Arc::new(notes), settings)
    } else {
        // Темы и шрифты — у всех хранилищ одни (библиотека): ядро без
        // хранилища, чтобы сервер работал и без них.
        let library = Notes::with_storage(Arc::new(notes_core::storage::MemStorage::new()), &config)
            .context("библиотека оформления")?;
        let settings = Arc::new(open_settings(&library, data)?);
        tracing::info!("хранилища: {}", vaults.root().display());
        notes_server::VaultSet::registry(vaults, config, Arc::new(library), settings)
    };
    let state = notes_server::AppState::new(set).with_token(token);
    if state.token.is_some() {
        tracing::info!("доступ — только с токеном");
    }
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("занять {addr}"))?;
        tracing::info!("открыть: http://{addr}/");
        notes_server::serve(listener, state, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
        Ok(ExitCode::SUCCESS)
    })
}

/// Строка списка или объект JSON: заметка из индекса исходников.
#[derive(Debug, serde::Serialize)]
struct ListItem<'a> {
    id: &'a NoteId,
    kind: NoteKind,
    title: Option<&'a str>,
    tags: &'a [String],
}

fn list(notes: &Notes, json: bool) -> Result<ExitCode> {
    let index = notes.index()?;
    let items: Vec<ListItem> = index
        .outlines()
        .map(|(entry, outline)| ListItem {
            id: &entry.id,
            kind: entry.kind,
            title: outline.title.as_deref(),
            tags: &outline.tags,
        })
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(ExitCode::SUCCESS);
    }
    for item in &items {
        let kind = if item.kind == NoteKind::Book { "  [книга]" } else { "" };
        let title = item.title.filter(|t| *t != item.id.name()).map(|t| format!("  «{t}»")).unwrap_or_default();
        let tags: String = item.tags.iter().flat_map(|t| ["  #", t.as_str()]).collect();
        println!("{}{kind}{title}{tags}", item.id);
    }
    Ok(ExitCode::SUCCESS)
}

fn tags(notes: &Notes) -> Result<ExitCode> {
    let index = notes.index()?;
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for (_, outline) in index.outlines() {
        for tag in &outline.tags {
            *counts.entry(tag).or_default() += 1;
        }
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (tag, count) in sorted {
        println!("{tag}\t{count}");
    }
    Ok(ExitCode::SUCCESS)
}

fn pdf(notes: &Notes, id: &NoteId, out: Option<PathBuf>, theme: Option<String>) -> Result<ExitCode> {
    let theme = theme.unwrap_or_else(|| notes.themes().names().first().cloned().unwrap_or_default());
    let out = out.unwrap_or_else(|| PathBuf::from(format!("{}.pdf", id.name())));
    match notes.pdf(id, &theme)? {
        Ok(bytes) => {
            std::fs::write(&out, bytes).with_context(|| format!("записать {}", out.display()))?;
            println!("{} → {}", id, out.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(errors) => {
            for e in errors {
                eprintln!("{id}: {e}");
            }
            Ok(ExitCode::FAILURE)
        }
    }
}

fn print_check(report: &Report, json: bool) -> Result<ExitCode> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        for n in &report.notes {
            for e in &n.errors {
                println!("{}: {e}", n.id);
            }
            for w in &n.warnings {
                println!("{}: {w}", n.id);
            }
            for l in &n.broken_links {
                let anchor = l.anchor.as_deref().map(|a| format!(" / {a}")).unwrap_or_default();
                println!("{}: битая ссылка «{}{anchor}»: {}", n.id, l.target, l.reason);
            }
        }
        for f in &report.folders {
            println!("{}: {}", f.file, f.error);
        }
        println!("{}", report.summary());
    }
    Ok(if report.is_clean() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(result: Result<NoteId>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn disk_path_gets_a_hint() {
        let vault = Path::new("/нет/data/vault");
        assert_eq!(note_id("Сеть/SSH", vault).unwrap().as_str(), "Сеть/SSH");
        assert!(
            err(note_id("/нет/data/vault/Сеть/SSH.typ", vault)).ends_with("нужен путь от корня хранилища: «Сеть/SSH»")
        );
        assert!(err(note_id("/elsewhere/SSH", vault)).contains("(/нет/data/vault)"));
        assert!(err(note_id("/нет/data/vault", vault)).contains("например"));
    }

    #[test]
    fn new_note_is_not_under_vault_name() {
        let vault = Path::new("/нет/data/vault");
        assert!(err(new_note_id("vault/Тема", vault)).ends_with("путь — от его корня: «Тема»"));
        assert_eq!(new_note_id("vaults/Тема", vault).unwrap().as_str(), "vaults/Тема");
        assert_eq!(new_note_id("Тема", vault).unwrap().as_str(), "Тема");
    }

    /// Хранилища по умолчанию нет: даже единственное — только названное.
    #[test]
    fn vault_must_be_named() {
        let data = tempfile::tempdir().unwrap();
        let vaults = Vaults::new(data.path());
        let err = |arg: Option<&VaultArg>| pick_vault(&vaults, arg).unwrap_err().to_string();
        assert!(err(None).starts_with("хранилищ нет — создайте"), "{}", err(None));
        vaults.create(&VaultName::new("Учёба").unwrap()).unwrap();
        assert_eq!(err(None), "укажите хранилище: --vault \"Имя\"; есть: «Учёба»");
        let (name, path) = pick_vault(&vaults, Some(&VaultArg::parse("Учёба"))).unwrap();
        assert_eq!((name.as_str(), path), ("Учёба", vaults.root().join("Учёба")));
        assert!(err(Some(&VaultArg::parse("Нет"))).starts_with("нет хранилища «Нет»"));
        assert!(!data.path().join("vaults/Нет").exists(), "не создаётся само");
    }
}
