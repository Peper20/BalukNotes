//! `notes service`: autostart of `notes serve` as a systemd user service:
//! `~/.config/systemd/user/baluk-notes.service`, `systemctl --user` (no root).
//! The service starts at login, restarts after a crash and logs to journald.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The service (systemd unit) name.
pub const UNIT: &str = "baluk-notes.service";

/// The unit file: `~/.config/systemd/user/baluk-notes.service`.
fn unit_path() -> Result<PathBuf> {
    let config = dirs::config_dir().context("user config directory not found")?;
    Ok(config.join("systemd/user").join(UNIT))
}

/// An `ExecStart` argument: quoted if it has whitespace or quotes; `%` (a
/// systemd specifier) and `$` (variable expansion) are doubled.
fn quote(arg: &str) -> String {
    let escaped = arg.replace('%', "%%").replace('$', "$$");
    if !escaped.is_empty() && !escaped.contains(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '\\' | ';')) {
        return escaped;
    }
    format!("\"{}\"", escaped.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The unit text: `<exe> <args...>`.
fn unit_text(exe: &Path, args: &[String]) -> String {
    let exec: Vec<String> =
        std::iter::once(exe.to_string_lossy().into_owned()).chain(args.iter().cloned()).map(|a| quote(&a)).collect();
    format!(
        "# Created by `notes service install`; remove with `notes service remove`.\n\
         [Unit]\n\
         Description=baluk notes: notes server (notes serve)\n\
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

/// `systemctl --user <args>`; an error carries systemctl's output.
fn systemctl(args: &[&str]) -> Result<String> {
    let out =
        Command::new("systemctl").arg("--user").args(args).output().context("run systemctl (systemd required)")?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("systemctl --user {}: {}", args.join(" "), stderr.trim());
    }
    Ok(stdout)
}

/// The service state (`active`, `inactive`, `failed`, ...): systemctl prints
/// it even with a non-zero exit code.
fn state(query: &str) -> String {
    Command::new("systemctl")
        .args(["--user", query, UNIT])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn ensure_linux() -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("autostart works only on Linux (systemd); run `notes serve` here");
    }
    Ok(())
}

/// Installs and starts the service: `notes <args...>` (`serve --addr ...`). If
/// it is already installed, the unit is rewritten and the service restarted.
pub fn install(exe: &Path, args: &[String], addr: SocketAddr) -> Result<()> {
    ensure_linux()?;
    let running = state("is-active") == "active";
    // The address is taken by something other than the service (a `notes
    // serve` started by hand): the service would crash and restart forever.
    if !running && std::net::TcpListener::bind(addr).is_err() {
        bail!("address {addr} is in use: stop the running notes serve and retry");
    }
    let path = unit_path()?;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    std::fs::write(&path, unit_text(exe, args)).with_context(|| format!("write {}", path.display()))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", UNIT])?;
    systemctl(&[if running { "restart" } else { "start" }, UNIT])?;
    out!("{}", path.display());
    let done = if running { "restarted" } else { "installed and started" };
    eprintln!("service {done}: http://{addr}/; from now on it starts at login");
    eprintln!("runs: {}", exe.display());
    eprintln!("log: journalctl --user -u {UNIT} -f");
    Ok(())
}

/// Stops the service and removes the unit.
pub fn remove() -> Result<()> {
    ensure_linux()?;
    let path = unit_path()?;
    if !path.is_file() {
        eprintln!("no service ({})", path.display());
        return Ok(());
    }
    systemctl(&["disable", "--now", UNIT])?;
    std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    systemctl(&["daemon-reload"])?;
    eprintln!("service stopped and removed; start by hand: notes serve");
    Ok(())
}

/// Whether the service is installed and running, and what it runs.
pub fn status() -> Result<()> {
    ensure_linux()?;
    let path = unit_path()?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        out!("service: not installed (notes service install)");
        return Ok(());
    };
    let exec = text.lines().find_map(|l| l.strip_prefix("ExecStart=")).unwrap_or("?");
    out!("service:  {} ({})", state("is-active"), state("is-enabled"));
    out!("runs:     {exec}");
    out!("unit:     {}", path.display());
    out!("log:      journalctl --user -u {UNIT} -f");
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
