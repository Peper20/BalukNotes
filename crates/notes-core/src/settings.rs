//! Client settings: schema, validation, storage.
//!
//! The schema is described here, and the client draws the form **by the
//! schema**: a new setting is one entry in [`Schema::new`]. An appearance
//! setting says how to apply it ([`Apply`]: a `data-...` attribute or a CSS
//! variable on `<html>`); the client (`app/src/lib/appearance.ts`) applies it
//! by the schema, and the rule goes to CSS (`app/src/baluk-css/`), with no TS
//! changes. Values are stored in a JSON file as a flat "key -> value" map;
//! unknown keys and invalid values are dropped on load (with a warning in the
//! log) and replaced by the defaults.
//!
//! **Device settings** ([`SettingDef::device`], the `device` group) are about
//! performance: each device has its own, with its own defaults ([`Platform`]),
//! and they are not synced. Appearance is shared. The core applies them on the
//! fly ([`SettingsStore::device`] -> `Notes::apply_device`).
//!
//! **Vault settings** ([`VaultSettings`], `<vault>/.baluk/settings.json`) sit
//! on top of the shared ones: any setting except device settings can be set
//! for one vault only (the user's decision); if not set, the shared one holds.
//! The file moves with the vault folder. A change from the interface goes to
//! the open vault; for [`SettingDef::shared`] settings (theme, font size) and
//! device settings, to all (the user's decisions).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use parking_lot::RwLock;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::cache::DiskLimits;
use crate::figures::FigureOptions;
use crate::fsutil::write_atomic;
use crate::themes::Theme;
use crate::warm::WarmMode;
use crate::{Error, Result};

/// Checks a text setting: the normalized value or an error text.
pub type TextCheck = fn(&str) -> std::result::Result<String, String>;

/// The description of one setting.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SettingDef {
    /// `group.name`, for example `view.numbering`.
    pub key: &'static str,
    pub label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub help: Option<&'static str>,
    #[serde(flatten)]
    pub kind: Kind,
    pub default: Value,
    /// How the client applies the setting to the page; `None` means the client
    /// does it in its own code (theme, books, refresh) or it is for the server (figures).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub apply: Option<Apply>,
    /// A device setting: each device has its own, not synced.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub device: bool,
    /// A change from the interface goes to all vaults by default (theme, font
    /// size: the user's decision), not only to the open one; a vault can set
    /// its own. The others are the other way round (see the module).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub shared: bool,
    /// A warning next to the setting: why it is dangerous.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub warning: Option<&'static str>,
    /// Checks the text ([`Kind::Text`]): the normalized value or an error.
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub check: Option<TextCheck>,
}

/// How an appearance setting is applied: the value goes on `<html>`, the rule into CSS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "to", rename_all = "lowercase")]
pub enum Apply {
    /// The attribute `name` (`data-...`) with the value as a string: `data-numbering="all"`,
    /// `data-toc="false"`.
    Attr { name: &'static str },
    /// The CSS variable `name` (`--...`): the value with a unit, `--k-size: 19px`.
    Var { name: &'static str, unit: &'static str },
}

impl SettingDef {
    /// A setting with nothing but its key, label, kind and default.
    fn new(key: &'static str, label: &'static str, kind: Kind, default: Value) -> Self {
        Self {
            key,
            label,
            help: None,
            kind,
            default,
            apply: None,
            device: false,
            shared: false,
            warning: None,
            check: None,
        }
    }

    fn help(mut self, help: &'static str) -> Self {
        self.help = Some(help);
        self
    }

    /// Applied as a `data-...` attribute on `<html>`.
    fn attr(mut self, name: &'static str) -> Self {
        self.apply = Some(Apply::Attr { name });
        self
    }

    /// Applied as a CSS variable on `<html>`.
    fn var(mut self, name: &'static str, unit: &'static str) -> Self {
        self.apply = Some(Apply::Var { name, unit });
        self
    }

    /// A change goes to all vaults by default.
    fn shared(mut self) -> Self {
        self.shared = true;
        self
    }

    /// A device setting.
    fn device(mut self) -> Self {
        self.device = true;
        self
    }

    fn warning(mut self, warning: &'static str) -> Self {
        self.warning = Some(warning);
        self
    }

    fn check(mut self, check: TextCheck) -> Self {
        self.check = Some(check);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Bool,
    Number {
        min: f64,
        max: f64,
        step: f64,
    },
    Choice {
        options: Vec<Choice>,
    },
    /// A string; `placeholder` is an example value in the empty field.
    Text {
        placeholder: &'static str,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Group {
    pub key: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Schema {
    pub groups: Vec<Group>,
    pub settings: Vec<SettingDef>,
}

fn choice(value: &str, label: &str) -> Choice {
    Choice { value: value.into(), label: label.into() }
}

/// The device the defaults of device settings are picked for: what is fine on
/// a computer is not on a phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Phone,
}

impl Platform {
    /// The build platform.
    pub fn current() -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) { Self::Phone } else { Self::Desktop }
    }

    /// The default for this platform.
    fn pick<T>(self, desktop: T, phone: T) -> T {
        match self {
            Self::Desktop => desktop,
            Self::Phone => phone,
        }
    }
}

/// Device settings as the core applies them (`Notes::apply_device`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub warm: WarmMode,
    /// Builds of different notes at once.
    pub builds: usize,
    /// The limit of pages in memory, in bytes.
    pub memory: usize,
    pub disk: DiskLimits,
    /// How many Typst builds remember figures (`comemo::evict`).
    pub memo: usize,
    /// Typst packages beyond the allowlist ([`crate::packages`]).
    pub packages: Vec<String>,
}

/// A megabyte in bytes.
const MB: u64 = 1 << 20;

impl Schema {
    /// The schema for a set of themes (the theme is a choice of those in
    /// `theme.typ`) and a platform (defaults of the device settings).
    #[expect(clippy::too_many_lines, reason = "a table of settings, not logic")]
    pub fn new(themes: &[Theme], platform: Platform) -> Self {
        let mut theme_options = vec![choice("auto", "как в системе")];
        theme_options.extend(themes.iter().map(|t| choice(&t.name, &t.title)));
        let choices = |options: &[(&str, &str)]| Kind::Choice {
            options: options.iter().map(|(value, label)| choice(value, label)).collect(),
        };
        let flag = |key, label, default: bool| SettingDef::new(key, label, Kind::Bool, json!(default));
        // Graph forces, % of the usual ones: the graph page, for all vaults.
        let percent = |key, label, help, (min, max): (u32, u32), default: u32| {
            let kind = Kind::Number { min: f64::from(min), max: f64::from(max), step: 5.0 };
            SettingDef::new(key, label, kind, json!(default)).help(help).shared()
        };
        let number = |key, label, help, (min, max, step): (f64, f64, f64), default: u64| {
            SettingDef::new(key, label, Kind::Number { min, max, step }, json!(default)).help(help).device()
        };
        let p = platform;

        // Labels, help and warnings are client interface text, so they stay
        // Russian (the user's decision: the interface is Russian).
        Self {
            groups: vec![
                Group { key: "appearance", label: "Внешний вид" },
                Group { key: "header", label: "Шапка заметки" },
                Group { key: "headings", label: "Заголовки" },
                Group { key: "books", label: "Книги" },
                Group { key: "figures", label: "Рисунки" },
                Group { key: "panels", label: "Панели" },
                Group { key: "graph", label: "Граф" },
                Group { key: "refresh", label: "Обновление" },
                Group { key: "device", label: "Это устройство" },
            ],
            settings: vec![
                SettingDef::new("appearance.theme", "Тема", Kind::Choice { options: theme_options }, json!("auto"))
                    .shared(),
                SettingDef::new(
                    "appearance.font_size",
                    "Кегль текста, px",
                    Kind::Number { min: 12.0, max: 32.0, step: 1.0 },
                    json!(19),
                )
                .help("Рисунки масштабируются вместе с текстом")
                .var("--k-size", "px")
                .shared(),
                SettingDef::new(
                    "appearance.measure",
                    "Ширина колонки, em",
                    Kind::Number { min: 25.0, max: 80.0, step: 1.0 },
                    json!(40),
                )
                .help("Удобно читать при 35–45 em")
                .var("--k-measure", "em"),
                flag("header.title", "Название", true).attr("data-header-title"),
                flag("header.kind", "Надпись над названием («Конспект»)", true).attr("data-header-kind"),
                flag("header.description", "Описание", true).attr("data-header-description"),
                flag("header.byline", "Автор и дата", true).attr("data-header-byline"),
                flag("header.tags", "Теги", true).attr("data-header-tags"),
                SettingDef::new(
                    "headings.numbering",
                    "Номера заголовков",
                    choices(&[("books", "только в книгах"), ("all", "везде"), ("none", "нигде")]),
                    json!("books"),
                )
                .help("Номера рисунков и определений не меняются")
                .attr("data-numbering"),
                SettingDef::new(
                    "headings.chapters",
                    "Главы книг",
                    choices(&[("decorated", "«Глава N» и крупная цифра"), ("plain", "простой заголовок")]),
                    json!("decorated"),
                )
                .attr("data-chapters"),
                SettingDef::new(
                    "books.pages",
                    "Показывать книгу",
                    choices(&[("chapters", "по главам"), ("whole", "целиком")]),
                    json!("chapters"),
                )
                .help("По главам — быстрее открывается; печать браузера видит только открытую главу"),
                SettingDef::new(
                    "figures.precision",
                    "Точность координат",
                    choices(&[
                        ("full", "как в Typst (без округления)"),
                        ("3", "0,001 pt"),
                        ("2", "0,01 pt"),
                        ("1", "0,1 pt — самая лёгкая"),
                    ]),
                    json!("2"),
                )
                .help("Грубее — страница легче. Вид при 0,01 pt не отличить от точного"),
                flag("panels.toc", "Оглавление сбоку, если хватает места", true).attr("data-toc"),
                SettingDef::new(
                    "panels.toc_depth",
                    "Уровней в оглавлении",
                    Kind::Number { min: 1.0, max: 4.0, step: 1.0 },
                    json!(2),
                )
                .help("1 — только главы книги или разделы заметки"),
                flag("panels.backlinks", "«Ссылаются сюда» под заметкой", true).attr("data-backlinks"),
                percent(
                    "graph.clusters",
                    "Папки, %",
                    "Узлы папки держатся вместе, между папками - просвет",
                    (0, 300),
                    5,
                ),
                percent("graph.repel", "Отталкивание узлов, %", "Больше - граф просторнее", (25, 300), 100),
                percent("graph.center", "Притяжение к центру, %", "Меньше - граф просторнее", (25, 300), 100),
                percent("graph.links", "Притяжение связей, %", "Больше - связанные заметки ближе", (25, 500), 100),
                percent(
                    "graph.pull",
                    "Соседи тянутся за узлом, %",
                    "Насколько соседи следуют за перетаскиваемым узлом",
                    (0, 300),
                    100,
                ),
                percent(
                    "graph.return",
                    "Соседи возвращаются, %",
                    "Какую часть пути назад соседи проходят, когда узел отпустили",
                    (0, 100),
                    25,
                ),
                SettingDef::new(
                    "refresh.mode",
                    "Показывать изменения заметок",
                    choices(&[("auto", "автоматически"), ("manual", "только по кнопке «Обновить»")]),
                    json!("auto"),
                )
                .help("Автоматически — сразу после правки файла и при возврате в окно"),
                SettingDef::new(
                    "device.warm",
                    "Собирать заметки заранее",
                    choices(&[
                        (WarmMode::All.key(), "всё хранилище"),
                        (WarmMode::Open.key(), "только открытое"),
                        (WarmMode::Off.key(), "нет"),
                    ]),
                    json!(p.pick(WarmMode::All, WarmMode::Off).key()),
                )
                .help("В фоне, чтобы открывались сразу. Открытое — вкладки и недавние")
                .device(),
                number(
                    "device.builds",
                    "Сборок одновременно",
                    "Больше — быстрее прогрев, но больше памяти и нагрузки",
                    (1.0, 8.0, 1.0),
                    2,
                ),
                number(
                    "device.memo",
                    "Память Typst, сборок",
                    "Сколько последних сборок Typst помнит рисунки: пересборка после правки быстрее, но больше памяти",
                    (0.0, 50.0, 1.0),
                    p.pick(10, 3),
                ),
                number(
                    "device.memory",
                    "Страницы в памяти, МБ",
                    "Открытые заметки; лишние вытесняются и читаются с диска",
                    (16.0, 1024.0, 16.0),
                    p.pick(64, 32),
                ),
                number("device.disk", "Кэш на диске, МБ", "Собранные заметки", (64.0, 8192.0, 64.0), p.pick(512, 256)),
                SettingDef::new(
                    "device.packages",
                    "Пакеты Typst сверх белого списка",
                    Kind::Text { placeholder: "@preview/имя:версия" },
                    json!(""),
                )
                .help(
                    "Через пробел, с версией: @preview/fletcher:0.5.8. Без списка заметки берут только пакеты белого списка (CeTZ)",
                )
                .warning(
                    "Пакет — чужой код: он выполняется при каждой сборке заметки и может её повесить или \
                     подменить содержимое. Добавляйте только пакеты авторов, которым доверяете.",
                )
                .check(|text| crate::packages::parse_list(text).map(|list| list.join(" ")))
                .device(),
                number(
                    "device.foreign_days",
                    "Кэш других хранилищ и версий, дней",
                    "Столько хранится кэш, который не обновлялся",
                    (1.0, 365.0, 1.0),
                    14,
                ),
            ],
        }
    }

    pub fn get(&self, key: &str) -> Option<&SettingDef> {
        self.settings.iter().find(|s| s.key == key)
    }

    pub fn defaults(&self) -> Map<String, Value> {
        self.settings.iter().map(|s| (s.key.to_owned(), s.default.clone())).collect()
    }

    /// Checks a value; numbers are normalized to the step, and going out of
    /// bounds is an error rather than silently clamped.
    pub fn validate(&self, key: &str, value: &Value) -> Result<Value> {
        let def = self.get(key).ok_or_else(|| setting_err(key, "no such setting"))?;
        match &def.kind {
            Kind::Bool => value.as_bool().map(Value::Bool).ok_or_else(|| setting_err(key, "expected true or false")),
            Kind::Number { min, max, .. } => {
                let n = value.as_f64().ok_or_else(|| setting_err(key, "expected a number"))?;
                if n < *min || n > *max {
                    return Err(setting_err(key, &format!("allowed from {min} to {max}")));
                }
                // A whole number stays whole: 19, not 19.0.
                #[expect(clippy::cast_possible_truncation, reason = "a whole number within the setting's range")]
                Ok(if n.fract() == 0.0 { json!(n as i64) } else { json!(n) })
            }
            Kind::Text { .. } => {
                let s = value.as_str().ok_or_else(|| setting_err(key, "expected a string"))?.trim();
                match def.check {
                    Some(check) => check(s).map(Value::String).map_err(|e| setting_err(key, &e)),
                    None => Ok(json!(s)),
                }
            }
            Kind::Choice { options } => {
                let s = value.as_str().ok_or_else(|| setting_err(key, "expected a string"))?;
                if options.iter().any(|o| o.value == s) {
                    Ok(json!(s))
                } else {
                    let all: Vec<_> = options.iter().map(|o| o.value.as_str()).collect();
                    Err(setting_err(key, &format!("allowed: {}", all.join(", "))))
                }
            }
        }
    }
}

/// Old keys of the settings file -> current ones: polling every N seconds and
/// "check on returning to the window" became one choice, `refresh.mode`
/// (0 seconds means "only on the button").
fn migrate(mut stored: Map<String, Value>) -> Map<String, Value> {
    let interval = stored.remove("refresh.interval");
    stored.remove("refresh.on_focus");
    if let Some(interval) = interval
        && !stored.contains_key("refresh.mode")
    {
        let manual = interval.as_f64() == Some(0.0);
        stored.insert("refresh.mode".into(), json!(if manual { "manual" } else { "auto" }));
    }
    stored
}

fn setting_err(key: &str, reason: &str) -> Error {
    Error::Setting { key: key.to_owned(), reason: reason.to_owned() }
}

/// Settings in a file. Thread-safe; writes are atomic (a temporary file and a
/// rename), so a failure halfway does not spoil the file.
#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    schema: Schema,
    values: RwLock<Map<String, Value>>,
}

/// Values from the file `path`, checked by the schema: invalid ones and those
/// rejected by `keep` are dropped with a warning; no file means none.
fn load(
    path: &Path,
    schema: &Schema,
    keep: impl Fn(&str) -> std::result::Result<(), &'static str>,
) -> Result<Map<String, Value>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let stored: Map<String, Value> = serde_json::from_str(&text)?;
    let mut values = Map::new();
    for (key, value) in migrate(stored) {
        match keep(&key).map_err(|reason| setting_err(&key, reason)).and_then(|()| schema.validate(&key, &value)) {
            Ok(v) => {
                values.insert(key, v);
            }
            Err(e) => tracing::warn!("{}: {e}; skipping", path.display()),
        }
    }
    Ok(values)
}

impl SettingsStore {
    pub fn open(path: impl Into<PathBuf>, schema: Schema) -> Result<Self> {
        let path = path.into();
        let mut values = schema.defaults();
        values.extend(load(&path, &schema, |_| Ok(()))?);
        Ok(Self { path, schema, values: RwLock::new(values) })
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn values(&self) -> Map<String, Value> {
        self.values.read().clone()
    }

    /// Figure processing by the `figures.*` settings.
    pub fn figure_options(&self) -> FigureOptions {
        figure_options(&self.values.read())
    }

    /// Device settings for the core.
    pub fn device(&self) -> Device {
        let values = self.values.read();
        // Every key below has a number default in the schema (test `device_settings_by_platform`).
        let number = |key: &str| {
            let n = values.get(key).and_then(Value::as_u64);
            n.or_else(|| self.schema.get(key)?.default.as_u64()).unwrap_or_default()
        };
        let warm = values.get("device.warm").and_then(Value::as_str).and_then(WarmMode::from_key);
        let size = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
        Device {
            warm: warm.unwrap_or_default(),
            builds: size(number("device.builds")),
            memory: size(number("device.memory") * MB),
            disk: DiskLimits {
                size: number("device.disk") * MB,
                foreign_ttl: Duration::from_hours(24 * number("device.foreign_days")),
            },
            memo: size(number("device.memo")),
            packages: values
                .get("device.packages")
                .and_then(Value::as_str)
                .and_then(|s| crate::packages::parse_list(s).ok())
                .unwrap_or_default(),
        }
    }

    /// Changes several settings at once: either all are valid and written, or none.
    pub fn update(&self, patch: &Map<String, Value>) -> Result<Map<String, Value>> {
        let mut checked = Vec::with_capacity(patch.len());
        for (key, value) in patch {
            checked.push((key.clone(), self.schema.validate(key, value)?));
        }
        let mut values = self.values.write();
        let mut next = values.clone();
        next.extend(checked);
        write_atomic(&self.path, &serde_json::to_vec_pretty(&next)?)?;
        *values = next;
        Ok(values.clone())
    }
}

/// Figure processing by the values of the `figures.*` settings.
pub fn figure_options(values: &Map<String, Value>) -> FigureOptions {
    match values.get("figures.precision").and_then(Value::as_str) {
        Some("full") => FigureOptions { precision: None },
        Some(p) => p.parse().map_or_else(|_| FigureOptions::default(), |p| FigureOptions { precision: Some(p) }),
        None => FigureOptions::default(),
    }
}

/// The settings of one vault: only those set in it (on top of the shared ones).
/// Without a file (`path` = `None`, an in-memory vault) they live in memory only.
#[derive(Debug)]
pub struct VaultSettings {
    path: Option<PathBuf>,
    own: RwLock<Map<String, Value>>,
}

/// Device settings are shared: a vault does not have them.
fn not_device(key: &str) -> std::result::Result<(), &'static str> {
    if key.starts_with("device.") { Err("a device setting is shared by all vaults") } else { Ok(()) }
}

impl VaultSettings {
    pub fn open(path: Option<PathBuf>, schema: &Schema) -> Result<Self> {
        let Some(path) = path else { return Ok(Self::in_memory()) };
        let own = load(&path, schema, not_device)?;
        Ok(Self { path: Some(path), own: RwLock::new(own) })
    }

    /// No settings of its own, changes are kept in memory only.
    pub fn in_memory() -> Self {
        Self { path: None, own: RwLock::default() }
    }

    /// Those set in the vault.
    pub fn own(&self) -> Map<String, Value> {
        self.own.read().clone()
    }

    /// Values for the vault: the shared ones with its own on top.
    pub fn merged(&self, mut shared: Map<String, Value>) -> Map<String, Value> {
        shared.extend(self.own());
        shared
    }

    /// Sets vault settings (`null` removes one: the shared one again); either
    /// all are valid and written, or none. Returns those set in the vault.
    pub fn update(&self, schema: &Schema, patch: &Map<String, Value>) -> Result<Map<String, Value>> {
        let mut checked = Vec::with_capacity(patch.len());
        for (key, value) in patch {
            not_device(key).map_err(|reason| setting_err(key, reason))?;
            let value = if value.is_null() {
                schema.get(key).ok_or_else(|| setting_err(key, "no such setting"))?;
                None
            } else {
                Some(schema.validate(key, value)?)
            };
            checked.push((key.clone(), value));
        }
        let mut own = self.own.write();
        let mut next = own.clone();
        for (key, value) in checked {
            match value {
                Some(v) => next.insert(key, v),
                None => next.remove(&key),
            };
        }
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
            }
            write_atomic(path, &serde_json::to_vec_pretty(&next)?)?;
        }
        *own = next;
        Ok(own.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> [Theme; 2] {
        [
            Theme { name: "classic".into(), title: "Классика".into(), dark: false },
            Theme { name: "night".into(), title: "Ночь".into(), dark: true },
        ]
    }

    fn schema() -> Schema {
        Schema::new(&themes(), Platform::Desktop)
    }

    #[test]
    fn keys_are_unique_and_grouped() {
        let s = schema();
        let mut keys: Vec<_> = s.settings.iter().map(|d| d.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), s.settings.len());
        for d in &s.settings {
            let group = d.key.split('.').next().unwrap();
            assert!(s.groups.iter().any(|g| g.key == group), "{} has no group", d.key);
            assert_eq!(s.validate(d.key, &d.default).unwrap(), d.default, "{}: invalid default", d.key);
        }
    }

    #[test]
    fn appearance_applies_by_schema() {
        let s = schema();
        let mut names: Vec<_> = s
            .settings
            .iter()
            .filter_map(|d| match d.apply {
                Some(Apply::Attr { name }) => {
                    assert!(name.starts_with("data-"), "{}: the attribute {name} is not data-...", d.key);
                    Some(name)
                }
                Some(Apply::Var { name, .. }) => {
                    assert!(name.starts_with("--"), "{}: the variable {name} is not --...", d.key);
                    Some(name)
                }
                None => None,
            })
            .collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n, "two settings apply to one attribute");
        // This is how the client sees the schema (appearance.ts).
        let json = serde_json::to_value(s.get("headings.numbering").unwrap()).unwrap();
        assert_eq!(json["apply"], json!({ "to": "attr", "name": "data-numbering" }));
        let json = serde_json::to_value(s.get("appearance.font_size").unwrap()).unwrap();
        assert_eq!(json["apply"], json!({ "to": "var", "name": "--k-size", "unit": "px" }));
        assert!(serde_json::to_value(s.get("appearance.theme").unwrap()).unwrap().get("apply").is_none());
    }

    #[test]
    fn validation() {
        let s = schema();
        assert!(s.validate("appearance.theme", &json!("night")).is_ok());
        assert!(s.validate("appearance.theme", &json!("нет")).is_err());
        assert!(s.validate("appearance.font_size", &json!(100)).is_err());
        assert!(s.validate("header.title", &json!("да")).is_err());
        assert!(s.validate("нет.такой", &json!(1)).is_err());
    }

    #[test]
    fn store_round_trip_and_atomic_update() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"header.title": false, "appearance.font_size": 999, "старое": 1}"#).unwrap();

        let store = SettingsStore::open(&path, schema()).unwrap();
        let v = store.values();
        assert_eq!(v["header.title"], json!(false));
        assert_eq!(v["appearance.font_size"], json!(19), "an invalid value -> the default");
        assert!(!v.contains_key("старое"));

        let mut changes = Map::new();
        changes.insert("refresh.mode".into(), json!("manual"));
        changes.insert("header.tags".into(), json!("нет"));
        assert!(store.update(&changes).is_err());
        assert_eq!(store.values()["refresh.mode"], json!("auto"), "nothing is applied partially");

        changes.remove("header.tags");
        store.update(&changes).unwrap();
        let reopened = SettingsStore::open(&path, schema()).unwrap();
        assert_eq!(reopened.values()["refresh.mode"], json!("manual"));
        assert_eq!(reopened.figure_options(), FigureOptions::default());

        let mut changes = Map::new();
        changes.insert("figures.precision".into(), json!("full"));
        store.update(&changes).unwrap();
        assert_eq!(store.figure_options(), FigureOptions { precision: None });
    }

    #[test]
    fn old_refresh_keys_are_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let open = |text: &str| {
            fs::write(&path, text).unwrap();
            SettingsStore::open(&path, schema()).unwrap().values()
        };
        let v = open(r#"{"refresh.interval": 0, "refresh.on_focus": false}"#);
        assert_eq!(v["refresh.mode"], json!("manual"));
        assert!(!v.contains_key("refresh.interval") && !v.contains_key("refresh.on_focus"));
        assert_eq!(open(r#"{"refresh.interval": 30}"#)["refresh.mode"], json!("auto"));
        assert_eq!(open(r#"{"refresh.interval": 0, "refresh.mode": "auto"}"#)["refresh.mode"], json!("auto"));
    }

    #[test]
    fn device_settings_by_platform() {
        let desktop = schema();
        let phone = Schema::new(&themes(), Platform::Phone);
        for s in [&desktop, &phone] {
            for d in &s.settings {
                assert_eq!(
                    d.device,
                    d.key.starts_with("device."),
                    "{}: device settings are in the device group",
                    d.key
                );
                assert!(!(d.device && d.shared), "{}: a device setting is shared anyway", d.key);
            }
        }
        assert_eq!(desktop.get("device.warm").unwrap().default, json!("all"));
        assert_eq!(phone.get("device.warm").unwrap().default, json!("off"), "no warming on a phone");
        assert_eq!(desktop.get("device.builds").unwrap().default, json!(2));

        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path().join("settings.json"), schema()).unwrap();
        let d = store.device();
        assert_eq!(d.warm, WarmMode::All);
        assert_eq!(d.builds, 2);
        assert_eq!(d.memory, crate::page_cache::MEMORY_BUDGET);
        assert_eq!(d.disk, DiskLimits::default());
        assert_eq!(d.memo, crate::world::MEMO);
        let mut changes = Map::new();
        changes.insert("device.warm".into(), json!("open"));
        changes.insert("device.memory".into(), json!(128));
        store.update(&changes).unwrap();
        assert_eq!(store.device().warm, WarmMode::Open);
        assert_eq!(store.device().memory, 128 << 20);
        assert_eq!(serde_json::to_value(desktop.get("device.warm").unwrap()).unwrap()["device"], json!(true));
        assert!(serde_json::to_value(desktop.get("header.title").unwrap()).unwrap().get("device").is_none());
    }

    #[test]
    fn vault_settings_over_shared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".baluk/settings.json");
        let shared = || SettingsStore::open(dir.path().join("shared.json"), schema()).unwrap().values();
        let vault = VaultSettings::open(Some(path.clone()), &schema()).unwrap();
        assert!(vault.own().is_empty(), "no file yet");
        assert_eq!(vault.merged(shared())["appearance.font_size"], json!(19));

        let set = |pairs: &[(&str, Value)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect();
        vault
            .update(&schema(), &set(&[("appearance.font_size", json!(22)), ("figures.precision", json!("full"))]))
            .unwrap();
        assert_eq!(vault.merged(shared())["appearance.font_size"], json!(22));
        assert_eq!(figure_options(&vault.merged(shared())), FigureOptions { precision: None });
        // Device settings are shared only.
        assert!(vault.update(&schema(), &set(&[("device.builds", json!(4))])).is_err());
        assert!(vault.update(&schema(), &set(&[("appearance.font_size", json!(999))])).is_err());

        // The file is in the vault folder; null makes the setting shared again.
        let reopened = VaultSettings::open(Some(path.clone()), &schema()).unwrap();
        assert_eq!(reopened.own().len(), 2);
        reopened.update(&schema(), &set(&[("appearance.font_size", Value::Null)])).unwrap();
        assert_eq!(reopened.merged(shared())["appearance.font_size"], json!(19));
        assert!(reopened.update(&schema(), &set(&[("нет.такой", Value::Null)])).is_err());

        // A device setting written by hand is skipped.
        fs::write(&path, r#"{"device.builds": 4, "header.title": false}"#).unwrap();
        let own = VaultSettings::open(Some(path), &schema()).unwrap().own();
        assert_eq!(own.keys().collect::<Vec<_>>(), ["header.title"]);
    }
}
