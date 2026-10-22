# How to write notes

Rules for the **content** of BalukNotes notes: what makes a note good and how
to check it (`notes docs writing`). The `baluk` library does the layout, its
API is `notes docs library`. A new note is started by the Claude Code skill
`/baluk-note`; everything goes through the `notes` command, from any folder:

| Command | What it does |
|---|---|
| `notes vaults`, `notes vaults new "Name"` | vaults; a new vault |
| `notes new --vault "Name" --title "Title" [--folder Folder] [--book] [--tag ...] [--lang ...]` | a note or book stub, the file name comes from the title; prints the file path, the note path and the command that opens it |
| `notes list --vault "Name"`, `notes tags --vault "Name"` | notes (path, book, title, tags) and tags |
| `notes check --vault "Name" [Path]` | build errors, warnings, broken links |
| `notes rename --vault "Name" Path "Title" [--dry-run]` | a new title, the file name and `#see` links to it follow |
| `notes pdf --vault "Name" Path -o file.pdf [--theme night]` | PDF of a note or book |
| `notes png --vault "Name" Path -o dir [--pages 2-5] [--theme night]` | the PDF pages as PNG images, one file per page |
| `notes info [--vault "Name"]` | where the vaults are, how to open the app |
| `notes app [--vault "Name" ["Path"]]` | the app window, optionally on a vault or a note; it refreshes on every save |
| `notes serve --addr 127.0.0.1:8439` | the same app in a browser at `http://127.0.0.1:8439/v/<Vault>/n/<Path>` (self-hosting; for a check by a browser tool, on a free port) |

There is no default vault: note commands always need `--vault "Name"` (the
list - `notes vaults`); the user creates the first vault.

## Essentials

A note **explains**, it does not enumerate: the reader wants to understand the
topic and then use it. They read in the app (a computer or a phone, a light or
dark theme, a chapter at a time); the PDF is the same text for printing.

- Motivation first, then the idea, then how to use it. A definition is easier
  to take in after the reader has seen why it is needed.
- Understanding usually comes from a picture or a worked example next to the
  statement, not from the statement alone. How many of them a topic needs is
  the topic's call.
- Hard parts deserve more room than easy ones; uniformly compressed text
  fails the reader exactly where they need help.
- A dense reference (tables "construct -> meaning", walls of code) is a cheat
  sheet, a separate note (`book.with(kind: [Шпаргалка], ...)`; in an English
  note `kind: [Cheat sheet]`), not a replacement for the explanation.

## Note or book

| Kind | What it is | File |
|---|---|---|
| **Note** (`note`) | one topic: a definition, a technique, a tool, a problem; `=` is a section, numbering is continuous | `Folder/Name.typ` |
| **Book** (`book`) | a course or a large topic: `=` is a chapter, numbering "chapter.n", title page and contents in PDF | `Folder/Name/main.typ` + chapters `NN-topic.typ` |

A chapter starts on a new page in PDF and is shown separately in the app, so a
short topic is a section of a note; a chapter is rather 3+ PDF pages.

**Title and path differ.** The title is `title: [...]` in the file (for a
book - in `main.typ`), any characters; the tree, tabs, graph and links show it
(a formula there shows as source: `[Ряд $sum 1/n^2$]` -> "Ряд sum 1/n^2",
better in words). The path is where the file lies (`Сеть/SSH основы`): links
(`#see`), checks and PDF builds use it. `notes new --title` takes the file name
from the title (without `/ \ : * ? " < > |`, a taken one gets a number). To
rename - `notes rename` (or in the app: right click in the tree ->
"Переименовать..."): the title and the file (folder) name in the same folder
change, and `#see` links to it in other notes are rewritten. Only the title,
without the file name - edit `title:`. A folder title is `_folder.toml` in it:
`title = "Сети и протоколы"` (no file - the folder name).

- Short notes on one idea are connected by links, as in Obsidian:
  `#see("Математика/Производная")`, a section - `anchor: "Heading"`. "Links
  here" and the graph are built from links. A link to an unwritten note is
  broken: write the note or do not link.
- Tags - `tags: ("матан", "интегралы")`: lowercase, in the note's language,
  shared across the vault (existing ones - `notes tags`).
- **Book and chapter properties.** `main.typ` is the book root:
  `book.with(...)` sets what all chapters share (language, words, tags of the
  whole topic), a chapter inherits it. A chapter's own properties, as of a
  note - `#show: chapter.with(title: [...], tags: (...))` at the start of its
  file instead of `= Title`: the title and own tags (added to the book tags;
  shown under the chapter heading and on the tag page). The first chapter is
  an ordinary chapter: what the book shares lives only in the root.
- Language - `lang:` (`"ru"` by default); for languages other than `ru` and
  `en` - own `words:`.
- Inside a note: `рис. @label`, `в разделе @sec-x` (the inflected word by
  hand). To another note - only `#see`.

## Process

1. Find out the **goal** (an exam, a contest, a lab, "to understand") and the
   **reader**: what they know well and what poorly (the less familiar gets
   more space). Gather the sources: lectures, problems, past exam papers -
   often they are in the folder the skill is called from.
2. Agree on the **plan** before the text: for a book - chapters and sections,
   for a note - sections; for each, what it explains, which pictures and
   examples carry it, where the hard parts are.
3. Write a chapter (section) at a time and check it (below). After each one,
   update the progress header of the file (the skill's "The file"), so that an
   interrupted work resumes from it.

## Text

What makes a section work, as orientation; the topic, the reader and the
user's request decide what it takes.

- Blocks (`lead`, `plan`, `definition`, `theorem`, `example`, `pitfall`,
  `summary`, ...) are tools, each earns its place by what it does for the
  reader: a `lead` says why a chapter exists and what it builds on, an
  `example` with `step`s and an `answer` shows a method at work, a `pitfall`
  names a trap where readers actually fall into it, a `summary` helps in a
  long chapter to hold the result. A short note may need none of them.
  Callouts (`remark`, `idea`, `pitfall`, `margin-note`) are seasoning: too
  many of them stop being noticed.
- A new term - `*bold*`, its explanation right next to it.
- A formula comes with words: before it - what it says, after it - how to use
  it. The key formula of a section can be set off with `#formula[...]`.
- For a hard part, examples from simple to complex, a step-by-step trace, a
  `#proof` and the common mistake explain more than one long paragraph.
- The voice follows the reader and the request: a school student and a
  reference for a colleague read differently.
- `$dc("0,5")$`, not `$0,5$` (it prints "0, 5"). Russian operators: `tg`,
  `ctg`, `sh`, `ch`.
- Typst eats `;` right after a call like `#see(...)` - write `\;` (`notes check`
  warns).
- Font size, fonts and colors only from the theme: a note does not set
  `set text(size: ...)` or `rgb(...)`. Shrinking to make things fit is not
  allowed.

## HTML is the main view

One source gives the page and the PDF, but mostly the page is read: everything
has to work in the light and the dark theme and on a narrow screen (about
400 px), not only in the PDF.

- **Layout only with library blocks**: `side-by-side` (text and a figure side
  by side), `in-row` (figures in a row), `fig`, `data-table`. In HTML Typst
  drops `grid`, `stack`, `align`, `place`, and `h`/`v` with `fr` and percents
  disappear. A new layout is needed - that is a library change: tell the user.
- A note has no HTML, JS or styles of its own (do not write `html.elem`):
  interactivity only with library blocks.
- Own lines and fills use theme colors (`theme.color.fig.second`,
  `pale(theme, color)`), otherwise they are invisible in the dark theme
  `night`.

Typst packages only from the whitelist (the library brings CeTZ itself);
another one is a build error, only the user can allow it in the device
settings ("Пакеты Typst сверх белого списка").

## Figures

A figure shows what text cannot: a shape, a structure, a comparison, a process.
Where the text describes one, a figure next to it, referenced from the text,
usually does more than another paragraph. Pictures that tend to carry the
idea: regions with a spoke, plots and 3D surfaces in calculus; arrays with
pointers, graphs with the current step, a DP table with arrows in algorithms;
the data before a query and the result in databases; a block diagram or a
pipeline by cycles in architecture; outcome trees and densities in
probability; `chart` for measurements and comparisons instead of a table of
numbers.

- **A static figure is the default.** It reads at a glance, in the PDF, in the
  dark theme and on a phone, and the reader sees the whole picture at once.
  Several states can sit side by side (`in-row`, a series of small panels).
- **Animation earns its place when the change itself is what the reader must
  see.** `frames` (a slider and playback) fits an algorithm whose state
  evolves step by step, where the reader wants to stop and go back: sorting
  passes, a table being filled. `interactive-plot` and `interactive-surface`
  (sliders, rotation) fit a dependence on parameters that a few static plots
  would not show: how a family of curves moves, how a surface bends. In the
  PDF `frames` shows one frame, or a storyboard, `pdf: (1, 4, 8)` - choose the
  frames so that the print still tells the story.
- **A caption is a statement, not a title.** Bad: "Region D". Good: "The spoke
  enters D through the parabola and leaves through the line - these are the
  limits of the inner integral".
- Highlight with one theme color (`"second"`), not a rainbow; labels no
  smaller than the library's defaults (the theme sets the sizes).
- A large figure - `fig(..., floating: true)` (in PDF - top or bottom of the
  page), a small one for a paragraph - `side-by-side`.
- What the library has, do not draw again: axes, plots, 3D, arrays, matrices,
  graphs, trees, charts, the vault graph. Beyond that - own CeTZ figures in
  `canvas(theme => ...)` in theme colors (lines, polygons, labels, `p3`
  points, own helpers). A new **kind** of figure or layout useful for other
  notes is needed - draw it in the note for now and tell the user.

## Code

- Code follows the idea and does not replace it; text between listings keeps
  them from being a wall.
- `listing` reads best up to about 25 lines; the key lines go with
  `highlight:` and `callouts:` (in the text "строка #callout(1) - ..."), the
  complexity with `complexity:`. A sample input and output make the listing
  checkable.
- Working code lives next to the book (`Folder/Book/code/...`) and is run
  (random tests against a naive solution for algorithms); a chapter takes a
  region: `#code-from-file("/Folder/Book/code/x.cpp", region: "build")` - the
  path is from the vault root.
- A query (SQL) is shown with its result, the first 3-5 rows as a table.

## Checks

After every chapter or section:

```sh
notes check --vault "Name" "Folder/Book"              # errors, warnings ("0,5" in a formula, ;), broken links
notes png --vault "Name" "Folder/Book" -o $TMP/pages  # and LOOK at the pages; --theme night for the dark theme
```

- `notes check` is clean: no errors, warnings or broken links.
- Every number is recomputed independently (Python, sympy, brute force,
  simulation); a figure caption is checked against the figure's own
  coordinates; code is run.
- By eye, in the light and the dark theme and on a narrow screen (about
  400 px; the page in a browser): overlapping labels, a flipped y axis, lines
  invisible in the dark, orphaned headings, "0, 5" instead of "0,5", code past
  the right edge, a table wider than a phone screen, interactive parts that
  work.
- The file header says what is done and what is next.
