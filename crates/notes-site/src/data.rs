//! Данные для скрипта сайта (индекс поиска, граф): `assets/data/<имя>.js`.
//!
//! Это JSON, обёрнутый в присваивание `window.balukData[имя]`: сайт
//! открывается и с диска (`file://`), а там браузер не даёт `fetch` читать
//! соседние файлы — а `<script src>` грузит. Скрипт сайта подключает файл,
//! когда данные понадобились (`app/src/static/parts.ts`, `loadData`).

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

/// Записать `value` как данные `name`.
pub(crate) fn write(dir: &Path, name: &str, value: &impl Serialize) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(format!("{name}.js")), wrap(name, &serde_json::to_string(value)?))?;
    Ok(())
}

fn wrap(name: &str, json: &str) -> String {
    format!("(window.balukData ??= {{}})[{}] = {json};\n", serde_json::Value::from(name))
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_file_assigns_json() {
        assert_eq!(super::wrap("search", r#"[{"a":1}]"#), "(window.balukData ??= {})[\"search\"] = [{\"a\":1}];\n");
    }
}
