//! `notes` calls a part from its own directory: a copy of the binary in a
//! temporary directory and a fake part, a shell script.

use crate::common::{fake_part, notes, setup};

#[test]
fn passes_args_version_and_exit_code() {
    let (dir, bin) = setup();
    fake_part(dir.path(), "notes-typst");
    let (code, out, _) = notes(&bin, &["--vault", "app", "list", "-x"]);
    assert_eq!(code, Some(7));
    assert!(out.ends_with(&format!("notes-typst {} --vault app list -x\n", notes_parts::VERSION)), "{out}");
}

#[test]
fn app_goes_to_its_part() {
    let (dir, bin) = setup();
    fake_part(dir.path(), "notes-app");
    let (_, out, _) = notes(&bin, &["app"]);
    assert!(out.contains("notes-app"), "{out}");
}

#[test]
fn hub_commands_go_to_the_hub_part() {
    let (dir, bin) = setup();
    fake_part(dir.path(), "notes-hub");
    fake_part(dir.path(), "notes-typst");
    for args in [&["hub", "serve"][..], &["--data", "d", "hub", "serve"], &["users", "list", "--data", "d"]] {
        let (code, out, _) = notes(&bin, args);
        assert_eq!(code, Some(7));
        assert!(out.contains("notes-hub") && out.ends_with(&format!(" {}\n", args.join(" "))), "{out}");
    }
    let (_, out, _) = notes(&bin, &["list", "hub"]);
    assert!(out.contains("notes-typst"), "{out}");
}

#[test]
fn help_of_a_part_command_goes_to_the_part() {
    let (dir, bin) = setup();
    fake_part(dir.path(), "notes-typst");
    let (_, out, _) = notes(&bin, &["help", "list"]);
    assert!(out.ends_with(&format!("notes-typst {} help list\n", notes_parts::VERSION)), "{out}");
}

#[test]
fn missing_part_is_named() {
    let (_dir, bin) = setup();
    let (code, _, err) = notes(&bin, &["app"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("notes-app") && err.contains("not installed"), "{err}");
}

/// Commands of the part `notes-typst` and the unknown ones.
#[test]
fn missing_typst_part_is_named() {
    let (_dir, bin) = setup();
    for args in [&["list", "--vault", "x"][..], &["frobnicate"]] {
        let (code, out, err) = notes(&bin, args);
        assert_eq!(code, Some(1), "{args:?}");
        assert!(out.is_empty(), "{out}");
        assert!(
            err.contains(&format!("\"notes {}\" belongs to the part notes-typst", args[0]))
                && err.contains("which is not installed: no ")
                && err.contains("notes-typst"),
            "{err}"
        );
    }
}

#[test]
fn help_lists_every_command_and_the_parts() {
    let (dir, bin) = setup();
    for installed in [false, true] {
        if installed {
            fake_part(dir.path(), "notes-typst");
        }
        let (code, out, _) = notes(&bin, &["--help"]);
        assert_eq!(code, Some(0), "help is the answer of notes itself, not of a part");
        let commands = notes_parts::LIGHT.iter().chain(notes_parts::PARTS.iter().flat_map(|p| p.commands));
        for command in commands {
            assert!(out.lines().any(|l| l.trim_start().starts_with(&format!("{command} "))), "no {command}: {out}");
        }
        let typst = if installed { "yes  notes-typst" } else { "no   notes-typst" };
        assert!(out.contains(typst) && out.contains("no   notes-app") && out.contains("no   notes-hub"), "{out}");
    }
}

#[test]
fn version_lists_parts() {
    let (dir, bin) = setup();
    fake_part(dir.path(), "notes-typst");
    let (code, out, _) = notes(&bin, &["--version"]);
    assert_eq!(code, Some(0));
    assert!(out.starts_with(&format!("notes {}\n", notes_parts::VERSION)), "{out}");
    assert!(out.contains("yes  notes-typst") && out.contains("no   notes-hub"), "{out}");
}
