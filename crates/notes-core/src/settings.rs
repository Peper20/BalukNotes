//! Настройки клиента: схема, проверка, хранение.
//!
//! Схема описана здесь, а клиент рисует форму **по схеме** — новая настройка
//! добавляется одной записью в [`Schema::new`] и её применением в клиенте
//! (`app/src/lib/appearance.ts`). Значения хранятся в JSON-файле плоским словарём
//! «ключ → значение»; неизвестные ключи и неверные значения при загрузке
//! отбрасываются (с предупреждением в журнал), вместо них — значения по умолчанию.

use std::fs;
use std::path::PathBuf;

use parking_lot::RwLock;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::figures::FigureOptions;
use crate::fsutil::write_atomic;
use crate::themes::Theme;
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

impl Schema {
    /// Схема для набора тем (тема — выбор из того, что есть в `theme.typ`).
    // Длинная, потому что это таблица настроек, а не логика.
    #[allow(clippy::too_many_lines)]
    pub fn new(themes: &[Theme]) -> Self {
        let mut theme_options = vec![choice("auto", "как в системе")];
        theme_options.extend(themes.iter().map(|t| choice(&t.name, &t.name)));
        let bool_def = |key, label, default: bool| SettingDef {
            key,
            label,
            help: None,
            kind: Kind::Bool,
            default: json!(default),
        };

        Self {
            groups: vec![
                Group { key: "appearance", label: "Внешний вид" },
                Group { key: "header", label: "Шапка заметки" },
                Group { key: "headings", label: "Заголовки" },
                Group { key: "books", label: "Книги" },
                Group { key: "figures", label: "Рисунки" },
                Group { key: "panels", label: "Панели" },
                Group { key: "refresh", label: "Обновление" },
            ],
            settings: vec![
                SettingDef {
                    key: "appearance.theme",
                    label: "Тема",
                    help: None,
                    kind: Kind::Choice { options: theme_options },
                    default: json!("auto"),
                },
                SettingDef {
                    key: "appearance.font_size",
                    label: "Кегль текста, px",
                    help: Some("Рисунки масштабируются вместе с текстом"),
                    kind: Kind::Number { min: 12.0, max: 32.0, step: 1.0 },
                    default: json!(19),
                },
                SettingDef {
                    key: "appearance.measure",
                    label: "Ширина колонки, em",
                    help: Some("Удобно читать при 35–45 em"),
                    kind: Kind::Number { min: 25.0, max: 80.0, step: 1.0 },
                    default: json!(40),
                },
                bool_def("header.title", "Название", true),
                bool_def("header.kind", "Надпись над названием («Конспект»)", true),
                bool_def("header.description", "Описание", true),
                bool_def("header.byline", "Автор и дата", true),
                bool_def("header.tags", "Теги", true),
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
                },
                SettingDef {
                    key: "headings.chapters",
                    label: "Главы книг",
                    help: None,
                    kind: Kind::Choice {
                        options: vec![
                            choice("konspekt", "«Глава N» и крупная цифра"),
                            choice("plain", "простой заголовок"),
                        ],
                    },
                    default: json!("konspekt"),
                },
                SettingDef {
                    key: "books.pages",
                    label: "Показывать книгу",
                    help: Some("По главам — быстрее открывается, но Ctrl+F ищет только в открытой главе"),
                    kind: Kind::Choice {
                        options: vec![choice("chapters", "по главам"), choice("whole", "целиком")]
                    },
                    default: json!("chapters"),
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
                },
                bool_def("panels.toc", "Оглавление сбоку, если хватает места", true),
                SettingDef {
                    key: "panels.toc_depth",
                    label: "Уровней в оглавлении",
                    help: Some("1 — только главы книги или разделы заметки"),
                    kind: Kind::Number { min: 1.0, max: 4.0, step: 1.0 },
                    default: json!(2),
                },
                bool_def("panels.backlinks", "«Ссылаются сюда» под заметкой", true),
                SettingDef {
                    key: "refresh.interval",
                    label: "Проверять изменения, раз в N секунд",
                    help: Some("0 — только по кнопке «Обновить»"),
                    kind: Kind::Number { min: 0.0, max: 600.0, step: 1.0 },
                    default: json!(5),
                },
                bool_def("refresh.on_focus", "Проверять при возврате в окно", true),
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
                for (key, value) in stored {
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

    fn schema() -> Schema {
        Schema::new(&[Theme { name: "классика".into(), dark: false }, Theme { name: "ночь".into(), dark: true }])
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
    fn validation() {
        let s = schema();
        assert!(s.validate("appearance.theme", &json!("ночь")).is_ok());
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
        changes.insert("refresh.interval".into(), json!(0));
        changes.insert("header.tags".into(), json!("нет"));
        assert!(store.update(&changes).is_err());
        assert_eq!(store.values()["refresh.interval"], json!(5), "частично не применяется");

        changes.remove("header.tags");
        store.update(&changes).unwrap();
        let reopened = SettingsStore::open(&path, schema()).unwrap();
        assert_eq!(reopened.values()["refresh.interval"], json!(0));
        assert_eq!(reopened.figure_options(), FigureOptions::default());

        let mut changes = Map::new();
        changes.insert("figures.precision".into(), json!("full"));
        store.update(&changes).unwrap();
        assert_eq!(store.figure_options(), FigureOptions { precision: None });
    }
}
