//! Настройки клиента: схема, проверка, хранение.
//!
//! Схема описана здесь, а клиент рисует форму **по схеме** — новая настройка
//! добавляется одной записью в [`Schema::new`]. Настройка вида говорит, как
//! её применить ([`Apply`]: атрибут `data-…` или CSS-переменная на `<html>`),
//! — клиент (`app/src/lib/appearance.ts`) применяет её по схеме, а правило
//! пишется в CSS (`app/src/baluk-css/`); правок TS не нужно. Значения хранятся в JSON-файле плоским словарём
//! «ключ → значение»; неизвестные ключи и неверные значения при загрузке
//! отбрасываются (с предупреждением в журнал), вместо них — значения по умолчанию.
//!
//! **Настройки устройства** ([`SettingDef::device`], группа `device`) —
//! производительность: у каждого устройства свои, со своими значениями по
//! умолчанию ([`Platform`]), и при синхронизации они не переносятся. Вид —
//! общий. Ядро применяет их на ходу ([`SettingsStore::device`] →
//! `Notes::apply_device`).

use std::fs;
use std::path::PathBuf;
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

/// Описание одной настройки.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SettingDef {
    /// `группа.имя`, например `view.numbering`.
    pub key: &'static str,
    pub label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub help: Option<&'static str>,
    #[serde(flatten)]
    pub kind: Kind,
    pub default: Value,
    /// Как клиент применяет настройку к странице; `None` — сам, в своём коде
    /// (тема, книги, обновление) или она для сервера (рисунки).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub apply: Option<Apply>,
    /// Настройка устройства: своя у каждого устройства, не синхронизируется.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub device: bool,
}

/// Применение настройки вида: значение — на `<html>`, правило — в CSS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "to", rename_all = "lowercase")]
pub enum Apply {
    /// Атрибут `name` (`data-…`) со значением строкой: `data-numbering="all"`,
    /// `data-toc="false"`.
    Attr { name: &'static str },
    /// CSS-переменная `name` (`--…`) — значение с единицей: `--k-size: 19px`.
    Var { name: &'static str, unit: &'static str },
}

impl SettingDef {
    /// Применять атрибутом `data-…` на `<html>`.
    fn attr(mut self, name: &'static str) -> Self {
        self.apply = Some(Apply::Attr { name });
        self
    }

    /// Применять CSS-переменной на `<html>`.
    fn var(mut self, name: &'static str, unit: &'static str) -> Self {
        self.apply = Some(Apply::Var { name, unit });
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Bool,
    Number { min: f64, max: f64, step: f64 },
    Choice { options: Vec<Choice> },
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

/// Устройство, под которое выбираются значения по умолчанию настроек
/// устройства: что допустимо на компьютере, на телефоне — нет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Phone,
}

impl Platform {
    /// Платформа сборки.
    pub fn current() -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) { Self::Phone } else { Self::Desktop }
    }

    /// Значение по умолчанию для этой платформы.
    fn pick<T>(self, desktop: T, phone: T) -> T {
        match self {
            Self::Desktop => desktop,
            Self::Phone => phone,
        }
    }
}

/// Настройки устройства, как их применяет ядро (`Notes::apply_device`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    pub warm: WarmMode,
    /// Сборок разных заметок одновременно.
    pub builds: usize,
    /// Предел страниц в памяти, байт.
    pub memory: usize,
    pub disk: DiskLimits,
    /// Сколько сборок Typst помнит рисунки (`comemo::evict`).
    pub memo: usize,
}

/// Мегабайт в байтах.
const MB: u64 = 1 << 20;

impl Schema {
    /// Схема для набора тем (тема — выбор из того, что есть в `theme.typ`) и
    /// платформы (значения по умолчанию настроек устройства).
    // Длинная, потому что это таблица настроек, а не логика.
    #[allow(clippy::too_many_lines)]
    pub fn new(themes: &[Theme], platform: Platform) -> Self {
        let mut theme_options = vec![choice("auto", "как в системе")];
        theme_options.extend(themes.iter().map(|t| choice(&t.name, &t.title)));
        let bool_def = |key, label, default: bool| SettingDef {
            key,
            label,
            help: None,
            kind: Kind::Bool,
            default: json!(default),
            apply: None,
            device: false,
        };
        let number = |key, label, help, (min, max, step): (f64, f64, f64), default: u64| SettingDef {
            key,
            label,
            help: Some(help),
            kind: Kind::Number { min, max, step },
            default: json!(default),
            apply: None,
            device: true,
        };
        let p = platform;

        Self {
            groups: vec![
                Group { key: "appearance", label: "Внешний вид" },
                Group { key: "header", label: "Шапка заметки" },
                Group { key: "headings", label: "Заголовки" },
                Group { key: "books", label: "Книги" },
                Group { key: "figures", label: "Рисунки" },
                Group { key: "panels", label: "Панели" },
                Group { key: "refresh", label: "Обновление" },
                Group { key: "device", label: "Это устройство" },
            ],
            settings: vec![
                SettingDef {
                    key: "appearance.theme",
                    label: "Тема",
                    help: None,
                    kind: Kind::Choice { options: theme_options },
                    default: json!("auto"),
                    apply: None,
                    device: false,
                },
                SettingDef {
                    key: "appearance.font_size",
                    label: "Кегль текста, px",
                    help: Some("Рисунки масштабируются вместе с текстом"),
                    kind: Kind::Number { min: 12.0, max: 32.0, step: 1.0 },
                    default: json!(19),
                    apply: None,
                    device: false,
                }
                .var("--k-size", "px"),
                SettingDef {
                    key: "appearance.measure",
                    label: "Ширина колонки, em",
                    help: Some("Удобно читать при 35–45 em"),
                    kind: Kind::Number { min: 25.0, max: 80.0, step: 1.0 },
                    default: json!(40),
                    apply: None,
                    device: false,
                }
                .var("--k-measure", "em"),
                bool_def("header.title", "Название", true).attr("data-header-title"),
                bool_def("header.kind", "Надпись над названием («Конспект»)", true).attr("data-header-kind"),
                bool_def("header.description", "Описание", true).attr("data-header-description"),
                bool_def("header.byline", "Автор и дата", true).attr("data-header-byline"),
                bool_def("header.tags", "Теги", true).attr("data-header-tags"),
                SettingDef {
                    key: "headings.numbering",
                    label: "Номера заголовков",
                    help: Some("Номера рисунков и определений не меняются"),
                    kind: Kind::Choice {
                        options: vec![
                            choice("books", "только в книгах"),
                            choice("all", "везде"),
                            choice("none", "нигде"),
                        ],
                    },
                    default: json!("books"),
                    apply: None,
                    device: false,
                }
                .attr("data-numbering"),
                SettingDef {
                    key: "headings.chapters",
                    label: "Главы книг",
                    help: None,
                    kind: Kind::Choice {
                        options: vec![
                            choice("decorated", "«Глава N» и крупная цифра"),
                            choice("plain", "простой заголовок"),
                        ],
                    },
                    default: json!("decorated"),
                    apply: None,
                    device: false,
                }
                .attr("data-chapters"),
                SettingDef {
                    key: "books.pages",
                    label: "Показывать книгу",
                    help: Some("По главам — быстрее открывается, но Ctrl+F ищет только в открытой главе"),
                    kind: Kind::Choice {
                        options: vec![choice("chapters", "по главам"), choice("whole", "целиком")]
                    },
                    default: json!("chapters"),
                    apply: None,
                    device: false,
                },
                SettingDef {
                    key: "figures.precision",
                    label: "Точность координат",
                    help: Some("Грубее — страница легче. Вид при 0,01 pt не отличить от точного"),
                    kind: Kind::Choice {
                        options: vec![
                            choice("full", "как в Typst (без округления)"),
                            choice("3", "0,001 pt"),
                            choice("2", "0,01 pt"),
                            choice("1", "0,1 pt — самая лёгкая"),
                        ],
                    },
                    default: json!("2"),
                    apply: None,
                    device: false,
                },
                bool_def("panels.toc", "Оглавление сбоку, если хватает места", true).attr("data-toc"),
                SettingDef {
                    key: "panels.toc_depth",
                    label: "Уровней в оглавлении",
                    help: Some("1 — только главы книги или разделы заметки"),
                    kind: Kind::Number { min: 1.0, max: 4.0, step: 1.0 },
                    default: json!(2),
                    apply: None,
                    device: false,
                },
                bool_def("panels.backlinks", "«Ссылаются сюда» под заметкой", true).attr("data-backlinks"),
                SettingDef {
                    key: "refresh.mode",
                    label: "Показывать изменения заметок",
                    help: Some("Автоматически — сразу после правки файла и при возврате в окно"),
                    kind: Kind::Choice {
                        options: vec![choice("auto", "автоматически"), choice("manual", "только по кнопке «Обновить»")],
                    },
                    default: json!("auto"),
                    apply: None,
                    device: false,
                },
                SettingDef {
                    key: "device.warm",
                    label: "Собирать заметки заранее",
                    help: Some("В фоне, чтобы открывались сразу. Открытое — вкладки и недавние"),
                    kind: Kind::Choice {
                        options: vec![
                            choice(WarmMode::All.key(), "всё хранилище"),
                            choice(WarmMode::Open.key(), "только открытое"),
                            choice(WarmMode::Off.key(), "нет"),
                        ],
                    },
                    default: json!(p.pick(WarmMode::All, WarmMode::Off).key()),
                    apply: None,
                    device: true,
                },
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

    /// Проверяет значение; числа приводятся к шагу и границам не молча —
    /// выход за границы — ошибка.
    pub fn validate(&self, key: &str, value: &Value) -> Result<Value> {
        let def = self.get(key).ok_or_else(|| setting_err(key, "нет такой настройки"))?;
        match &def.kind {
            Kind::Bool => value.as_bool().map(Value::Bool).ok_or_else(|| setting_err(key, "ожидалось да/нет")),
            Kind::Number { min, max, .. } => {
                let n = value.as_f64().ok_or_else(|| setting_err(key, "ожидалось число"))?;
                if n < *min || n > *max {
                    return Err(setting_err(key, &format!("допустимо от {min} до {max}")));
                }
                // Целое остаётся целым: 19, а не 19.0.
                #[allow(clippy::cast_possible_truncation)]
                Ok(if n.fract() == 0.0 { json!(n as i64) } else { json!(n) })
            }
            Kind::Choice { options } => {
                let s = value.as_str().ok_or_else(|| setting_err(key, "ожидалась строка"))?;
                if options.iter().any(|o| o.value == s) {
                    Ok(json!(s))
                } else {
                    let all: Vec<_> = options.iter().map(|o| o.value.as_str()).collect();
                    Err(setting_err(key, &format!("допустимо: {}", all.join(", "))))
                }
            }
        }
    }
}

/// Старые ключи файла настроек — в нынешние: опрос раз в N секунд и
/// «проверять при возврате в окно» стали одним выбором `refresh.mode`
/// (0 секунд — «только по кнопке»).
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

/// Настройки в файле. Потокобезопасно; запись — атомарная (временный файл +
/// переименование), чтобы сбой посреди записи не портил файл.
#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    schema: Schema,
    values: RwLock<Map<String, Value>>,
}

impl SettingsStore {
    pub fn open(path: impl Into<PathBuf>, schema: Schema) -> Result<Self> {
        let path = path.into();
        let mut values = schema.defaults();
        match fs::read_to_string(&path) {
            Ok(text) => {
                let stored: Map<String, Value> = serde_json::from_str(&text)?;
                for (key, value) in migrate(stored) {
                    match schema.validate(&key, &value) {
                        Ok(v) => {
                            values.insert(key, v);
                        }
                        Err(e) => tracing::warn!("{}: {e} — беру значение по умолчанию", path.display()),
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::io(&path, e)),
        }
        Ok(Self { path, schema, values: RwLock::new(values) })
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn values(&self) -> Map<String, Value> {
        self.values.read().clone()
    }

    /// Обработка рисунков по настройкам `figures.*`.
    pub fn figure_options(&self) -> FigureOptions {
        let values = self.values.read();
        match values.get("figures.precision").and_then(Value::as_str) {
            Some("full") => FigureOptions { precision: None },
            Some(p) => p.parse().map_or_else(|_| FigureOptions::default(), |p| FigureOptions { precision: Some(p) }),
            None => FigureOptions::default(),
        }
    }

    /// Настройки устройства для ядра.
    pub fn device(&self) -> Device {
        let values = self.values.read();
        let number = |key: &str| {
            let n = values.get(key).and_then(Value::as_u64);
            n.or_else(|| self.schema.get(key)?.default.as_u64()).expect("число в схеме")
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
        }
    }

    /// Меняет несколько настроек разом: либо все верны и записаны, либо ни одна.
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
            assert!(s.groups.iter().any(|g| g.key == group), "{} без группы", d.key);
            assert_eq!(s.validate(d.key, &d.default).unwrap(), d.default, "{}: неверное значение по умолчанию", d.key);
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
                    assert!(name.starts_with("data-"), "{}: атрибут {name} — не data-…", d.key);
                    Some(name)
                }
                Some(Apply::Var { name, .. }) => {
                    assert!(name.starts_with("--"), "{}: переменная {name} — не --…", d.key);
                    Some(name)
                }
                None => None,
            })
            .collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n, "два применения на один атрибут");
        // Так схему видит клиент (appearance.ts).
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
        assert_eq!(v["appearance.font_size"], json!(19), "неверное значение → по умолчанию");
        assert!(!v.contains_key("старое"));

        let mut changes = Map::new();
        changes.insert("refresh.mode".into(), json!("manual"));
        changes.insert("header.tags".into(), json!("нет"));
        assert!(store.update(&changes).is_err());
        assert_eq!(store.values()["refresh.mode"], json!("auto"), "частично не применяется");

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
                assert_eq!(d.device, d.key.starts_with("device."), "{}: настройка устройства — в группе device", d.key);
            }
        }
        assert_eq!(desktop.get("device.warm").unwrap().default, json!("all"));
        assert_eq!(phone.get("device.warm").unwrap().default, json!("off"), "на телефоне прогрева нет");
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
}
