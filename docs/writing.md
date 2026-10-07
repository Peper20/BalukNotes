# How to write notes

Rules for the **content** of BalukNotes notes: what to write and how to check
it (`notes docs writing`). The `baluk` library does the layout, its API is
`notes docs library`. A new note is started by the Claude Code skill
`/baluk-note`; everything goes through the `notes` command, from any folder:

| Command | What it does |
|---|---|
| `notes vaults`, `notes vaults new "Name"` | vaults; a new vault |
| `notes new --vault "Name" --title "Title" [--folder Folder] [--book] [--tag ...] [--lang ...]` | a note or book stub, the file name comes from the title; prints the file path and the note path |
| `notes list --vault "Name"`, `notes tags --vault "Name"` | notes (path, book, title, tags) and tags |
| `notes check --vault "Name" [Path]` | build errors, warnings, broken links |
| `notes pdf --vault "Name" Path -o file.pdf [--theme night]` | PDF of a note or book |
| `notes info [--vault "Name"]` | where the vaults are, the app address |
| `notes serve` | the app: `http://127.0.0.1:8421/v/<Vault>/n/<Path>`; in the background and at login - `notes service install` |

There is no default vault: note commands always need `--vault "Name"` (the
list - `notes vaults`); the user creates the first vault.

## Essentials

A note **explains**, it does not enumerate: the reader wants to understand the
topic from scratch and then solve problems. They read in the app - on a
computer or phone, in a light or dark theme, a chapter at a time; the PDF is
the same text for printing.

- First "why", then "what", then "how to use it".
- A definition and a theorem are required, but understanding comes from a
  figure and a worked example next to them.
- The hard parts get more detail than the easy ones; uniformly compressed text
  is a failure.
- A dense reference (tables "construct -> meaning", walls of code) is a cheat
  sheet, a separate note (`book.with(kind: [Шпаргалка], ...)`).

## Note or book

| Kind | What it is | File |
|---|---|---|
| **Note** (`note`) | one topic: a definition, a technique, a tool, a problem; `=` is a section, numbering is continuous | `Folder/Name.typ` |
| **Book** (`book`) | a course or a large topic: `=` is a chapter, numbering "chapter.n", title page and contents in PDF | `Folder/Name/main.typ` + chapters `NN-topic.typ` |

**Title and path differ.** The title is `title: [...]` in the file (for a
book - in `main.typ`), any characters; the tree, tabs, graph and links show it
(a formula there shows as source: `[Ряд $sum 1/n^2$]` -> "Ряд sum 1/n^2",
better in words). The path is where the file lies (`Сеть/SSH основы`): links
(`#see`), checks and PDF builds use it. `notes new --title` takes the file name
from the title (without `/ \ : * ? " < > |`, a taken one gets a number) and
prints the path on the second line. To rename - `notes rename --vault "Name"
Path "New title"` (or in the app: right click in the tree -> "Переименовать..."):
the title and the file (folder) name in the same folder change, and `#see`
links to it in other notes are rewritten; `--dry-run` only shows. Only the
title, without the file name - edit `title:`. A folder title is `_folder.toml`
in it: `title = "Сети и протоколы"` (no file - the folder name).

- Short notes on one idea are connected by links, as in Obsidian:
  `#see("Математика/Производная")`, a section - `anchor: "Heading"`. "Links
  here" and the graph are built from links. A link to an unwritten note is
  broken: write the note or do not link.
- Tags - `tags: ("матан", "интегралы")`: lowercase, in Russian, shared across
  the vault (existing ones - `notes tags`).
- **Book and chapter properties.** `main.typ` is the book root:
  `book.with(...)` sets what all chapters share (language, words, tags of the
  whole topic), a chapter inherits it. A chapter's own properties, as of a
  note - `#show: chapter.with(title: [...], tags: (...))` at the start of its
  file instead of `= Title`: the title and own tags (added to the book tags;
  shown under the chapter heading and on the tag page). The first chapter is
  an ordinary chapter: what the book shares lives only in the root.
- Language - `lang:` (`"ru"` by default); for languages other than `ru` and
  `en` - own `words:`.

## Process

1. Find out the **goal** (an exam, a contest, a lab, "to understand") and the
   **reader**: what they know well and what poorly (the less familiar gets
   more space). Gather the sources: lectures, problems, past exam papers -
   often they are in the folder the skill is called from. Sources and the
   original notes (`~/Documents/abstract/...`) are read only; everything of
   your own goes to the vault, drafts and screenshots to a temporary folder.
2. Agree on the **plan** before the text: for a book - chapters and sections,
   for a note - sections; for each - figures, examples, hard parts. Wait for
   "yes".
3. Write a chapter (section) at a time; after each - the check (below) and a
   note in the file **header** - `//` comments at the start of the book's
   `main.typ` or the note: goal and reader, sources, plan, what is done, what
   is next. The author writes the header (`notes new` does not create it).
   Work is often interrupted - the next session starts from the header. Done -
   delete the header.
4. Add visualizations right away, without waiting to be asked.

The user sees the note as it is written: `notes serve` picks up edits by
itself.

## A book chapter (a note section - the same, without `plan`)

```typst
#import "/_baluk/lib.typ": *

= Chapter title               // or #show: chapter.with(title: [...], tags: (...))
#lead[2-4 sentences: why the chapter and what it builds on]
#plan([what we will learn to do], [...], [...])

== Section                   // motivation -> intuition with a figure -> rigor
   #definition / #theorem    briefly
   #example                  step by step: #step[Figure] ... #step[Limits] ... #answer[...]
   #pitfall                  the main trap of the section, right after the example

#pitfalls(...)   #summary(...)   #quiz(...)      // at the end of the chapter
```

- A chapter starts on a new page in PDF and is shown separately in the app, so
  a short topic is a **section**. A guide: a chapter is 3+ PDF pages.
- Every definition and theorem gets at least one worked example.
- References inside a note: `рис. @label`, `в разделе @sec-x` (the inflected
  word by hand). To another note - only `#see`.

## Text

- Lively language in the first person plural: "let us split", "note that".
- A new term - `*bold*`, its explanation right next to it.
- No bare formulas: before it - what it says, after - how to use it. The key
  formula of a section - `#formula[...]`, one or two per section.
- A hard part: 2-3 examples from simple to complex, a step-by-step trace,
  `#proof`, a common mistake.
- Boxes (`remark`, `idea`, `pitfall`, `margin-note`) are seasoning: no more
  than one per half page.
- `$dc("0,5")$`, not `$0,5$` (it prints "0, 5"). Russian operators: `tg`,
  `ctg`, `sh`, `ch`.
- Typst eats `;` right after a call like `#see(...)` - write `\;` (`notes check`
  warns).
- Font size, fonts and colors only from the theme: a note does not set
  `set text(size: ...)` or `rgb(...)`. Shrinking to make things fit is not
  allowed.

## HTML is the main view

One source gives the page and the PDF, but mostly the page is read.

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

## Figures - required, without reminders

The norm: 2-3 figures per chapter and at least one in every section about
geometry, a data structure, an algorithm or a diagram - next to the explaining
text and referenced from it.

- **A caption is a statement, not a title.** Bad: "Region D". Good: "The spoke
  enters D through the parabola and leaves through the line - these are the
  limits of the inner integral".
- An algorithm - in **frames** (`frames`): a slider and playback in the app,
  the default frame or a storyboard `pdf: (1, 4, 8)` in PDF. One static figure
  for an algorithm is too little.
- A dependence on a parameter - `interactive-plot` (sliders) or
  `interactive-surface` (3D with rotation).
- Highlight with one theme color (`"second"`), not a rainbow; labels no
  smaller than 8pt.
- A large figure - `fig(..., floating: true)` (in PDF - top or bottom of the
  page), a small one for a paragraph - `side-by-side`.
- What the library has, do not draw again: axes, plots, 3D, arrays, matrices,
  graphs, trees, charts, the vault graph. Beyond that - own CeTZ figures in
  `canvas(theme => ...)` in theme colors (lines, polygons, labels, `p3`
  points, own helpers). A new **kind** of figure or layout useful for other
  notes is needed - draw it in the note for now and tell the user.

| Subject | Required pictures |
|---|---|
| Calculus | spokes for limits, plots, a 3D surface, solids and sections, integral sums (in frames) |
| Algorithms and data structures | an array with blocks, block values, pointers and arcs; frames of the run; a graph with the current step; a DP table with arrows |
| Databases | the data **before** a query and the **result**; which rows matched in a join; the window frame |
| Computer architecture | a block diagram, the memory hierarchy, a pipeline by cycles, splitting an address into fields |
| Probability | geometric probability, outcome trees, density and distribution function |
| Any subject | measurements and comparisons - `chart`, not a table of numbers |

## Code

- Comes **after** the idea is explained and does not replace it; two listings
  in a row without text between them are not allowed.
- `listing` up to 25 lines, key lines - `highlight:` and `callouts:`, in the
  text "строка #callout(1) - ..."; next to it the complexity (`complexity:`)
  and a sample input and output.
- Working code lives next to the book (`Folder/Book/code/...`), is compiled and
  tested (random tests against a naive solution); a chapter takes a region:
  `#code-from-file("/Folder/Book/code/x.cpp", region: "build")` - the path is
  from the vault root.
- **SQL: every query has its result** (the first 3-5 rows as a table).

## Checks (after every chapter or section)

```sh
notes check --vault "Name" "Folder/Book"              # errors, warnings ("0,5" in a formula, ;), broken links
notes pdf --vault "Name" "Folder/Book" -o $TMP/k.pdf  # and --theme night -o $TMP/k-night.pdf
pdftoppm -r 70 -png $TMP/k.pdf $TMP/page              # and LOOK at the pages (PNG)
```

- `$TMP` is a temporary folder, not the vault and not the sources folder.
- The page - `http://127.0.0.1:8421/v/<Vault>/n/Folder/Book` in a browser
  (Claude in Chrome): light and dark theme (the switch at the top right), a
  narrow window (~400 px), interactivity. The server is not running - `notes
  serve` (or ask the user; permanently - `notes service install`).
- Your own note has no errors, warnings or broken links; other problems of the
  vault (`notes check` without a path) are not fixed silently - tell the user.
- Every number is recomputed independently (Python, sympy, brute force,
  simulation); a figure caption is checked against the figure's own
  coordinates.
- By eye: overlapping labels, a flipped y axis, lines invisible in the dark,
  orphaned headings, "0, 5" instead of "0,5", code past the right edge, a
  table wider than a phone screen.

## Chapter checklist

- [ ] a lead and a plan; every section has motivation, a figure, a step-by-step example
- [ ] hard parts are covered in more detail than easy ones
- [ ] 2-3 figures or more, each with a statement caption and a reference from the text
- [ ] algorithms in frames, SQL with results
- [ ] listings up to 25 lines, the code is compiled and tested
- [ ] numbers are recomputed independently
- [ ] `pitfalls`, `summary`, `quiz` at the end
- [ ] `notes check` is clean; the PDF and the page (light, dark, narrow screen) are looked at
- [ ] the file header reflects what is done and what is next
