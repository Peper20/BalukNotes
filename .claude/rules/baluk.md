---
paths:
  - "baluk/**"
---

# baluk - editing the library

The API for note authors - `baluk/README.md` (= `notes docs library`).

- Change the PDF look only deliberately, comparing with the notes
  (`Конспекты/` of the user's vault); a refactoring keeps PDF and HTML byte for
  byte.
- In English: the public interface (names of functions, parameters, theme keys
  and string values), error messages, comments. Captions are in the note's
  language (`i18n.typ`), theme titles are Russian.
- Public names are only the re-exports of `lib.typ`, notes import only it. The
  list is the snapshot `tests/snapshots/baluk-api.txt`; the test
  `crates/notes-core/tests/it/library.rs` fails on a missing name and on a new
  one not described in `baluk/README.md`, and also checks
  `skills/baluk-note/reference.md` against the sources. Names with `_` are
  internal. The exception is the fixture `tests/vault/Рисунки/Интерактив.typ`
  (`_formula`, `_eval`).
- A large module is a subdirectory of parts (`figures/`, `plots/`) and a
  top-level file that gathers them.
- `/_baluk/` of a debug build is the repository directory (edits show at
  once), of a release build - a copy in the binary (rebuild).
- In HTML the whole document is `<article class="k-doc" data-doc>`.
- Every block has two branches; HTML gets only markup with `k-...` classes
  (the `elem` helper from `web.typ`), without colors and sizes - CSS sets them
  (`app/src/baluk-css/`):

  ```typst
  #let lead(body) = context {
    if is-web() { return elem("div", "k-lead", body) }   // HTML
    ...                                                  // PDF
  }
  ```

  There is no shared helper `web-or(html, pdf)`: the branches compute
  different things, and wrapping them in functions is longer than
  `if is-web() { return ... }`.
- The HTML export drops `grid`, `stack`, `align`, `place` (with their
  content), `h`, `v` - do not use them in the HTML branch. For note code the
  template (`_web-template`) sets fallbacks: an SVG frame `k-layout`, a class
  for `align`, an empty `span.k-h`/`div.k-v` for absolute spacing. Inside
  `html.frame` layout is paged again - the fallbacks check `is-web()`.
- Figures inherit `set text` of the template's HTML branch (font, size,
  color): without it SVG labels are black and invisible in the dark theme.
  Check figures in `night`.
- `return` in a block drops the `show`/`set` before it - wrap them in
  `{ ... }`.
- A named argument with a hyphen: `elem("div", "k-x", ..("data-x": v), body)`.
- Theme colors and fonts live only in `theme.typ`; `css.typ` exports them for
  HTML. A new theme is a dictionary in `themes` (with `name` and a Russian
  `title`): CSS variables and browser fonts appear by themselves.
- Figures only through `canvas`: it wraps the SVG in `div.k-frame`, by which
  the core merges themes. A `frames` frame is a plain `canvas`.
- Layout words only via `word(...)` from `i18n.typ`; a new word goes into all
  dictionaries.
- Parsing interactive figure formulas - `plots/formula.typ` and
  `app/src/lib/plot/formula.ts`, line by line the same: edit one - edit the
  other and extend the cross-check in `tests/vault/Рисунки/Интерактив.typ`
  (update the snapshot - Vitest compares the client with Typst).
- The `k-` prefix in classes and variables and the name `baluk` stay (user's
  decision); rename only all at once (core, CSS, client, snapshots).

```
lib.typ        entry point - public names
theme.typ      themes, customize(), current-theme(), css-colors() for HTML
template.typ   note and book: HTML and PDF branches
blocks.typ     boxes, lead/plan/summary/quiz, margin notes, tables
code.typ       listings, callouts, code from a file, reference code colors for HTML
figures.typ    figures: figures/canvas (canvas, fig, in-row, color by name), plane (2D),
               space (3D), cells (arrays, matrices), graphs (graph and layouts)
plots.typ      interactive: plots/formula (formula parsing), common, plot, surface
frames.typ     frames
graph.typ      vault-graph (data from the app)
charts.typ     data charts
links.typ      see
i18n.typ       layout words: word()
web.typ        HTML mode: is-web(), elem(), frame()
css.typ        theme export for CSS (the app reads it)
math.typ       operators, dc, dd, defeq
```
