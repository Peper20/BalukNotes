//! App parts (architecture §1): the thin `notes` finds a part in its own
//! directory and hands it the command; the part checks the version. No
//! dependencies: both `notes` and the parts use this library.

use std::path::{Path, PathBuf};

/// An app part: a separate binary next to `notes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// Binary name (without `.exe`).
    pub bin: &'static str,
    /// `notes` commands that go to this part; `notes-typst` takes all the rest.
    pub commands: &'static [&'static str],
    /// What it is, for help and messages.
    pub about: &'static str,
}

impl Part {
    /// The part's binary in `dir`.
    pub fn path(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{}{}", self.bin, std::env::consts::EXE_SUFFIX))
    }
}

/// Directory of the parts: the directory of the running binary (a symlink to it does not count).
pub fn parts_dir() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    Ok(exe.parent().map(Path::to_path_buf).unwrap_or_default())
}

/// Building notes: every command except those of other parts.
pub const TYPST: Part =
    Part { bin: "notes-typst", commands: &[], about: "builds notes: serve, new, list, check, pdf, ..." };
/// The app window.
pub const APP: Part = Part { bin: "notes-app", commands: &["app"], about: "app window: notes app" };
/// The storage server online and its accounts (`notes hub serve`, `notes users ...`).
pub const HUB: Part = Part {
    bin: "notes-hub",
    commands: &["hub", "users"],
    about: "storage server online: hub serve, users add | list | passwd | remove",
};
pub const PARTS: &[Part] = &[TYPST, APP, HUB];

/// Global `notes-typst` flags that take a value: in `notes --vault app list`
/// the command is `list`. A `notes-typst` test checks them against `clap`.
pub const VALUE_FLAGS: &[&str] = &["--data", "--vault", "--trash", "--library", "--font-path"];

/// Directory of the sockets: `$XDG_RUNTIME_DIR/baluk-notes`.
fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join("baluk-notes"))
}

/// Default core socket (`notes serve --socket`):
/// `$XDG_RUNTIME_DIR/baluk-notes/notes.sock`. The `notes-app` window looks for it.
pub fn default_socket() -> Option<PathBuf> {
    runtime_dir().map(|dir| dir.join("notes.sock"))
}

/// Socket of the running `notes-app` window, next to the core socket:
/// `$XDG_RUNTIME_DIR/baluk-notes/app.sock`. A second `notes app` hands its
/// start page to the running window over it.
pub fn app_socket() -> Option<PathBuf> {
    runtime_dir().map(|dir| dir.join("app.sock"))
}

/// Version of the `notes` that called a part: an environment variable of the part.
pub const VERSION_ENV: &str = "NOTES_VERSION";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The command: the first argument that is neither a flag nor a flag's value.
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

/// The part to hand the arguments to.
pub fn part_for<S: AsRef<str>>(args: &[S]) -> Part {
    command(args).and_then(|c| PARTS.iter().find(|p| p.commands.contains(&c))).copied().unwrap_or(TYPST)
}

/// A part called by a `notes` of another version is an error: parts are installed together.
pub fn check_version(part: &str) -> Result<(), String> {
    match std::env::var(VERSION_ENV) {
        Ok(theirs) if theirs != VERSION => Err(format!(
            "notes {theirs} and {part} {VERSION} are different versions; reinstall them together (tools/install.sh)"
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
        assert_eq!(part_for(&["hub", "serve", "--addr", "127.0.0.1:1"]), HUB);
        assert_eq!(part_for(&["--data", "d", "hub", "serve"]), HUB);
        assert_eq!(part_for(&["--data=d", "users", "add", "ivan"]), HUB);
        assert_eq!(part_for(&["list", "app"]), TYPST);
        assert_eq!(part_for(&["list", "hub"]), TYPST);
        assert_eq!(part_for::<&str>(&[]), TYPST);
    }

    #[test]
    fn sockets_share_a_directory() {
        let (core, app) = (default_socket(), app_socket());
        assert_eq!(core.is_some(), app.is_some());
        if let (Some(core), Some(app)) = (core, app) {
            assert_eq!(core.parent(), app.parent());
            assert_eq!(core.file_name().and_then(|n| n.to_str()), Some("notes.sock"));
            assert_eq!(app.file_name().and_then(|n| n.to_str()), Some("app.sock"));
        }
    }
}
