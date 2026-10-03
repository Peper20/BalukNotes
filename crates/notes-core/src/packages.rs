//! Пакеты Typst: заметка берёт только пакеты белого списка ([`ALLOWED`],
//! с версиями) и те, что пользователь разрешил сверх него (настройка
//! устройства `device.packages`). Остальные — ошибка сборки: пакет — чужой
//! код, он выполняется при каждой сборке заметки (решение пользователя).
//!
//! Версия заметки, читавшей пакет (разрешённый или нет), учитывает список
//! разрешённых: данные хранилища `/_vault/packages/policy` ([`PolicyData`]).
//! Разрешили пакет — пересоберутся заметки, читавшие пакеты (с библиотекой
//! оформления — все: она берёт CeTZ), остальные — нет. Список меняют редко.

use std::collections::BTreeSet;
use std::sync::Arc;

use parking_lot::RwLock;
use typst::syntax::package::PackageSpec;

/// Пакеты, которые заметки берут без спроса: библиотека оформления рисует ими
/// (CeTZ) и то, что тянут они сами. Пересматривать и расширять: красота
/// заметок важнее (`docs/roadmap.md`).
pub const ALLOWED: &[&str] = &["@preview/cetz:0.4.2", "@preview/oxifmt:1.0.0"];

/// Префикс данных хранилища и файл с отпечатком списка.
pub const DATA_PREFIX: &str = "packages";
pub const POLICY_FILE: &str = "packages/policy";

/// Какие пакеты можно брать: белый список и разрешённые сверх него.
#[derive(Debug, Default)]
pub struct PackagePolicy {
    extra: RwLock<BTreeSet<String>>,
}

impl PackagePolicy {
    /// Разрешённые сверх белого списка (`@пространство/имя:версия`); `true` —
    /// список изменился.
    pub fn set_extra(&self, extra: &[String]) -> bool {
        let next: BTreeSet<String> = extra.iter().cloned().collect();
        let mut current = self.extra.write();
        let changed = *current != next;
        *current = next;
        changed
    }

    /// Можно ли брать пакет; нет — текст ошибки для заметки.
    pub fn check(&self, spec: &PackageSpec) -> Result<(), String> {
        let name = spec.to_string();
        if ALLOWED.contains(&name.as_str()) || self.extra.read().contains(&name) {
            return Ok(());
        }
        Err(format!(
            "пакет {name} не из белого списка ({}); разрешить — настройка «Пакеты сверх белого списка» \
             (это чужой код: только от автора, которому доверяете)",
            ALLOWED.join(", ")
        ))
    }

    /// Отпечаток для версии заметки: разрешённые сверх списка.
    fn describe(&self) -> String {
        self.extra.read().iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

/// Список пакетов из настройки: через пробел или запятую,
/// `@пространство/имя:версия`. Ошибка — какой пакет записан неверно.
pub fn parse_list(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for item in text.split([' ', ',', '\n', '\t']).filter(|s| !s.is_empty()) {
        let spec: PackageSpec = item
            .parse()
            .map_err(|e| format!("«{item}»: {e}; нужно @пространство/имя:версия, например @preview/fletcher:0.5.8"))?;
        let name = spec.to_string();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Поставщик `/_vault/packages/policy` — список разрешённых: только для
/// версии заметок, читавших пакеты.
#[derive(Debug)]
pub struct PolicyData(pub Arc<PackagePolicy>);

impl crate::vault_data::DataProvider for PolicyData {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        match path {
            "policy" => Ok(self.0.describe().into_bytes()),
            _ => Err(format!("нет данных хранилища «{DATA_PREFIX}/{path}»")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(s: &str) -> PackageSpec {
        s.parse().unwrap()
    }

    #[test]
    fn whitelist_and_extra() {
        let policy = PackagePolicy::default();
        assert!(policy.check(&spec("@preview/cetz:0.4.2")).is_ok());
        let err = policy.check(&spec("@preview/cetz:0.4.1")).unwrap_err();
        assert!(err.starts_with("пакет @preview/cetz:0.4.1 не из белого списка"), "{err}");
        assert!(policy.check(&spec("@preview/fletcher:0.5.8")).is_err());
        assert!(policy.set_extra(&["@preview/fletcher:0.5.8".into()]));
        assert!(!policy.set_extra(&["@preview/fletcher:0.5.8".into()]), "тот же список — без изменений");
        assert!(policy.check(&spec("@preview/fletcher:0.5.8")).is_ok());
        assert!(policy.check(&spec("@preview/fletcher:0.5.7")).is_err(), "версия — точная");
    }

    #[test]
    fn list_from_setting() {
        assert_eq!(
            parse_list(" @preview/fletcher:0.5.8,@local/my:1.0.0  @preview/fletcher:0.5.8").unwrap(),
            ["@preview/fletcher:0.5.8", "@local/my:1.0.0"]
        );
        assert_eq!(parse_list("").unwrap(), Vec::<String>::new());
        assert!(parse_list("fletcher").unwrap_err().starts_with("«fletcher»"));
        assert!(parse_list("@preview/fletcher").is_err(), "без версии");
    }
}
