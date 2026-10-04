//! `notes`: a thin command that hands its arguments to an app part in its own
//! directory (`notes app` -> `notes-app`, the rest -> `notes-typst`) and builds
//! nothing itself (architecture §1). Help is the help of `notes-typst` plus
//! the list of parts; `notes --version` lists the parts' versions.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode};

use notes::{PARTS, VERSION, VERSION_ENV};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let text: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let dir = match notes::parts_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("error: notes directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let command = notes::command(&text);
    if command.is_none() && text.iter().any(|a| a == "-V" || a == "--version") {
        let _ = writeln!(std::io::stdout(), "notes {VERSION}");
        print_parts(&dir);
        return ExitCode::SUCCESS;
    }
    let help = command.is_none_or(|c| c == "help");
    let part = notes::part_for(&text);
    let path = part.path(&dir);
    if !path.is_file() {
        if help {
            let _ = writeln!(std::io::stdout(), "notes {VERSION}: notes in Typst");
            print_parts(&dir);
            return ExitCode::SUCCESS;
        }
        eprintln!(
            "error: \"notes {}\" belongs to the part {} ({}), which is not installed: no {}",
            command.unwrap_or_default(),
            part.bin,
            part.about,
            path.display()
        );
        return ExitCode::FAILURE;
    }
    let mut child = Command::new(&path);
    child.args(&args).env(VERSION_ENV, VERSION);
    if help {
        // The part's help, then which parts are installed.
        let code = run(&mut child);
        let _ = writeln!(std::io::stdout());
        print_parts(&dir);
        return code;
    }
    exec(child, &path)
}

/// Lists the parts. Write errors are ignored: `notes --help | head` closes
/// the pipe early, and `println!` would panic.
fn print_parts(dir: &Path) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "Parts (in {}):", dir.display());
    for part in PARTS {
        let state = if part.path(dir).is_file() { "yes" } else { "no " };
        let _ = writeln!(out, "  {state}  {:<12} {}", part.bin, part.about);
    }
}

fn run(child: &mut Command) -> ExitCode {
    match child.status() {
        Ok(status) => exit_code(status.code()),
        Err(e) => {
            eprintln!("error: starting {}: {e}", child.get_program().display());
            ExitCode::FAILURE
        }
    }
}

/// Unix: this process becomes the part (signals, input and exit code are its own).
#[cfg(unix)]
fn exec(mut child: Command, path: &Path) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let e = child.exec();
    eprintln!("error: starting {}: {e}", path.display());
    ExitCode::FAILURE
}

#[cfg(not(unix))]
fn exec(mut child: Command, _path: &Path) -> ExitCode {
    run(&mut child)
}

fn exit_code(code: Option<i32>) -> ExitCode {
    code.and_then(|c| u8::try_from(c).ok()).map_or(ExitCode::FAILURE, ExitCode::from)
}
