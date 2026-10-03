//! `notes service` — автозапуск `notes serve` службой systemd пользователя:
//! `~/.config/systemd/user/baluk-notes.service`, `systemctl --user` (без
//! root). Служба стартует при входе в систему, после падения
//! перезапускается, лог — в journald.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Имя службы (юнита systemd).
pub const UNIT: &str = "baluk-notes.service";

/// Файл юнита: `~/.config/systemd/user/baluk-notes.service`.
fn unit_path() -> Result<PathBuf> {
    let config = dirs::config_dir().context("не найден каталог настроек пользователя")?;
    Ok(config.join("systemd/user").join(UNIT))
}

/// Аргумент `ExecStart`: в кавычках, если в нём пробел или кавычка; `%` —
/// спецификатор systemd, `$` — подстановка переменной: удваиваются.
fn quote(arg: &str) -> String {
    let escaped = arg.replace('%', "%%").replace('$', "$$");
    if !escaped.is_empty() && !escaped.contains(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '\\' | ';')) {
        return escaped;
    }
    format!("\"{}\"", escaped.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Текст юнита: `<exe> <args…>`.
fn unit_text(exe: &Path, args: &[String]) -> String {
    let exec: Vec<String> =
        std::iter::once(exe.to_string_lossy().into_owned()).chain(args.iter().cloned()).map(|a| quote(&a)).collect();
    format!(
        "# Создан `notes service install`; убрать — `notes service remove`.\n\
         [Unit]\n\
         Description=baluk notes: сервер заметок (notes serve)\n\
         \n\
         [Service]\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=3\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exec.join(" ")
    )
}

/// `systemctl --user <args>`: ошибка — с выводом systemctl.
fn systemctl(args: &[&str]) -> Result<String> {
    let out =
        Command::new("systemctl").arg("--user").args(args).output().context("запустить systemctl (нужен systemd)")?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("systemctl --user {}: {}", args.join(" "), stderr.trim());
    }
    Ok(stdout)
}

/// Состояние службы (`active`, `inactive`, `failed`…): systemctl печатает
/// его и при ненулевом коде.
fn state(query: &str) -> String {
    Command::new("systemctl")
        .args(["--user", query, UNIT])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn ensure_linux() -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("автозапуск — только в Linux (systemd); здесь запускайте `notes serve`");
    }
    Ok(())
}

/// Поставить и запустить службу: `notes <args…>` (`serve --addr …`). Уже
/// стоит — юнит переписывается, служба перезапускается.
pub fn install(exe: &Path, args: &[String], addr: SocketAddr) -> Result<()> {
    ensure_linux()?;
    let running = state("is-active") == "active";
    // Адрес занят не службой — запущенный вручную `notes serve`: служба
    // падала бы и перезапускалась.
    if !running && std::net::TcpListener::bind(addr).is_err() {
        bail!("адрес {addr} занят — остановите запущенный notes serve (kill $(pgrep -x notes-typst)) и повторите");
    }
    let path = unit_path()?;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).with_context(|| format!("создать {}", dir.display()))?;
    std::fs::write(&path, unit_text(exe, args)).with_context(|| format!("записать {}", path.display()))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", UNIT])?;
    systemctl(&[if running { "restart" } else { "start" }, UNIT])?;
    println!("{}", path.display());
    let done = if running { "перезапущена" } else { "поставлена и запущена" };
    eprintln!("служба {done}: http://{addr}/; дальше запускается при входе в систему");
    eprintln!("запускает: {}", exe.display());
    eprintln!("лог: journalctl --user -u {UNIT} -f");
    Ok(())
}

/// Остановить службу и убрать юнит.
pub fn remove() -> Result<()> {
    ensure_linux()?;
    let path = unit_path()?;
    if !path.is_file() {
        eprintln!("службы нет ({})", path.display());
        return Ok(());
    }
    systemctl(&["disable", "--now", UNIT])?;
    std::fs::remove_file(&path).with_context(|| format!("удалить {}", path.display()))?;
    systemctl(&["daemon-reload"])?;
    eprintln!("служба остановлена и убрана; запуск вручную — notes serve");
    Ok(())
}

/// Стоит ли служба, работает ли, что запускает.
pub fn status() -> Result<()> {
    ensure_linux()?;
    let path = unit_path()?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!("служба: не поставлена (notes service install)");
        return Ok(());
    };
    let exec = text.lines().find_map(|l| l.strip_prefix("ExecStart=")).unwrap_or("?");
    println!("служба:     {} ({})", state("is-active"), state("is-enabled"));
    println!("запускает:  {exec}");
    println!("юнит:       {}", path.display());
    println!("лог:        journalctl --user -u {UNIT} -f");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_args_are_quoted() {
        assert_eq!(quote("serve"), "serve");
        assert_eq!(quote("/home/я/Мои заметки"), "\"/home/я/Мои заметки\"");
        assert_eq!(quote("50%"), "50%%");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("$HOME"), "$$HOME");
        assert_eq!(quote(""), "\"\"");
    }

    #[test]
    fn unit_runs_given_command() {
        let text = unit_text(
            Path::new("/home/u/.local/bin/notes"),
            &["serve".into(), "--addr".into(), "127.0.0.1:8421".into()],
        );
        assert!(text.contains("\nExecStart=/home/u/.local/bin/notes serve --addr 127.0.0.1:8421\n"), "{text}");
        assert!(text.contains("\nWantedBy=default.target\n"));
    }
}
