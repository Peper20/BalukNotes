---
name: baluk-note
description: BalukNotes notes in Typst - create a note or a book, add a chapter, edit or rename a note (or name a folder) by the vault rules. Runs from any folder, often the one with the sources (lectures, problems).
disable-model-invocation: true
argument-hint: "[тема, книга ..., допиши ... или переименуй ...]"
allowed-tools: Bash(notes info:*), Bash(notes vaults:*), Bash(notes docs:*), Bash(notes list:*), Bash(notes tags:*), Bash(notes new:*), Bash(notes check:*), Bash(notes rename:*), Bash(notes pdf:*), Bash(notes png:*), Bash(notes serve:*), Read, Glob, Grep
---

# BalukNotes note

**Language:** reply to the user in the language of their request. Write a new note in that language too (unless the user asks otherwise); when editing, keep the note's language. A Russian request means a Russian note, even though these instructions and samples are in English.

**User request:**

<request>
$ARGUMENTS
</request>

This is your task: work through it with the process below. If the request is empty, ask in one question which note to create or edit.

## Context

BalukNotes keeps notes as Typst files in a **vault** (a folder the user owns). The `baluk` library provides the layout: templates, text blocks, figures, interactive plots. The app (a desktop window, or a browser through `notes serve` or a storage server) renders a note as a web page; `notes pdf` prints it. The user reads mostly the page, in a light or dark theme, on a desktop or a phone; the PDF is the same text for print. So the page is the main result.

Everything you need is in the skill folder ("Base directory" at the top, usually `~/.claude/skills/baluk-note`) and in `notes docs writing`, the rules for the content of notes:
- this file: the process, paths, what is specific to the library, `notes check` messages;
- `examples/`: finished notes and a book that together use every library function a note needs; each builds without errors;
- `reference.md`: signatures of all library functions with allowed values.

The library is not in any public documentation and BalukNotes sources are not needed: the samples and the reference describe it accurately. Work through the `notes` command and by editing note files.

## Pick the task

- A. New note: one topic. The default when unsure.
- B. New book: a course or a large topic with chapters.
- C. Chapter in an existing book.
- D. Edit a note.
- E. Rename a note, book or folder, or name a folder.

Work only in the chosen vault. In it, create or change only what the task is about: the note you create, or the notes, books and folders the user named. If you see a reason to change anything else (a link in another note, a broken link, a typo), ask first and wait for the answer; the vault is the user's own writing.

## Process

1. **Vault.** `notes vaults` lists vault names, one per line. If the command is missing, tell the user to install BalukNotes (`tools/install.sh` in its repo) and stop. If there are no vaults, tell the user to create one (in the app or `notes vaults new "Name"`) and stop: vaults are the user's decision. Use the vault the user named; otherwise ask, even if there is only one, since a note in the wrong vault is easy to lose. Every `notes` command except `vaults` takes `--vault "Name"`; there is no default vault. The vault folder on disk is the first line of `notes info --vault "Name"`.
2. **Survey.** `notes list --vault "Name"` prints notes (path, `[book]`, the title in quotes when it differs from the file name, the tags; under a book, `chapter "..."` lines); `notes tags --vault "Name"` prints tags with counts. Use them to pick the folder, tags and link targets.
3. **Sources.** `ls` the current folder and the folders the user named; read what relates to the topic. Sources are read-only: create nothing in their folders.
4. **Title, folder, tags** (see "Title and path"). Decide the obvious yourself; if the place, or note vs book, is genuinely unclear, ask one question with your default first.
5. **Create** (A, B):
   ```sh
   notes new --vault "Name" --title "Title" --folder "Folder" --tag tag1 --tag tag2   # A. note
   notes new --vault "Name" --book --title "Title" --folder "Folder" --tag tag        # B. book
   ```
   `--lang en` (ISO 639 code) for a note that is not in Russian: it sets `lang:`, so block words come in that language. Output: line 1 is the file on disk (edit it), line 2 is the note path ("Path" below: for `notes check`, `notes pdf`, `#see`), line 3 is the command that opens the note in the app. Exit code 1 (`invalid note path`, `empty title`): report the message, leave existing files alone.
6. **Check the stub** with `notes check --vault "Name" "Path"` (`errors: 0, warnings: 0`) and add the progress header (see "The file"). Give the user the open command from step 5 once; do not launch the window yourself, a window that appears uninvited interrupts their work.
7. **Prepare** (once per session; skip for rename and small fixes): run `notes docs writing`, read `reference.md` and every file in `examples/` in full. The plan should use the figures and blocks the library has, and the samples show how finished notes look.
8. **Plan.** Find out the goal and the reader, propose sections (a book: chapters, then sections) with the figures and hard spots of each, and wait for approval. If the user already said what the note must contain, that is the plan.
9. **Write** one section (chapter) at a time. Start each call from its sample; the samples show the calls, not the limits: combine features and draw your own figures.
10. **Check** after each section: `notes check --vault "Name" "Path"` until `errors: 0, warnings: 0, broken links: 0`. Then update the progress header: work is often interrupted and the next session starts from it. When everything is done, delete the header.
11. **Look at the result.** `notes check` does not show overlapping labels, lines invisible in the dark theme or a table wider than a phone; the checks of the writing rules say what to look for. If you can read images:
    ```sh
    notes png --vault "Name" "Path" -o $TMP/pages                  # one PNG per page, exactly the PDF
    notes png --vault "Name" "Path" -o $TMP/pages --theme night    # dark theme; --pages 3-5 for a few pages of a book
    ```
    `$TMP` is the session temp folder (or `/tmp`), never the vault or a sources folder. If you have a browser tool, also look at the page itself; it cannot drive the app window, so start a temporary server on a free port and stop it by PID afterwards (not `pkill`, it could hit the user's processes):
    ```sh
    notes serve --addr 127.0.0.1:8439     # in the background, remember its PID; take another port if busy
    # http://127.0.0.1:8439/v/<Vault>/n/<Path>   (vault and path percent-encoded)
    kill <PID>
    ```
    Look at both themes (switch at the top right), a narrow window (about 400 px) and the interactive parts.
12. **Report**: file path, the command that opens it, what is done, what is left.

**C. Chapter.** Instead of steps 5-6 create the chapter file by hand (`notes new` makes no chapters): the book folder on disk + `NN-topic.typ`, NN following the existing chapters. It starts with `#import "/_baluk/lib.typ": *`, then `#show: chapter.with(title: [Title], tags: (...))` when the chapter has tags of its own, otherwise `= Title` (sample `examples/book/01-idea.typ`). Add `#include "NN-topic.typ"` to the book's `main.typ` after the last `#include`. Check with the book's path.

**D. Edit.** Instead of steps 5-6: the path comes from `notes list`, the file is vault folder + path + `.typ` (a book is a folder with `main.typ` and chapters). Read the whole file, change what was asked.

**E. Rename.** Use `notes rename`, not a manual move: it writes the new title into the file (a folder: `_folder.toml`), renames the file or folder after the title in the same parent folder, and rewrites `#see("path")` links to it in other notes.
```sh
notes rename --vault "Name" "Path" "New title" --dry-run   # prints the new path and the notes whose links change; changes nothing
notes rename --vault "Name" "Path" "New title"
```
The first output line is the new path, the next names the notes with rewritten links or says there are none. Those link edits are part of the rename the user asked for: list them in the report. To change only the title and keep the file name, edit `title: [...]` by hand (a book: `main.typ`; a folder: `_folder.toml`, sample `examples/_folder.toml`). Check with `notes check --vault "Name"` without a path: folder errors appear only in the full check.

## Title and path

**Title**: `title: [...]` in the file (a book: `main.typ`), any text; the tree, tabs, graph and links show it (a formula there shows as its source). **Path**: where the file is, from the vault root, `/`-separated, without `.typ`: `Math/Parabola`. `notes new` and `notes list` print it; it is the same in `notes check`, `notes pdf`, `#see(...)` and the page address. Folders are created automatically. Not a disk path, not prefixed with the vault name, no `.typ`; names starting with `_` or `.` are reserved for the app. Spaces and non-Latin letters are fine; quote the path in commands.

Folder (`--folder`): an existing one from `notes list`, or a new short one in the note's language. Tags: reuse the existing ones, they are shared across the vault. **Folder title**: `_folder.toml` in the folder, one line `title = "Networks and protocols"`, other keys are errors; without it the folder shows its name. Write it only when the user asks to name a folder.

## The file

A note file has three parts, in order:
- **progress header**: `//` lines at the top, written and kept up to date by you, not by `notes new`: goal and reader, sources, plan (and whether the user approved it), what is done, what is next. One header per file (a book: in `main.typ`). A file that already has one keeps its keys, even in another language; add one to a file without it when the work spans more than one session.
- `#import "/_baluk/lib.typ": *` and `#show: note.with(title: [...], tags: (...))`: the template. Keep them; you may change `title`, `description`, `tags` and add other fields from `reference.md` (`lang`, `words`). The text goes after the closing `)` of `#show`.
- the text.

A book's `main.typ` has `book` instead of `note` and `#include "01-topic.typ"` lines after `#show`; the text lives in chapter files. `main.typ` is the book root: what goes there and what into a chapter is in the writing rules.

```typst
// Work on the note (update after each section; delete when done):
//   goal and reader: school student before the exam, weak at graphs
//   sources: ~/study/algebra/lecture-3.pdf
//   plan: 1) vertex and branches 2) shifts 3) problems; approved
//   done: sections 1-2
//   next: section 3, problems from the lecture
```

When rewriting a file with Write, keep the header, `#import` and `#show` unchanged; line numbers and tool markers from the read output must not end up in the file. When replacing a piece with Edit, copy the old piece verbatim. Run `notes check` after every write: errors are cheap to fix one at a time.

## Library specifics

Typst is neither LaTeX nor Markdown. What is specific to this library, and the usual mistakes:

- The import is exactly `#import "/_baluk/lib.typ": *` (otherwise `file not found ... /_baluk/...`). Never set `theme`; the app picks it.
- `#` appears only in text, before a call: `#fig(canvas(...), [Caption])`, not `#fig(#canvas(...))`. `;` right after a call is eaten by Typst: write `\;` (warning about `;` after a call).
- Math: adjacent letters form one name, so `ax` is an unknown variable, write `a x`. A decimal comma needs `dc("0,5")` (warning `a comma between digits`). Russian notes use Russian operators (`tg`, `sh`, ..., see `reference.md`).
- A one-element array needs a trailing comma: `("tag",)`. Dictionary keys in `highlight:` and the like are strings. Plain numbers in canvas arguments are canvas units.
- Formulas of `interactive-plot` and `interactive-surface` are strings with their own small language: no `^` or `**` (`x * x` or `calc.pow(x, 2)`), functions as `calc.sin(x)`.
- In a Russian note the case word before a reference is written by hand (`в разделе @sec-x`).

`notes check` messages:
- `unexpected argument`, `missing argument: caption`, `unknown variable` outside math: a wrong or mistyped parameter or function; compare with the sample and `reference.md`.
- `expected comma`, `expected expression`, `unclosed delimiter`: syntax; usually a `.` instead of `,` between arguments, or a stray `#`.
- `unexpected character` or `unknown name` in a formula string: see the formula rule above.
- `no styling words`: the note language is neither `ru` nor `en`, so block words fall back to English; set your own with `words:` (keys in `reference.md`).
- `broken link`: the `#see` path is not in `notes list`, or the anchor does not match a heading text. Write the target note or drop the link.
- `_folder.toml: unknown field`: only `title` is allowed.
- `package ... is not whitelisted`: only whitelisted Typst packages may be imported (the library brings CeTZ). Draw with the library; if a package is really needed, ask the user: only they can allow it in the device settings.
- Other messages come from the library and say what to change.

## Samples: `examples/`

Each file is a whole note that builds without errors, with comments on the calls. They show what the library can do, not what a note must contain. All functions and parameters: `reference.md`.

- `blocks.typ`: template `note` with `lang`, `words`; text blocks; links `@label` and `see`; math (`dc`, `dd`, Russian operators).
- `figures.typ`: `fig`, `canvas`; 2D: axes, plots, fills, spokes, points, contours, own lines in theme colors; figures in a row; `chart`; 3D: surfaces, sections, prisms, solids of revolution.
- `algorithms.typ`: `array-cells`, `matrix-cells`, `graph` and trees; `frames` for an algorithm whose state evolves; `data-table` trace.
- `interactive.typ`: `interactive-plot`, `interactive-surface`, `frames` with a number.
- `book/` (`main.typ`, `01-idea.typ`, `02-code.typ`, `code/prefix.cpp`): template `book`, chapters (`chapter` with own tags, plain `=`), `#include`; `listing`, `code-from-file`; `vault-graph`.
- `_folder.toml`: folder title (task E).

`#see("examples/...")` in the samples points to other samples; in your note use paths from `notes list`.

---

**User request again:**

<request>
$ARGUMENTS
</request>

Reply in the language of this request and write the note in it too. Start with step 1.
