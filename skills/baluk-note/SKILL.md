---
name: baluk-note
description: BalukNotes notes in Typst - create a note or a book, add a chapter, edit or rename a note (or name a folder) by the vault rules. Runs from any folder, often the one with the sources (lectures, problems).
disable-model-invocation: true
argument-hint: "[тема, книга ..., допиши ... или переименуй ...]"
allowed-tools: Bash(notes info:*), Bash(notes vaults:*), Bash(notes docs:*), Bash(notes list:*), Bash(notes tags:*), Bash(notes new:*), Bash(notes check:*), Bash(notes rename:*), Bash(notes pdf:*), Bash(pdftoppm:*), Bash(pgrep -x notes), Read, Glob, Grep
---

# BalukNotes note

**Language:** reply to the user in the language of their request. Write a new note in that language too (unless the user asks otherwise); when editing, keep the note's language. A Russian request means a Russian note, even though these instructions and samples are in English.

**User request:**

<request>
$ARGUMENTS
</request>

This is your task: work through it with the process below. If the request is empty, ask in one question which note to create or edit.

## Context

BalukNotes keeps notes as Typst files in a **vault** (a folder the user owns). The app (`notes serve`) renders them as web pages, `notes pdf` prints them, and the `baluk` library provides the layout: templates, text blocks, figures, interactive plots. The user reads notes mostly in the app, on a desktop or a phone, in a light or dark theme; the PDF is the same text for print. So the web page is the main result, and everything has to work in both themes.

Everything you need is in the skill folder ("Base directory" at the top, usually `~/.claude/skills/baluk-note`):
- this file: the process, paths, Typst basics, `notes check` errors, rules for good notes;
- `examples/`: whole notes and a book that together use every library function; each builds without errors;
- `reference.md`: full signatures of all library functions with allowed values.

You do not need the BalukNotes sources or web docs: the library is not in any public documentation, and the samples plus the reference are the accurate description of it. Work through the `notes` command and by editing note files. `notes` prints in Russian; the messages that matter are quoted below.

## Pick the task

- A. New note: "note", "create a note about ...", one topic. The default when unsure.
- B. New book: "book", "course notes", a large topic with chapters.
- C. Chapter in an existing book: "add a chapter to ...".
- D. Edit a note: "continue / fix note ...".
- E. Rename a note, book or folder, or name a folder.

Work only in the chosen vault; never touch other vaults. In it, create or change only what the task is about: the note you create, or the notes, books and folders the user named (C, D, E). If you see a reason to change anything else (a link in another note, a broken link, a typo), ask the user first and wait for the answer; the vault is the user's own writing.

## Process

1. **Vault.** `notes vaults` lists vault names, one per line. If the command is missing, tell the user to install BalukNotes (`tools/install.sh` in its repo) and stop. If there are no vaults, tell the user to create one (in the app or `notes vaults new "Name"`) and stop: choosing and naming vaults is the user's decision. Use the vault the user named; otherwise ask, even if there is only one, since a note in the wrong vault is easy to lose. If the named vault is not in the list, show the list and ask again. Every `notes` command except `vaults` needs `--vault "Name"` right after the subcommand (`notes list --vault "Name"`); there is no default vault. The vault folder on disk is the first line of `notes info --vault "Name"`.
2. **Survey.** `notes list --vault "Name"` prints notes (path, `[книга]` for books, title, tags; under a book, `глава «...»` lines for chapters with own tags); `notes tags --vault "Name"` prints tags. Use them to pick the folder, tags and link targets.
3. **Sources.** `ls` the current folder and the folders the user named; read what relates to the topic (lectures, problems, old exams). Sources are read-only: create nothing in their folders.
4. **Decide title, folder, tags** (see "Title and path"). Decide the obvious yourself. If the place, or note vs book, is genuinely unclear, ask one question with your default first.
5. **Create** (A and B):
   ```sh
   notes new --vault "Name" --title "Title" --folder "Folder" --tag tag1 --tag tag2   # A. note
   notes new --vault "Name" --book --title "Title" --folder "Folder" --tag tag        # B. book
   ```
   Add `--lang en` (ISO 639 code) when the note is not in Russian: it sets `lang:` in the template, so block words become "Definition", "Fig." instead of Russian ones. The file name is derived from the title. Output: line 1 is the file on disk (read and edit it), line 2 is the note path ("Path" below: for `notes check`, `notes pdf`, `#see`), line 3 is the app URL. Exit code 1 (`недопустимый путь`, `пустое название`): report the message to the user and leave existing files alone. Exit code 2: wrong arguments, see `notes new --help`.
6. `notes check --vault "Name" "Path"`: the fresh stub is clean (`ошибок: 0, предупреждений: 0`). Add the progress header at the top of the new file (a book: `main.typ`), see "The file".
7. **Where to look.** If `pgrep -x notes` prints something, the app runs and the page at the URL from step 5 refreshes on every save; otherwise suggest `notes serve` to the user.
8. **Plan.** First read `reference.md` and every file in `examples/` in full, once per session (skip this for task E and small fixes): the plan should use the figures and blocks the library actually has, and the samples show how a finished note looks. Then find out the goal (exam, contest, lab, "understand it") and the reader: what they know well and what poorly, since weak spots deserve more room. Propose sections (a book: chapters, then sections) with the figures, examples and hard spots of each, and wait for approval. If the user already said what the note must contain ("one parabola plot with a slider"), that is the plan.
9. **Write** one section (chapter) at a time, see "Editing the file" and "Writing a good note". Start each call from its sample in `examples/`; `reference.md` lists every parameter and allowed value. The samples show the calls, not the limits: combine features and draw your own figures (see "Figures").
10. **Check** after each section: `notes check --vault "Name" "Path"`, then fix and repeat until `ошибок: 0, предупреждений: 0, битых ссылок: 0`. The "Errors" list below explains the usual messages; for an unclear one compare your call with the sample and the signature in `reference.md`.
11. **Header.** Update the progress header (see "The file") after each section: work is often interrupted, and the next session starts from it. When everything is done, delete the header.
12. **Look at the result.** Numbers and `notes check` do not show overlapping labels, invisible lines in the dark theme or a table wider than a phone. If you can read images:
    ```sh
    notes pdf --vault "Name" "Path" -o $TMP/note.pdf
    pdftoppm -r 70 -png $TMP/note.pdf $TMP/page                        # open the PNG pages
    notes pdf --vault "Name" "Path" --theme night -o $TMP/note-night.pdf  # dark theme
    ```
    `$TMP` is the session temp folder (or `/tmp`), never the vault or a sources folder. If you have a browser tool and the app runs, also open the app URL: light and dark theme (switch at the top right), a narrow window (about 400 px), and the interactive parts (sliders, frames, rotation).
13. **Report**: file path, app URL, what is done, what is left.

**C. Chapter.** Instead of steps 5-6 create the chapter file by hand (`notes new` does not create chapters): the book folder on disk (vault folder + book path) + `NN-topic.typ`, NN following the existing chapters. It starts with `#import "/_baluk/lib.typ": *`, then `#show: chapter.with(title: [Chapter title], tags: (...))` when the chapter has tags of its own, otherwise `= Chapter title` (sample: `examples/book/01-idea.typ`). Add `#include "NN-topic.typ"` to the book's `main.typ` after the last `#include`. Check with `notes check --vault "Name" "Book path"`.

**D. Edit.** Instead of steps 5-6: the path comes from `notes list`, the file is vault folder + path + `.typ` (a book is a folder with `main.typ` and chapters). Read the whole file and change what was asked.

**E. Rename.** Steps 1, 2, the check and the report. Use `notes rename`, not a manual move: it writes the new title into the file (a folder: `_folder.toml`), renames the file or folder after the title (in the same parent folder) and rewrites `#see("path")` links to it in other notes, so nothing breaks.
```sh
notes rename --vault "Name" "Path" "New title" --dry-run   # prints the new path and the notes whose links will change; changes nothing
notes rename --vault "Name" "Path" "New title"             # does it
```
The first output line is the new path; the next one names the notes with rewritten links (`ссылки поправлены: ...`) or says there are none (`ссылок сюда в других заметках нет`). Those link edits are part of the rename the user asked for: list them in the report. To change only the title and keep the file name, edit `title: [...]` by hand (a book: `main.typ`; a folder: `_folder.toml`, sample `examples/_folder.toml`).

Check with `notes check --vault "Name"` without a path, since folder errors appear only in the full check.

## Title and path

**Title**: `title: [...]` in the file (a book: `main.typ`), any text, shown to readers in the tree, tabs, graph and links (a formula there shows as its source, `sum 1/n^2`, so prefer words). **Path**: where the file is, from the vault root, `/`-separated, without `.typ`: `Math/Parabola`. `notes new` (line 2) and `notes list` (first on each line) print it; it is the same in `notes check`, `notes pdf`, `#see(...)` and the app URL. Folders are created automatically.

**Folder title**: `_folder.toml` in the folder, one line `title = "Networks and protocols"`; other keys are errors. Without it the folder shows its name. Write it only when the user asks to name a folder.

Common path mistakes:
- `"Math/Parabola"`, not `"/home/.../vaults/Notes/Math/Parabola"`: a vault path, not a disk path.
- `"Parabola"` for a note in the root, not `"Notes/Parabola"`: the vault name is not part of the path.
- No `.typ` at the end.
- Names starting with `_` or `.` are reserved for the app.

Spaces and non-Latin letters are fine; quote the path in commands. Folder (`--folder`): an existing one from `notes list`, or a new short one in the note language. Tags: lowercase, in the note language, reusing those from `notes tags`, because tags are shared across the vault.

## The file

A note file has three parts, in order:
- **progress header**: `//` lines at the top, written and kept up to date by you, not by `notes new`. It holds the state of the work: goal and reader, sources, plan (and whether the user approved it), what is done, what is next. The next session starts from it, so update it after each section and delete it when the note is finished. One header per file (a book: in `main.typ`). A file that already has one keeps its keys, even if they are in another language; add a header to a file without one when the work spans more than one session.
- `#import "/_baluk/lib.typ": *` and `#show: note.with(title: [...], tags: (...))`: the template that styles the whole note. Keep them; you may change the fields `title`, `description`, `tags` and add others from `reference.md` (`lang`, `words`). The text goes after the closing `)` of `#show`.
- the text: sections, blocks, figures.

A book's `main.typ` has `book` instead of `note` and `#include "01-topic.typ"` lines after `#show`; the text lives in chapter files. `main.typ` is the book root: its `lang`, `words` and `tags` apply to every chapter, so tags of the whole topic go there and a chapter's own tags go in its `chapter.with` (the first chapter is an ordinary chapter).

A header in progress (keys as here, values in the note language):

```typst
// Work on the note (update after each section; delete when done):
//   goal and reader: school student before the exam, weak at graphs
//   sources: ~/study/algebra/lecture-3.pdf
//   plan: 1) vertex and branches 2) shifts (slider plot) 3) problems; approved
//   done: sections 1-2
//   next: section 3, problems from the lecture
```

### Editing the file

- Read the whole file first. When rewriting it (Write), keep the header, `#import` and `#show` unchanged and put the text after them; line numbers and tool markers from the read output ("End of file...") must not end up in the file.
- When appending or replacing a piece (Edit), copy the old piece verbatim from the file so that it is unique.
- Run `notes check` after every write: errors are cheap to fix one at a time.

## Typst essentials

Typst is neither LaTeX nor Markdown: `\frac`, `\begin`, `**bold**`, `# Heading` do not work.

- Section / subsection: `= Section`, `== Subsection` (in a book `=` is a chapter).
- Bold, italic, code: `*term*`, `_italic_`, `` `a[i]` ``.
- Lists: `- item`, numbered `+ item`.
- Inline / display math: `$x^2$` / `$ x^2 $` (spaces inside the `$` make a display line).
- Label and reference: `= Section <sec-x>` ... `section @sec-x`; a figure: `label: "fig-x"` ... `fig. @fig-x`. In Russian notes the case word is written by hand (`в разделе @sec-x`).
- Comment: `// to end of line`.
- Literal `# $ * _ @ < \`: `\#`, `\$`, `\*`, `\_`, `\@`, `\<`, `\\`.

**Math:** `x^2`, `x_1`, `x_(i+1)`, `a / b` (fraction), `sqrt(x)`, `sum_(i=1)^n`, `integral_a^b f(x) dd(x)`, `lim_(x -> 0)`, `oo`, `pi`, `alpha`, `epsilon`, `<=`, `>=`, `!=`, `->`, `dots`, `abs(x)`, text `"text"`. Adjacent letters form one name: `ax` is an unknown variable, write `a x` (`$y = a x^2 + b x + c$`). A decimal comma needs `dc("0,5")`: a plain `0,5` in math prints as "0, 5". Russian notes use Russian operators (`tg`, `ctg`, `sh`, `ch`, see `reference.md`).

**Function call**: `#`, name, `(arguments)`, optional `[text]`: `#definition(title: "limit")[...]`.
- `#` switches from text to code, so it appears only in text, before a call: `#fig(canvas(...), [Caption])`, not `#fig(#canvas(...))`.
- Arguments are separated by commas; named ones are `name: value`: `#fig(canvas(...), [Caption], label: "fig-1")`.
- `[...]` content, `"..."` string, `(...)` array or dictionary, `{...}` code block. A one-element array needs a trailing comma: `("tag",)`. Dictionary `(a: 1, b: 2)`. Array item `arr.at(0)`, not `arr[0]`.
- Units: `2.5cm`, `11pt`, `40%`, `90deg`. Some parameters take plain numbers (`width: 8` of `interactive-plot` is centimeters); `reference.md` says which.
- Own variables and functions: `#let a = (1, 2, 3)`, `#let f(x) = x * x`; in code `for`, `if`, `range(4).map(i => i * i)`.
- A `;` right after a call (`#see(...);`) ends the expression and disappears from the text: write `\;`.

## Errors of `notes check`

- `expected comma`, `expected identifier` at `)`: a dot or nothing between arguments, e.g. `fig(...). [Caption]` instead of `fig(...), [Caption]`.
- `unclosed delimiter`: an unclosed `(` `[` `{` or `$`.
- `unknown variable: ax` in math: letters merged into one name; write `a x`, text as `"text"`.
- `unknown variable: name` outside math: a typo in a function name; names are in `reference.md`.
- `unexpected argument: name`: no such parameter; see the signature in `reference.md`.
- `missing argument: caption`: `fig` needs a caption, `fig(figure, [Caption])`.
- `expected expression`: an extra or missing `#`, or an empty argument `,,`.
- `invalid number suffix`: an unknown unit; valid are `pt cm mm em % deg fr`.
- `непонятный знак` in a formula of `interactive-plot`: the formula language has no `**` or `^`; write `x * x` or `calc.pow(x, 2)`.
- `неизвестное имя` in such a formula: functions need `calc.`, e.g. `calc.sin(x)`.
- `file not found ... /_baluk/...`: the import must be exactly `#import "/_baluk/lib.typ": *`.
- warning `запятая между цифрами`: `dc("0,5")` instead of `0,5` in math.
- warning about `;` after a call: `\;`.
- warning `нет слов оформления`: the note language is neither ru nor en, so block words fall back to English; set own words with `words:` (keys in `reference.md`).
- `битая ссылка` (broken link): the `#see("...")` path is not in `notes list`, or the anchor does not match a heading text. Either write the target note or drop the link.
- `_folder.toml: unknown field`: only `title = "Title"` is allowed.
- Other Russian messages come from the library and say what to change.

## Writing a good note

A note **explains** rather than lists: the reader wants to understand the topic from scratch and then solve problems. A dense cheat sheet (tables of "construct: meaning", walls of code) is a separate note (`book.with(kind: [Cheat sheet], ...)`), not a replacement for an explanation.

**Note or book.** A note is one topic (a definition, a technique, a tool, a problem): `=` is a section, numbering runs through the note. A book is a course or a large topic: `=` is a chapter ("Chapter N") that starts a new PDF page and is shown separately in the app, with a title page and contents in the PDF. A short topic is therefore a section, not a chapter; aim for a chapter of at least 3 PDF pages. Short notes on one idea, linked with `#see`, work well: the app builds backlinks and a graph from the links.

**Structure of a chapter (a section of a note is the same without `plan`):**
- `#lead[...]`: 2-4 sentences, why this matters and what it builds on; `#plan(...)` in a book chapter.
- Each `== Section`: motivation, intuition with a figure, then rigor (`#definition`, `#theorem`, `#proof`), a worked `#example` with `#step`s and `#answer`, and the main `#pitfall` right after the example.
- At the end: `#pitfalls(...)`, `#summary(...)` (3-6 points to remember), `#quiz(...)`.
- Every definition and theorem gets at least one worked example.

**Text.**
- Why first, then what, then how to use it. Hard parts get more room than easy ones: evenly compressed text fails exactly where the reader struggles. A hard spot gets 2-3 examples from simple to harder, a step-by-step trace, a proof sketch and the typical mistake.
- A lively "we" voice ("let us split", "note that"). A new term in `*bold*` with its explanation right next to it.
- No bare formulas: before one say what it means, after it how to use it. The key formula of a section goes in `#formula[...]`, one or two per section.
- Callouts (`remark`, `idea`, `pitfall`, `margin-note`) are seasoning: about one per half page, or they stop standing out.
- Recompute every number independently (Python, sympy, brute force, simulation), and check each figure caption against the figure's own coordinates.

**The web page is the main view.**
- Layout only through library blocks: `side-by-side` (text and a small figure), `in-row` (figures in a row), `fig`, `data-table`. The Typst HTML export drops `grid`, `stack`, `align`, `place` and fractional `h`/`v`, so such layout silently disappears in the app.
- No own HTML, JavaScript or styles, and no fixed colors (`rgb(...)`) or font sizes (`set text(size: ...)`): the theme sets colors and sizes, and the reader switches themes. Interactivity comes only from library blocks.

**Figures.** Add them without being asked: 2-3 per chapter, and at least one in every section about geometry, a graph, a data structure, an algorithm or a scheme, placed next to the text that refers to it.
- The caption is a statement about what is visible, not a name: not "Region D" but "The spoke enters D through the parabola and leaves through the line: these are the limits of the inner integral".
- An algorithm is shown as `frames` (a slider and playback in the app, a storyboard `pdf: (1, 4, 8)` in the PDF); one static picture is not enough. A dependence on a parameter is an `interactive-plot` or `interactive-surface`.
- Highlight the main thing with one theme color, `"second"`, not a rainbow; labels no smaller than the defaults.
- A large figure can float in the PDF (`fig(..., floating: true)`); a small one sits next to its paragraph with `side-by-side`.
- Use library helpers where they exist (axes, plots, 3D, arrays, matrices, graphs, trees, charts, the vault graph) instead of redrawing them. Beyond them, draw freely with CeTZ inside `canvas(theme => { ... })`: lines, polygons, arcs, labels, 3D points via `p3`, loops and own helper functions, always in theme colors (`reference.md`, "Own lines"). If the note needs a whole new kind of figure or layout that other notes would reuse, draw it for now and tell the user it belongs in the library.
- Check figures in the dark theme too: a color outside the theme is invisible there.

Figures by subject, as a starting point:
- analysis: spokes for integration limits, graphs, 3D surfaces, solids and sections, Riemann sums as frames;
- algorithms and data structures: arrays with blocks, pointers and arcs, algorithm frames, graphs with the current step, DP tables with arrows;
- databases: the data before a query and the result, matched rows of a join, the window frame;
- computer architecture: block diagrams, the memory hierarchy, a pipeline by cycles, address fields;
- probability: geometric probability, outcome trees, density and distribution functions;
- any subject: measurements and comparisons as a `chart`, not a table of numbers.

**Code.**
- Code comes after the idea and does not replace it; never two listings in a row without text between them.
- A `listing` has at most 25 lines; mark key lines with `highlight:` and `callouts:` and refer to them in the text ("line #callout(1) ..."); add `complexity:` and a sample input and output.
- Working code lives in the vault next to the book (`Folder/Book/code/...`), is compiled and tested (random tests against a naive solution), and a chapter takes a region of it with `code-from-file` (path from the vault root).
- SQL: every query is followed by its result (the first 3-5 rows as a table).

**Before calling a chapter done:**
- lead and plan; each section has motivation, a figure and a worked example;
- hard spots covered in more detail than easy ones;
- 2-3 figures, each with a statement caption and a reference from the text;
- algorithms as frames, SQL with results;
- listings of at most 25 lines, code compiled and tested;
- numbers recomputed independently;
- `pitfalls`, `summary`, `quiz` at the end;
- `notes check` clean for your note (problems in other notes: tell the user, do not fix them silently); PDF and, if possible, the app page viewed in both themes and a narrow window;
- the header says what is done and what is next.

## Common calls

```typst
#lead[2-4 sentences: why this topic matters.]

#definition(title: "limit")[A number $a$ is the *limit* of $x_n$ if ...]
#theorem(title: "squeeze")[...]
#proof[...]

#example(title: "squeeze with sine")[
  Find $lim (sin n) / n$.
  #step[Bound] $-1 / n <= (sin n) / n <= 1 / n$.
  #answer[the limit is zero]
]

#fig(
  canvas(unit: 1cm, {
    axes(x: (-3, 3), y: (-1, 5))
    plot(x => x * x, -2.2, 2.2, label: $y = x^2$, label-x: 1.5)
  }),
  [The parabola $y = x^2$ is symmetric about the $y$ axis],
  label: "fig-parabola",
)

#fig(
  interactive-plot(
    "a * x * x + b * x + c",          // formula is a STRING; x^2 is x * x
    -5, 5,                            // x from, to
    params: (
      a: (from: -3, to: 3, step: 0.1, value: 1),
      b: (from: -5, to: 5, step: 0.5, value: 0),
      c: (from: -5, to: 5, step: 0.5, value: 0),
    ),
    y: (-10, 10),
  ),
  [$a$ changes the width and direction of the branches, $c$ shifts the parabola],
)

On derivatives see #see("Math/Derivative")\;
```

## Samples: `examples/`

Each file is a whole note that builds without errors, with comments on the calls. All functions and their parameters: `reference.md`.

- `blocks.typ`: template `note` with `lang`, `words`; all text blocks (definitions, worked examples, callouts, tables, summary); links `@label` and `see`; math (`dc`, `dd`, Russian operators).
- `figures.typ`: `fig`, `canvas`; 2D: axes, plots, fills, spokes, points, contours, own lines in theme colors; figures in a row; `chart`; 3D: surfaces, sections, prisms, solids of revolution.
- `algorithms.typ`: `array-cells`, DP table `matrix-cells`, graphs and trees `graph`; algorithm as `frames`; trace `data-table`.
- `interactive.typ`: `interactive-plot` (sliders, several curves), `interactive-surface`, `frames` with a number.
- `book/` (`main.typ`, `01-idea.typ`, `02-code.typ`, `code/prefix.cpp`): book template `book`, chapters (`chapter` with own tags, plain `=`), `#include`; `listing`, `code-from-file`; vault graph `vault-graph`.
- `_folder.toml`: folder title (task E).

`#see("examples/...")` in the samples points to other samples; in your note use paths from `notes list`.

---

**User request again:**

<request>
$ARGUMENTS
</request>

Reply in the language of this request and write the note in it too. Start with step 1.
