//! Части приложения (architecture §1): тонкий `notes` находит часть в своей
//! папке и передаёт ей команду, часть сверяет версию. Без зависимостей:
//! библиотеку берут и `notes`, и части.

/// Часть приложения - отдельный бинарник рядом с `notes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// Имя бинарника (без `.exe`).
    pub bin: &'static str,
    /// Команды `notes`, которые ведут в эту часть; у `notes-typst` - все
    /// остальные.
    pub commands: &'static [&'static str],
    /// Что это, для справки и сообщений.
    pub about: &'static str,
}

impl Part {
    /// Бинарник части в папке `dir`.
    pub fn path(&self, dir: &std::path::Path) -> std::path::PathBuf {
        dir.join(format!("{}{}", self.bin, std::env::consts::EXE_SUFFIX))
    }
}

/// Папка частей - папка запущенного бинарника (ссылка на него - не в счёт).
pub fn parts_dir() -> std::io::Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    Ok(exe.parent().map(std::path::Path::to_path_buf).unwrap_or_default())
}

/// Сборка заметок: все команды, кроме команд других частей.
pub const TYPST: Part =
    Part { bin: "notes-typst", commands: &[], about: "сборка заметок: serve, new, list, check, pdf, ..." };
/// Окно приложения.
pub const APP: Part = Part { bin: "notes-app", commands: &["app"], about: "окно приложения: notes app" };
pub const PARTS: &[Part] = &[TYPST, APP];

/// Общие флаги `notes-typst` со значением: в `notes --vault app list`
/// команда - `list`. Сверяет с `clap` тест `notes-typst`.
pub const VALUE_FLAGS: &[&str] = &["--data", "--vault", "--trash", "--library", "--font-path"];

/// Сокет ядра (`notes serve --socket`) по умолчанию:
/// `$XDG_RUNTIME_DIR/baluk-notes/notes.sock`. Его ищет окно `notes-app`.
pub fn default_socket() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| std::path::PathBuf::from(dir).join("baluk-notes").join("notes.sock"))
}

/// Версия `notes`, вызвавшего часть: переменная окружения части.
pub const VERSION_ENV: &str = "NOTES_VERSION";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Команда - первый аргумент, который не флаг и не значение флага.
pub fn command<S: AsRef<str>>(args: &[S]) -> Option<&str> {
    let mut args = args.iter().map(AsRef::as_ref);
    while let Some(arg) = args.next() {
        if arg == "--" {
            return args.next();
        }
        if !arg.starts_with('-') {
            return Some(arg);
        }
        if VALUE_FLAGS.contains(&arg) {
            args.next();
        }
    }
    None
}

/// Часть, которой отдать аргументы.
pub fn part_for<S: AsRef<str>>(args: &[S]) -> Part {
    command(args).and_then(|c| PARTS.iter().find(|p| p.commands.contains(&c))).copied().unwrap_or(TYPST)
}

/// Часть вызвана из `notes` другой версии - ошибка: части ставятся вместе.
pub fn check_version(part: &str) -> Result<(), String> {
    match std::env::var(VERSION_ENV) {
        Ok(theirs) if theirs != VERSION => Err(format!(
            "notes {theirs} и {part} {VERSION} - разных версий; поставьте их заново вместе (tools/install.sh)"
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_skips_flags_and_their_values() {
        assert_eq!(command(&["list"]), Some("list"));
        assert_eq!(command(&["--vault", "app", "list"]), Some("list"));
        assert_eq!(command(&["--vault=app", "list"]), Some("list"));
        assert_eq!(command(&["--help"]), None);
        assert_eq!(command::<&str>(&[]), None);
        assert_eq!(command(&["--", "app"]), Some("app"));
    }

    #[test]
    fn part_by_command() {
        assert_eq!(part_for(&["app"]), APP);
        assert_eq!(part_for(&["--data", "d", "app"]), APP);
        assert_eq!(part_for(&["list", "app"]), TYPST);
        assert_eq!(part_for::<&str>(&[]), TYPST);
    }
}
