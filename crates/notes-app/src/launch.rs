//! The `notes app` command line and the addresses of the client: the pure
//! parts, without Tauri. The client's addresses are `/` (no vault: the last
//! one or the picker), `/v/<vault>/` (vault home) and `/v/<vault>/n/<note
//! path>` (a note); a segment is percent-encoded as JS `encodeURIComponent`
//! does, `/` separates note path segments.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

pub const USAGE: &str = "\
Usage: notes app [--vault <name|path>] [<note>]

Opens the app window; with a window already open, shows the note in it.

Arguments:
  <note>   a note path from the vault root, without .typ (\"Network/SSH\");
           needs --vault

Options:
  --vault <name|path>  a vault of the data directory by name, or a directory
                       (with a \"/\")
  --data, --trash, --library, --font-path <value>
                       passed to the window's own core
  -h, --help           this text";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Run(Launch),
}

/// A launch of the window.
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    /// Global flags for the window's own core: the shared `notes::VALUE_FLAGS`
    /// and, for a vault given by a path, `--vault <absolute path>`.
    pub core_args: Vec<OsString>,
    /// The page to show: `/`, `/v/<vault>/` or `/v/<vault>/n/<note>`.
    pub start: String,
    /// Whether the command line names a vault (`start` is then not `/`).
    pub has_vault: bool,
}

/// Parses the arguments after the program name. `notes app` has the `app`
/// word as the first positional argument: it is dropped.
pub fn parse(args: &[String]) -> Result<Command, String> {
    let mut core_args: Vec<OsString> = Vec::new();
    let mut vault: Option<String> = None;
    let mut note: Option<String> = None;
    let mut command_seen = false;
    let mut only_positional = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if only_positional || !arg.starts_with('-') || arg == "-" {
            if !command_seen && arg == "app" {
                command_seen = true;
            } else if note.is_some() {
                return Err(format!("unexpected argument \"{arg}\""));
            } else {
                note = Some(arg.clone());
            }
            continue;
        }
        match arg.as_str() {
            "--" => only_positional = true,
            "-h" | "--help" => return Ok(Command::Help),
            _ => {
                let (flag, inline) = match arg.split_once('=') {
                    Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_owned())),
                    _ => (arg.as_str(), None),
                };
                if !notes::VALUE_FLAGS.contains(&flag) {
                    return Err(format!("unknown flag \"{arg}\""));
                }
                let Some(value) = inline.or_else(|| args.next().cloned()) else {
                    return Err(format!("the flag {flag} needs a value"));
                };
                if flag == "--vault" {
                    if value.is_empty() {
                        return Err("the flag --vault needs a name or a path".into());
                    }
                    vault = Some(value);
                } else {
                    core_args.push(flag.into());
                    core_args.push(value.into());
                }
            }
        }
    }
    let Some(vault) = vault else {
        if note.is_some() {
            return Err("name a vault: notes app --vault \"Name\" \"Path\"".into());
        }
        return Ok(Command::Run(Launch { core_args, start: "/".into(), has_vault: false }));
    };
    let name = if is_path(&vault) {
        let dir = resolve_dir(&vault)?;
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .ok_or_else(|| format!("the directory name of {} does not fit a vault name", dir.display()))?;
        core_args.push("--vault".into());
        core_args.push(dir.into_os_string());
        name
    } else {
        vault
    };
    Ok(Command::Run(Launch { core_args, start: vault_path(&name, note.as_deref()), has_vault: true }))
}

/// Same rule as `--vault` of `notes-typst`: a path has a separator or is `.`/`..`.
fn is_path(vault: &str) -> bool {
    vault.contains(['/', '\\']) || vault == "." || vault == ".."
}

/// An absolute path without `.`/`..` if the directory exists; otherwise only absolute.
fn resolve_dir(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    path.canonicalize().or_else(|_| std::path::absolute(path)).map_err(|e| format!("vault directory {raw}: {e}"))
}

/// What `encodeURIComponent` leaves as is, besides letters and digits.
const COMPONENT: AsciiSet = NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

fn encode(segment: &str) -> String {
    utf8_percent_encode(segment, &COMPONENT).to_string()
}

/// The client's address of a vault, or of a note in it (`"Network/SSH"`).
pub fn vault_path(vault: &str, note: Option<&str>) -> String {
    let mut path = format!("/v/{}/", encode(vault));
    let segments = note.into_iter().flat_map(|n| n.split('/')).filter(|s| !s.is_empty());
    let mut note_path = String::new();
    for segment in segments {
        if !note_path.is_empty() {
            note_path.push('/');
        }
        note_path.push_str(&encode(segment));
    }
    if !note_path.is_empty() {
        path.push_str("n/");
        path.push_str(&note_path);
    }
    path
}

/// The vault of an address path `/v/<vault>/...`, percent-decoded.
pub fn vault_of_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/v/")?;
    let segment = rest.split('/').next().filter(|s| !s.is_empty())?;
    Some(percent_decode_str(segment).decode_utf8_lossy().into_owned())
}

/// The address path names a note (`/v/<vault>/n/...`), not only a vault.
pub fn names_note(path: &str) -> bool {
    path.strip_prefix("/v/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(_, tail)| tail.strip_prefix("n/").is_some_and(|note| !note.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Command, String> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        parse(&args)
    }

    fn launch(args: &[&str]) -> Launch {
        match run(args) {
            Ok(Command::Run(launch)) => launch,
            other => panic!("not a launch: {other:?}"),
        }
    }

    #[test]
    fn paths_are_encoded_like_the_client_does() {
        assert_eq!(vault_path("Notes", None), "/v/Notes/");
        assert_eq!(vault_path("Notes", Some("Network/SSH")), "/v/Notes/n/Network/SSH");
        assert_eq!(
            vault_path("Конспекты", Some("Сети/Это ssh")),
            "/v/%D0%9A%D0%BE%D0%BD%D1%81%D0%BF%D0%B5%D0%BA%D1%82%D1%8B/n/%D0%A1%D0%B5%D1%82%D0%B8/%D0%AD%D1%82%D0%BE%20ssh"
        );
        assert_eq!(vault_path("A", Some("C++ и C#")), "/v/A/n/C%2B%2B%20%D0%B8%20C%23");
        assert_eq!(vault_path("A", Some("50% готово")), "/v/A/n/50%25%20%D0%B3%D0%BE%D1%82%D0%BE%D0%B2%D0%BE");
        assert_eq!(vault_path("A b", Some("it's (a)!~*")), "/v/A%20b/n/it's%20(a)!~*");
        assert_eq!(vault_path("A", Some("/x//y/")), "/v/A/n/x/y");
        assert_eq!(vault_path("A", Some("")), "/v/A/");
    }

    #[test]
    fn vault_is_taken_from_a_path_and_decoded() {
        assert_eq!(vault_of_path("/v/Notes/"), Some("Notes".into()));
        assert_eq!(vault_of_path("/v/Notes"), Some("Notes".into()));
        assert_eq!(vault_of_path("/v/Notes/n/Network/SSH"), Some("Notes".into()));
        assert_eq!(vault_of_path("/v/%D0%9A%D0%BE%D0%BD/n/x"), Some("Кон".into()));
        assert_eq!(vault_of_path("/v/C%2B%2B%20%D0%B8%20C%23/"), Some("C++ и C#".into()));
        assert_eq!(vault_of_path("/v/50%25%20%D0%B3/n/x"), Some("50% г".into()));
        assert_eq!(vault_of_path("/"), None);
        assert_eq!(vault_of_path("/v/"), None);
        assert_eq!(vault_of_path("/api/settings"), None);
        assert_eq!(vault_of_path("/vault/x/"), None);
    }

    #[test]
    fn note_in_a_path_is_recognized() {
        assert!(names_note("/v/A/n/x"));
        assert!(names_note("/v/A/n/x/y"));
        assert!(!names_note("/v/A/"));
        assert!(!names_note("/v/A"));
        assert!(!names_note("/v/A/n/"));
        assert!(!names_note("/v/A/graph"));
        assert!(!names_note("/"));
    }

    #[test]
    fn path_and_vault_round_trip() {
        for name in ["Notes", "Конспекты", "C++ и C#", "50% готово", "a/b", "q?x=1&y#z"] {
            assert_eq!(vault_of_path(&vault_path(name, Some("n"))).as_deref(), Some(name));
        }
    }

    #[test]
    fn no_arguments_is_the_start_page() {
        assert_eq!(launch(&[]), Launch { core_args: vec![], start: "/".into(), has_vault: false });
        assert_eq!(launch(&["app"]).start, "/");
    }

    #[test]
    fn a_vault_name_is_not_given_to_the_core() {
        let l = launch(&["app", "--vault", "Конспекты"]);
        assert_eq!(l.start, "/v/%D0%9A%D0%BE%D0%BD%D1%81%D0%BF%D0%B5%D0%BA%D1%82%D1%8B/");
        assert!(l.core_args.is_empty());
        assert!(l.has_vault);
        let l = launch(&["app", "--vault=x", "Network/SSH"]);
        assert_eq!(l.start, "/v/x/n/Network/SSH");
        assert!(l.core_args.is_empty());
    }

    #[test]
    fn flags_may_stand_before_and_after_the_command() {
        let l = launch(&["--data", "/d", "--vault", "A", "app", "Note"]);
        assert_eq!(l.start, "/v/A/n/Note");
        assert_eq!(l.core_args, ["--data", "/d"]);
        let l = launch(&["app", "C++ и C#", "--vault=A", "--data=/d", "--font-path", "/f"]);
        assert_eq!(l.start, "/v/A/n/C%2B%2B%20%D0%B8%20C%23");
        assert_eq!(l.core_args, ["--data", "/d", "--font-path", "/f"]);
        // `app` as a flag value is not the command; the second `app` is a note.
        let l = launch(&["--vault", "app", "app", "app"]);
        assert_eq!(l.start, "/v/app/n/app");
    }

    #[test]
    fn a_vault_directory_goes_to_the_core_as_an_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("My notes");
        std::fs::create_dir(&vault).unwrap();
        let raw = vault.join("..").join("My notes");
        let l = launch(&["app", "--vault", raw.to_str().unwrap(), "Intro"]);
        assert_eq!(l.start, "/v/My%20notes/n/Intro");
        let canonical = vault.canonicalize().unwrap();
        assert_eq!(l.core_args, [OsString::from("--vault"), canonical.into_os_string()]);
        // A missing directory is made absolute (the core creates it).
        let l = launch(&["app", &format!("--vault={}/new", dir.path().display())]);
        assert_eq!(l.start, "/v/new/");
        assert!(Path::new(&l.core_args[1]).is_absolute());
    }

    #[test]
    fn dots_are_a_directory() {
        let l = launch(&["app", "--vault", "."]);
        let name = std::env::current_dir().unwrap().file_name().unwrap().to_str().unwrap().to_owned();
        assert_eq!(l.start, vault_path(&name, None));
        assert_eq!(l.core_args.len(), 2);
        assert!(Path::new(&l.core_args[1]).is_absolute());
    }

    #[test]
    fn a_note_needs_a_vault() {
        let err = run(&["app", "Network/SSH"]).unwrap_err();
        assert_eq!(err, "name a vault: notes app --vault \"Name\" \"Path\"");
    }

    #[test]
    fn help_and_errors() {
        assert_eq!(run(&["app", "-h"]), Ok(Command::Help));
        assert_eq!(run(&["--vault", "A", "app", "--help"]), Ok(Command::Help));
        assert!(run(&["app", "--bogus"]).unwrap_err().contains("--bogus"));
        assert!(run(&["app", "-x"]).unwrap_err().contains("-x"));
        assert!(run(&["app", "--vault", "A", "one", "two"]).unwrap_err().contains("two"));
        assert!(run(&["app", "--vault"]).unwrap_err().contains("needs a value"));
        assert!(run(&["app", "--vault="]).unwrap_err().contains("--vault"));
        // After `--` everything is positional.
        assert_eq!(launch(&["app", "--vault", "A", "--", "-odd"]).start, "/v/A/n/-odd");
    }
}
