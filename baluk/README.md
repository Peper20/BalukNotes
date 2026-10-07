# baluk - the BalukNotes layout library

A Typst library for BalukNotes notes: one source gives both the page in the
app (HTML) and the PDF. Function and parameter names are English, note text is
in any language. What to write and how to check it - `notes docs writing`.
Tested on Typst 0.15.1; the external dependency is `@preview/cetz:0.4.2`
(cache `~/.cache/typst/packages`, works offline).
License - AGPL-3.0, as BalukNotes; notes using the library and the HTML and
PDF built from them - on any terms of the notes' author (`LICENSE-EXCEPTION`).

## Note and book

The vault sees the library as `/_baluk/` (virtual, no copy in the vault). The
theme comes as the input `theme` (`--input theme=night`), a document does not
set it.

**A note** is one file; `=` is a section, numbering is continuous
("Определение 3", "Рис. 2"), section numbers are hidden by default:

```typst
#import "/_baluk/lib.typ": *
#show: note.with(title: [SSH], tags: ("network", "security"))

#lead[...]
= How it works
```

**A book** is a folder with `main.typ`: `=` is a chapter ("Глава N"),
numbering "chapter.n". `main.typ` is the book **root**: `book.with(...)` -
shared properties all chapters inherit (language, theme, words, book tags),
and `#include` of the chapters. Chapters are files `NN-topic.typ`, they start
with the same `#import`.

```typst
#import "/_baluk/lib.typ": *
#show: book.with(
  kind: auto,                    // label above the title: auto - "Конспект"; [Задачник], [Шпаргалка]...
  title: [Multiple integrals],
  subtitle: [Double, triple and $n$-dimensional integrals],
  author: [Calculus, semester 2],
  date: [2026],
  description: [Who the notes are for and how to read them - 2-3 sentences for the title page.],
  title-page: true, toc: true, depth: 2,   // PDF only
)
#include "01-review.typ"
```

**A chapter** is `= Title` or, if it has own properties like a note,
`chapter.with`: the title, own tags (added to the root tags), a label. The
first chapter is an ordinary chapter, it holds nothing shared. The template
sets the chapter heading, under it in HTML - own tags (tags are not printed in
PDF); outside a book - an error.

```typst
#import "/_baluk/lib.typ": *
#show: chapter.with(title: [Double integral], tags: ("integrals",), label: "ch-double")
```

**Language** - `lang:` of `note` and `book` (`"ru"` by default): it sets the
layout words ("Определение"/"Definition", "Рис."/"Fig.", "Глава"/"Chapter",
the frame caption), hyphenation and quotes; in HTML - `lang` on `<article>`.
Dictionaries - `i18n.typ`, `ru` and `en`. A language without a dictionary gets
English words and a `notes check` warning. Own words - `words:` over the
dictionary (keys as in `i18n.typ`). Library error messages are in English.

```typst
#show: note.with(lang: "en", title: [Limits])     // #definition -> "Definition 1."
#show: note.with(lang: "de", words: (definition: "Definition", figure: "Abb."), title: [...])
```

**Links between notes** - a path from the vault root without `.typ`; the
anchor is a heading text or a label. `notes check` checks the note and the
section:

```typst
#see("Network/UFW")                                  // the note title
#see("Network/SSH", anchor: "Changing the port")     // "Changing the port"
#see("Network/SSH", anchor: "Changing the port")[port]  // own text
```

## Themes

`classic` (blue accent, boxes with a rule, book paragraphs) and `night` - the
same layout with a dark palette: made from `classic` with one
`customize(classic, (color: ...))`. `title` is the theme name in the
interface. An own theme is a deep merge, give only what changes:

```typst
#let mine = customize(themes.classic, (
  name: "mine", title: "Моя",
  color: (accent: rgb("#7a1f5c"), fig: (line: rgb("#7a1f5c"))),
  size: (text: 11.5pt),
))
#show: book.with(theme: mine, ...)   // or a new entry in themes - it appears in the app
```

The current theme - `current-theme()` (only inside `context`); in
`canvas(theme => ...)` it comes as the argument. A pale fill visible in both
themes - `pale(theme, color)`.

## Blocks (`blocks.typ`)

| Call | What it is |
|---|---|
| `#definition(title: "...")[...]` | a definition; numbered "chapter.n", together with theorems |
| `#theorem(title: "...")[...]` | a theorem, a statement |
| `#proof[...]` | a proof sketch, smaller |
| `#example(title: "...")[...]` | a worked example; own numbering |
| `#step[Title]` | in an example: "Шаг 1. Title."; numbering is separate in each example |
| `#answer[...]` | an answer in a frame on the right |
| `#remark[...]`, `#idea[...]`, `#algorithm[...]`, `#pitfall[...]` | unnumbered boxes |
| `#formula(label: "...")[$ ... $]` | a key formula in a frame; `label` is a caption above it, not a label for `@` |
| `#lead[...]` | 2-4 introductory sentences |
| `#plan([...], [...])` | "В этой главе": what we will learn to do |
| `#summary([...], [...])` | "Коротко о главном" at the end of a chapter |
| `#quiz(([question], [answer]), ...)` | "Проверь себя", answers in small type below |
| `#pitfalls(([mistake], [how to avoid]), ...)` | a table of common mistakes |
| `#data-table(cols, header: (...), highlight: ("4,*": "third"), ...cells)` | a table highlighting a cell ("2,2"), a row ("4,\*"), a column ("\*,2") - traces |
| `#margin-note[...]` | a small note on the side |
| `#side-by-side[text][figure]` | text and a small figure side by side |
| `#small-caps[...]` | small caps for Cyrillic (`smallcaps` does not work with the fonts) |

Definitions and theorems are not split between pages, examples are.

## Code (`code.typ`)

```typst
#listing(
  caption: [building P],
  highlight: (5, 6),              // highlight lines
  callouts: ("5": 1, "12": 2),    // numbered circles at lines
  complexity: [$O(n m)$],
  ```cpp
  ...
  ```,
)
Line #callout(1) - the building formula.   // a reference to a circle from the text

#code-from-file("/code/prefix.cpp", region: "build", highlight: (3,))
```

Highlighting uses theme colors, ligatures are off; a listing up to 15 lines is
not split. `code-from-file` takes the part between `// region: name` and
`// endregion: name`; the path is from the vault root, with `/`; no region - a
clear build error.

## Figures (`figures.typ`, CeTZ 0.4.2)

```typst
#fig(
  canvas(unit: 2.5cm, theme => {       // theme - the current theme
    import cetz.draw: *
    fill-between(x => x * x, x => x, 0, 1)   // fill between curves
    axes(x: (-0.1, 1.3), y: (-0.1, 1.2))
    plot(x => x * x, 0, 1.08, label: $y = x^2$, label-x: 0.9, label-anchor: "north-west")
    spoke(0.6, 0.36, 0.6)              // a double-headed arrow
    tick(1, $1$)
    line((0, 0), (1, 1), stroke: 1pt + theme.color.fig.second)   // own lines in theme colors
  }),
  [A caption - a statement about what is visible],
  label: "fig-spoke",                  // in the text: рис. @fig-spoke
  floating: false,                     // true - to the top or bottom of a PDF page
)
```

| Helper | Purpose |
|---|---|
| `axes`, `tick`, `plot`, `parametric`, `fill-between`, `spoke`, `point` | 2D: axes through the origin, functions, parametric curves, fills |
| `contours(f, levels, xr:, yr:)` | contour lines |
| `p3`, `axes3d`, `surface(f, xr, yr, style: "shaded"/"flat"/"wire")`, `prisms`, `base-shape`, `revolution(r)`, `cross-section(f, y0)` | pseudo-3D, the painter's algorithm |
| `array-cells(values, highlight:, spans:, pointers:, block-size:, block-values:, arcs:, index-from:)` | an array: blocks and block values (sqrt decomposition), arcs, pointers |
| `matrix-cells(matrix, rects:, cells:, arrows:, values:, index-from:)` | a matrix and a DP table: highlights, transition arrows |
| `graph(vertices, edges, directed:, highlight:, marks:, shape:)` | a graph; nodes `"circle"` or `"rect"` (two lines: key and priority); opposite edges are separated automatically |
| `tree-layout(edges, root)`, `binary-layout(children, root)` + `binary-edges(children)`, `circle-layout(names)` | vertex coordinates: by levels, by key order (search trees), on a circle |
| `in-row(..., separators: ($=$, $-$))` | canvases in a row: algorithm frames, a formula of pictures |

A `graph` edge is `(u, v, style, label, (side: "left", at: 0.5))`: the label is
left or right of the direction u -> v, at a fraction of the length from u; a
vertex covers it - change `side` or `at`. Styles: `"normal"`, `"bold"`,
`"second"`, `"dim"`, `"dashed"`.

Colors by names `"line"`, `"second"`, `"third"`, `"accent"` (a color of the
current theme) or the `color` type. The y axis points up everywhere except
`matrix-cells` and `array-cells` (rows top to bottom). 3D: x right, z up, y
"away from us". Inside `import cetz.draw: *` the names `line`, `rect`,
`content`, `fill`, `stroke`, `anchor`, `mark` are taken - do not name your
variables so.

## Interactive figures (`plots.typ`)

A plot with sliders and a rotatable surface. The formula is a string of Typst
code; in the app the figure is live, in PDF and without JS - the frame at the
default values with the caption "a = 1, b = 2".

```typst
#fig(interactive-plot(
  ("a * calc.sin(b * x)", (f: "calc.sin(x)", label: "sin x", dashed: true, color: "second")),
  -5, 5,                                   // x range
  params: (a: (from: 0, to: 3, step: 0.1, value: 1), b: (from: 0.5, to: 4, value: 1)),
  y: (-3, 3),                              // better to give it: otherwise from the default frame
  labels: ("x", "y"), width: 8, height: 5, // the plot area, cm
), [Caption])

#fig(interactive-surface("calc.sin(k * x) * calc.cos(y)", (-3, 3), (-3, 3),
  params: (k: (from: 0.2, to: 2, value: 1)), z: (-1, 1),
  n: 24, style: "shaded",                  // or "wire"; color: "face"/"line"/"second"/"third"
  rotation: -30, tilt: 28, size: 3,        // the initial view (degrees), box size (cm)
), [Caption])
```

- A formula: numbers (`2.5`, `1e-3`), `x` (and `y` for a surface), parameters,
  `+ - * /`, parentheses, unary minus, `calc.sin cos tan asin acos atan exp ln
  log sqrt pow abs floor ceil`, `calc.pi`, `calc.e`. A power is
  `calc.pow(x, 2)`; `asin`, `atan` give a number (radians); `x-1` is x minus 1.
- Outside the domain and past the plot area (asymptotes of `tan`) the curve
  has a gap, not an error. An unclear formula is a build error with an
  explanation.

## Frames (`frames.typ`)

A figure with a parameter: Typst builds it for each value, the app gets a
slider, buttons and playback (algorithm steps, convergence). In PDF and
without JS - the default frame with a caption of the value.

```typst
#fig(frames(n => canvas(theme => {
  import cetz.draw: *
  rect((-0.1, -0.1), (3.1, 1.2), stroke: none)   // a frame of constant size
  ...                                            // the figure for this n
}), n: (from: 1, to: 12, value: 4)), [A Riemann sum: $n$ rectangles])

#fig(frames(k => canvas(...), k: (values: (0, 1, 2, 3)),
  label: k => [pass #k],     // own caption; none - no caption
  fps: 2, loop: false,       // speed, whether to loop
  pdf: (1, 2, 3, 4),         // in PDF - these frames in a row (numbers from 1); auto - the default frame
), [Algorithm steps])
```

- Exactly one parameter, named: `(from:, to:, step:, value:)` (`step` defaults
  to 1), `(values: (...), value:)` or an array. Values can be anything,
  including dictionaries of algorithm state.
- The default caption is "n = 4" for numbers, "кадр 2 из 5" for anything else.
- No more than 60 frames (each is its own SVG, ~4-8 KB).
- `pdf:` is a row without wrapping: pick fewer wide frames.
- Frames lie on top of each other, the space is that of the largest. To keep
  the figure from jumping, draw an invisible frame of constant size: CeTZ fits
  the canvas to what is drawn.

## Vault graph (`graph.typ`)

```typst
#vault-graph()                                        // the whole vault
#vault-graph(around: "Network/SSH", depth: 2)         // neighbors of a note
#vault-graph(folders: ("Math",), missing: false, width: 10cm)
#vault-graph(tag: "linalg", orphans: false)
```

- `around`, `depth` - the neighbors of a note within `depth` steps; it is in
  the center even if another filter would hide it;
- `folders` / `hidden` - only these / except these top-level folders
  (`"в корне"` - notes in the root);
- `tag` - notes with the tag; `missing` - unwritten notes; `orphans` - notes
  without links;
- `width` - the largest width (a large graph shrinks).

The app computes the graph (as on the graph page); in PDF - a CeTZ picture, in
the app - live. A note with a graph is rebuilt on an edit of any note; outside
the app (`typst compile`) it does not build.

## Data charts (`charts.typ`)

For measurements: running time, sizes, shares. Ranges, ticks, the grid and the
legend are computed automatically.

```typst
#fig(canvas(chart(
  (kind: "line", points: true, label: [brute force], data: ((1, 10), (2, 40), (3, 90))),
  (kind: "function", f: x => x * calc.log(x), from: 1, to: 5, label: [$n log n$]),
  width: 5.2, height: 3.2, labels: ([$n$], [ms]), legend: "inside",
)), [What the chart shows])
```

Series (`kind`): `"line"` (`points: true`, `dashed: true`), `"points"`,
`"steps"`, `"bars"` (data - `((label, value), ...)`), `"function"` (`f`,
`from`, `to`). Other options: `x`, `y` (ranges), `ticks-x`, `ticks-y`,
`format-x`, `format-y`, `gridlines`, `legend` (`"right"`, `"inside"`, `none`).

## Math (`math.typ`)

- `tg ctg arctg arcctg sh ch th cth rot grad const` - operators in the Russian
  tradition.
- `dc("0,5")` - a decimal with a comma; `$0,5$` prints "0, 5".
- `dd(x)` - a differential; `defeq` - "equal by definition" (`:=`).
