//! `notes` - тонкая команда: передаёт аргументы части приложения из своей
//! папки (`notes app` -> `notes-app`, остальное -> `notes-typst`) и сама
//! ничего не собирает (architecture §1). Справка - справка `notes-typst` и
//! список частей; `notes --version` - версии частей.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, ExitCode};

use notes::{PARTS, VERSION, VERSION_ENV};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let text: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let dir = match notes::parts_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("ошибка: папка notes: {e}");
            return ExitCode::FAILURE;
        }
    };
    let command = notes::command(&text);
    if command.is_none() && text.iter().any(|a| a == "-V" || a == "--version") {
        println!("notes {VERSION}");
        print_parts(&dir);
        return ExitCode::SUCCESS;
    }
    let help = command.is_none_or(|c| c == "help");
    let part = notes::part_for(&text);
    let path = part.path(&dir);
    if !path.is_file() {
        if help {
            println!("notes {VERSION}: заметки на Typst");
            print_parts(&dir);
            return ExitCode::SUCCESS;
        }
        eprintln!(
            "ошибка: «notes {}» - из части {} ({}), она не установлена: нет {}",
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
        // Справка части, затем - какие части стоят.
        let code = run(&mut child);
        println!();
        print_parts(&dir);
        return code;
    }
    exec(child, &path)
}

fn print_parts(dir: &Path) {
    println!("Части (в {}):", dir.display());
    for part in PARTS {
        let state = if part.path(dir).is_file() { "есть" } else { "нет " };
        println!("  {state}  {:<12} {}", part.bin, part.about);
    }
}

fn run(child: &mut Command) -> ExitCode {
    match child.status() {
        Ok(status) => exit_code(status.code()),
        Err(e) => {
            eprintln!("ошибка: запуск {}: {e}", child.get_program().display());
            ExitCode::FAILURE
        }
    }
}

/// Unix: процесс становится частью (сигналы, ввод и код выхода - её).
#[cfg(unix)]
fn exec(mut child: Command, path: &Path) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let e = child.exec();
    eprintln!("ошибка: запуск {}: {e}", path.display());
    ExitCode::FAILURE
}

#[cfg(not(unix))]
fn exec(mut child: Command, _path: &Path) -> ExitCode {
    run(&mut child)
}

fn exit_code(code: Option<i32>) -> ExitCode {
    code.and_then(|c| u8::try_from(c).ok()).map_or(ExitCode::FAILURE, ExitCode::from)
}
