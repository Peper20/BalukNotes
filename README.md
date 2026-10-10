# baluk notes

English | [Русский](README.ru.md)

Notes on Typst with the beauty of printed lecture notes and the convenience of
Obsidian: links, a graph, quick switching, search - and interactivity a PDF
does not have. A Rust core builds a vault of `.typ` files into HTML (light and
dark themes) and PDF on the device itself; today that is the desktop window
(`notes app`), next - an Android app, the core in WASM and a storage server
with sign-in (`docs/roadmap.md`); in a browser it works through `notes serve`.

## Quick start

1. **Tools** (once): Rust 1.92+ (`rustup`), Node.js 22+ with `npm`. Fonts are
   embedded; the Typst package `@preview/cetz` downloads itself on the first
   build of a note with a figure.
2. **Install** `notes` and the Claude Code skill `/baluk-note` (and after every
   repository update): `tools/install.sh` (the first build takes a few
   minutes). Then `notes` works from any folder. `notes` is a thin command: it
   hands work to the parts in its folder (`notes-typst` - building and all
   commands below; `notes-app` - the window; `notes-hub` - accounts and the
   storage server); which are installed - `notes
   --version`.
3. **Vault**: `notes vaults new "Notes"` (or in the app).
4. **App**: the window "baluk notes" from the menu, or `notes app` (`notes app
   --vault "Notes"` - on a vault, `notes app --vault "Notes" "Path"` - on a
   note). It starts its own core and stops it on exit; a second launch focuses
   the open window. A note is built when first opened (a book with figures -
   seconds), then it comes from the cache. A browser version for one's own
   machine or home server - `notes serve` (below).
5. **Note**: `notes new --vault "Notes" --title "My note"` (a book - `--book`)
   prints the file path and the command that opens it in the app; edit the file,
   the window picks it up by itself:
   ```typst
   #import "/_baluk/lib.typ": *
   #show: note.with(title: [My note])

   = Section
   Text, formulas $x^2$, a link to another note: #see("Folder/Other").
   ```
   With Claude Code - the `/baluk-note` skill from the folder with your
   materials: it starts a note or a book and follows the rules (goal, plan,
   chapters, checks).

## Commands

```sh
notes app [--vault "Name" ["Path"]]          # the app window; on a vault or a note
notes serve                                  # the browser version / self-hosting; --auth - sign-in (required beyond localhost); --socket - a socket for the window
notes sync login <server> --login <name>     # sync with a storage server: then link --vault "Name", now, status, unlink, logout
notes users add <login> | list | passwd | remove   # accounts for sign-in (notes serve --auth, notes hub)
notes hub serve                              # the storage server without Typst, for a VPS: docs/server.md
notes service install | remove | status      # autostart of notes serve for self-hosting (systemd); log - journalctl --user -u baluk-notes
notes vaults [new "Name"]                    # vaults
notes new --vault "Name" --title "Title"     # a stub; also --folder, --book, --tag, --lang
notes list --vault "Name" | notes tags --vault "Name"
notes check --vault "Name" [Path]            # build errors, warnings, broken links
notes pdf --vault "Name" Path -o x.pdf [--theme night]
notes png --vault "Name" Path -o dir [--pages 2-5]   # the PDF pages as PNG images
notes rename --vault "Name" Path "Title"     # the file name and links to it too; --dry-run
notes docs writing | library                 # how to write notes; the library API
notes info                                   # where data and vaults are
notes --version                              # version and installed parts
```

**Data** - `~/.local/share/baluk-notes` by default: `vaults/` - vaults,
`settings.json`, `cache/` (can be deleted). Another directory - in
`~/.config/baluk-notes/config.toml` (`data = "~/Notes"`), once -
`--data <dir>` or `NOTES_DATA`. A vault outside the data directory -
`--vault <path>`.

## Development

From the repository without installing - `cargo run -p notes-typst --
<command>` (a debug build takes `baluk/` and `app/dist` from disk); a test
server on a vault with every feature - `tools/test-env.sh`
(http://127.0.0.1:8432). Checks - `tools/check.sh` (`.claude/rules/tools.md`).
Repository layout, documents and rules - `.claude/rules/project.md`.

## License

Copyright (C) 2026 Ivan Baluk.

BalukNotes is under the [GNU AGPL-3.0](LICENSE) (version 3 only): the program
may be used, studied, changed and distributed, including for money, but a
modified version - including one used over a network - only with its source
code under the same license.

Notes and everything built from them (HTML, PDF) are yours: they may be
distributed on any terms, even if they use the `baluk` library
([additional permission](LICENSE-EXCEPTION)).

**A commercial license** - without the AGPL conditions (a closed product or
service based on BalukNotes): by agreement with the author,
[github.com/Peper20](https://github.com/Peper20).

Third-party code in the repository is under its own licenses, their texts lie
next to it (`vendor/`, `fonts/`). Contributions from other people are accepted
only with an agreement assigning the rights to the author (CLA): otherwise
they cannot be included in the commercial license.
