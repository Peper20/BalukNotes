# baluk library reference

**Full** signatures: positional arguments, then named ones with defaults; there are no other parameters. Start from the sample (`examples/...` in the section title); this file lists what else you can change.

## Common

- **Color** (`color:`, `highlight:`, styles): a theme name, `"line"` (main), `"second"` (highlight, the main thing in a figure), `"third"`, `"accent"`; rarely `"fill"`, `"face"`, `"axis"`, `"grid"`. `auto` is the default. No own `rgb(...)`: it breaks the dark theme.
- **Theme in own code**: `canvas(theme => { ... })`; outside a canvas `current-theme()` inside `context` (`#context fig(...)`). Colors: `theme.color.fig.line` (also `fill face second third axis grid`), `theme.color.muted` (gray), `.bg`, `.text`, `.accent`; `col.transparentize(60%)` is lighter.
  ```
  current-theme()      // only inside context
  pale(theme, col)     // pale fill visible in both themes
  ```
- **Label anchor** (`label-anchor:`, `anchor:`): the side of the label touching the point. `"south-west"` puts it up-right, `"north"` below, `"west"` right, `"east"` left; also `"south"`, `"north-east"`, `"center"`, ...
- **Bare numbers** in a canvas are canvas units (`unit:`); `(a, b)` is a range or a point. A function: `x => x * x`, of two: `(x, y) => x * y`.

## Templates: `examples/blocks.typ`, `book/main.typ`, `book/01-idea.typ`

```
note(theme: auto, lang: "ru", words: (:), title: none, description: none, tags: (), body)
book(theme: auto, lang: "ru", words: (:), kind: auto, title: none, subtitle: none, author: none, date: none, description: none, tags: (), title-page: true, toc: true, depth: 2, body)
chapter(title: none, tags: (), label: none, body)
```

- Only as `#show: note.with(...)` / `#show: book.with(...)`; never set `theme` (the app picks it).
- `description`: line in the note list; `tags`: array of strings `("tag",)`.
- `book` in `main.typ` is the book root: every chapter inherits its `lang`, `words` and `tags`. `chapter` gives one chapter own traits, like a note: `#show: chapter.with(title: [...], tags: (...))` at the top of a chapter file instead of `= Title`; `title` is required, `tags` add to the book tags (shown under the chapter heading, found on the tag page), `label: "ch-x"` is a string for `@ch-x` and `#see(..., anchor: "ch-x")`. Only inside a book; a chapter without own tags may stay `= Title`.
- `kind`: label above the book title (`auto` is `Конспект`); `title-page`, `toc`, `depth`: title page, contents and its depth, PDF only.
- `lang`: language of the block words, `"ru"` or `"en"` (others get English words). `words: (key: "word")` sets own words; keys: `definition theorem example remark pitfall idea algorithm step answer proof plan-note plan-chapter summary quiz answers mistake avoid complexity figure chapter contents book-kind frame frame-of`.

## Text blocks: `examples/blocks.typ`

```
lead(body)                        // 2-4 sentences before the first section
plan(..items)                     // "in this note": plan([...], [...])
definition(title: none, body)     // numbered together with theorem
theorem(title: none, body)
proof(body)
example(title: none, body)        // own numbering
step(name)                        // inside example: "Step 1. name."
answer(body)                      // inside example: boxed answer
remark(title: none, body)         // unnumbered callouts
idea(title: none, body)
algorithm(title: none, body)
pitfall(title: none, body)
formula(label: none, body)        // key formula in a box: formula[$ ... $]
margin-note(body)                 // small, on the side
side-by-side(main, side, width: 40%, gap: 1.2em)   // width is the share of side
data-table(cols, ..cells, header: none, highlight: (:), cell-align: auto)
pitfalls(..pairs)                 // (([mistake], [how to avoid]), ...)
summary(..items)                  // ([...], [...])
quiz(..pairs)                     // (([question], [answer]), ...)
small-caps(s)                     // small caps: small-caps[gost]
```

- Block `title:` is a string: `title: "squeeze"`.
- `formula(label: "first")`: caps text above the formula, **not** a label for `@`; formulas cannot be referenced.
- `data-table`: `cols` are widths `(auto, 1fr)`, then cells in a row; `highlight: ("2,3": "second")`, rows and columns from 1, row 0 is the header; `"2,*"` a row, `"*,3"` a column. `cell-align: center` etc.
- Plain Typst `table(...)` is fine too.

## Links

```
see(id, anchor: none, ..body-args)
```

- `see("Folder/Note")` shows the note title; `see("...", anchor: "Heading text")` links a section; `see("...")[own text]`. Paths from `notes list`.
- Own section or figure: label `<sec-x>` after a heading or `label: "fig-x"` of `fig`; in text `@sec-x`.

## Code: `examples/book/02-code.typ`

```
listing(src, lang: "cpp", caption: none, highlight: (), callouts: (:), line-numbers: true, complexity: none)
code-from-file(file-path, region: none, ..rest)
callout(n)
```

- `src`: a raw block ` ```python ... ``` ` (language from it) or a string with `lang:`.
- `highlight: (4, 5)` shades lines; `callouts: ("4": 1)` puts marker 1 at line 4; `#callout(1)` is the same marker in text.
- `code-from-file("/path/from/vault/root.cpp", region: "name")`: the lines between `// region: name` and `// endregion: name`; other named arguments as in `listing` except `lang` (from the extension).

## Math: `examples/blocks.typ`

```
dc(s)       // decimal comma: $dc("0,5")$, never $0,5$
dd(x)       // differential: $integral f(x) dd(x)$
```

`defeq` is ":="; Russian operators `tg ctg arctg arcctg sh ch th cth rot grad const`. All inside `$...$`.

## Figure and canvas: `examples/figures.typ`

```
fig(body, caption, label: none, floating: false)
canvas(body, unit: 1cm)
in-row(..elements, separators: none, gap: 0.8em)
```

- Every figure is `fig(canvas(...), [Caption])`; `interactive-*` and `frames` go into `fig` too. `floating: true` lets a big figure float to the top or bottom of a PDF page.
- `canvas` body: `{ ... }` or `theme => { ... }`.
- `in-row(canvas(...), canvas(...), separators: ($+$,))`: canvases in a row with signs between.
- **Own lines** (CeTZ) after `import cetz.draw: *` in the canvas body:
  ```
  line(a, b, c, close: true, fill: ..., stroke: 0.8pt + theme.color.fig.line)
  line(a, b, stroke: (paint: theme.color.fig.second, thickness: 0.6pt, dash: "dashed"))  // or "dotted"
  line(a, b, mark: (end: "stealth", fill: theme.color.fig.line))                         // arrow
  rect((x0, y0), (x1, y1), fill: pale(theme, theme.color.fig.line), stroke: none)
  circle((x, y), radius: 0.5, stroke: 0.6pt + theme.color.fig.line)
  content((x, y), [text], anchor: "west", padding: 3pt)
  ```
  Then `line rect circle content fill stroke anchor mark` are taken: do not use them as variable names.

## 2D: `examples/figures.typ`

```
axes(x: (-0.5, 4), y: (-0.5, 3), labels: ($x$, $y$), origin: $O$)
tick(x, label, up-to: none)
plot(f, a, b, n: 80, color: auto, thickness: 1.1pt, dashed: false, label: none, label-anchor: "south-west", label-x: none)
parametric(fx, fy, t0, t1, n: 90, color: auto, thickness: 1.1pt, closed: false, fill-color: none)
fill-between(lo, hi, a, b, n: 60, color: auto)
spoke(c, from, to, horizontal: false, color: "second")
point(p, label: none, label-anchor: "south-west", color: "line", radius: 1.9pt)
contours(f, levels, xr: (-2, 2), yr: (-2, 2), n: 40, color: auto)
```

- `axes`: `origin: none` hides "O"; draw axes **after** fills.
- `tick(1, $1$, up-to: 0.5)`: tick on the x axis and a dashed line up to 0.5.
- `plot`: label at the right end or at `label-x`.
- `parametric`: `fill-color: auto` fills a closed curve.
- `fill-between(low, high, a, b)`: between two curves on `[a, b]`.
- `spoke(x, y0, y1)`: vertical double arrow; `horizontal: true` makes it `spoke(y, x0, x1)`.
- `contours(f, (1, 2, 3))`: level lines `f(x, y) = c`.

## 3D: `examples/figures.typ`

x goes right, z up, y away from the viewer. A point is `p3(x, y, z)` (`line` accepts it).

```
p3(x, y, z)
axes3d(x: 3, y: 2.6, z: 2.4, labels: ($x$, $y$, $z$), hidden: (:))
surface(f, xr, yr, n: 14, m: auto, style: "shaded", color: auto, edge: auto)
cross-section(f, y0, xr: (0, 3), n: 40, color: "second", fill-color: true)
base-shape(..points, color: auto, label: none)
prisms(f, xr, yr, n: 5, m: 4, gap: 0.0, highlight: (), color: auto, highlight-color: "second")
revolution(r, zmin: 0, zmax: 2, rings: 8, n: 40, color: auto)
```

- `axes3d`: draw **after** the body; `hidden: (z: 1.2)` dashes the first 1.2 units of an axis hidden by the body.
- `surface(f, (x0, x1), (y0, y1))`: `style` is `"shaded"` (light and shade), `"flat"` or `"wire"` (transparent mesh); `n`, `m` are cells along x and y; `edge` is the face outline (`none` for none).
- `cross-section(f, 1.2)`: section by the plane y = 1.2.
- `base-shape((0, 0), (3, 0), (3, 2), label: $D$)`: region on the floor z = 0.
- `prisms`: `n x m` bars; `highlight: ((1, 0),)` marks cells (i along x, j along y, from 0).
- `revolution(z => radius)`: solid of revolution around the z axis.

## Arrays, tables, graphs: `examples/algorithms.typ`

```
array-cells(values, cell: 0.62, highlight: (:), spans: (), pointers: (), indices: true, index-from: 0, block-size: none, block-values: (), arcs: ())
matrix-cells(mat, cell: 0.55, rects: (), cells: (:), indices: true, index-from: 1, compact: false, values: true, arrows: ())
graph(vertices, edges, directed: false, radius: 0.27, highlight: (), dimmed: (), labels: (:), marks: (:), shape: "circle", size: (0.72, 0.46))
tree-layout(edges, root, dx: 0.9, dy: 0.95)
binary-layout(children, root, dx: 0.85, dy: 0.95)
binary-edges(children, style: "normal")
circle-layout(names, radius: 1.3, start-angle: 90deg)
```

- Cell numbers start at `index-from`; dictionary keys are **strings**.
- `array-cells`: `highlight: ("3": "second")`; `spans: ((from, to, color, [label]),)` bracket below; `pointers: ((i, [label]),)` arrow above; `arcs: ((i, j, [label], color),)` arc (order differs from `spans`!); `block-size: 4, block-values: (9, 22)` blocks with boxes above; `cell` is the cell size; `indices: false` hides numbers.
- `matrix-cells` (rows go down): `cells: ("2,3": "second")`; `rects: ((r1, c1, r2, c2, color),)` a rectangle of cells; `arrows: ((r1, c1, r2, c2),)`, color optional fifth; `values: false` hides numbers; `compact: true` makes them smaller.
- `graph`: `vertices` is `("a": (x, y), ...)` or a layout below; an edge is `("a", "b")`, `("a", "b", style)`, `("a", "b", style, [label])` or `("a", "b", style, [label], (side: "right", at: 0.3))`. Styles: `"normal" "bold" "second" "dim" "dashed"`. `highlight`, `dimmed`: arrays of names; `labels: ("a": [text])`; `marks: ("a": ([$d = 0$], "west"))` a mark with its anchor; `shape: "rect"` with `size` for two-line nodes.
- `tree-layout(edges, root)` by levels; `binary-layout(("4": ("2", "6"), "2": ("1", none)), "4")` with `binary-edges(same dict)` for search trees; `circle-layout(("a", "b", "c"))` on a circle. All return `vertices`.

## Charts from data: `examples/figures.typ`

```
chart(..series-list, x: auto, y: auto, width: 6, height: 3.6, labels: (none, none), ticks-x: auto, ticks-y: auto, format-x: _num, format-y: _num, gridlines: true, legend: "right", zero: true)
```

- Inside `canvas(chart(...))`. A series is a dictionary by `kind`:
  - `"line"`: `data: ((x, y), ...)`, `points: true`, `dashed: true`;
  - `"points"`, `"steps"`: `data: ((x, y), ...)`;
  - `"bars"`: `data: (([label], value), ...)` or `(value, ...)`;
  - `"function"`: `f: x => ..., from: 1, to: 5`.

  All take `label: [legend text]`, `color: "second"`.
- `x`, `y`: ranges (`auto` from data); `labels: ([$n$], [ms])` axis labels; `legend: "right" | "inside" | none`; `width`, `height` in canvas units; `ticks-x: (1, 2, 4)` own ticks; `format-x: v => [#v s]` tick text; `gridlines: false` no grid; `zero: false` y axis not from zero.

## Interactive figures: `examples/interactive.typ`

```
interactive-plot(formula, a, b, params: (:), y: auto, labels: ("x", "y"), width: 8, height: 5, n: 160)
interactive-surface(formula, xr, yr, params: (:), z: auto, labels: ("x", "y", "z"), n: 24, style: "shaded", color: "face", rotation: -30, tilt: 28, size: 3)
```

- **The formula is a string**: numbers (`2.5`, `1e-3`), `x` (and `y` for a surface), parameters, `+ - * /`, parentheses, `calc.sin cos tan asin acos atan exp ln log sqrt abs floor ceil`, `calc.pow(x, 2)`, `calc.pi`, `calc.e`. No `^`, `**`; `log` is base 10.
- A plot `formula` is a string, a dictionary `(f: "...", label: "sin x", dashed: true, color: "second")` or an array of them.
- `params: (a: (from: -3, to: 3, step: 0.1, value: 1))` is a slider; the name is not `x`, `y`, `calc`; `step` defaults to 1/100 of the range, `value` to `from`. The PDF shows the frame at `value`.
- Always set `y:` / `z:` as `(from, to)`; `width`, `height`, `size` are plain numbers in cm.
- Surface: `style: "shaded" | "wire"`, `color: "face" | "line" | "second" | "third"`, `rotation`, `tilt`: initial view in degrees.

## Frames: `examples/algorithms.typ`, `interactive.typ`

```
frames(body, label: auto, fps: 2, loop: false, pdf: auto, ..param)
```

- `body`: a function `value => canvas(...)`. Exactly **one** named parameter, any name: `n: (from: 1, to: 12, step: 1, value: 4)`, `k: (values: (0, 1, 2), value: 0)` or `k: (0, 1, 2)`. Values can be anything (numbers, strings, dictionaries). At most 60 frames.
- `label: v => [pass #v]` own caption, `none` for none.
- `fps`: playback speed; `loop: true` repeats.
- `pdf: (1, 3, 5)`: these frames (numbered from 1) in a row in the PDF; `auto` is one frame at `value`.
- Draw an invisible frame `rect(..., stroke: none)` of the same size in every frame, or the figure jumps.

## Vault graph: `examples/book/02-code.typ`

```
vault-graph(around: none, depth: 1, folders: (), hidden: (), tag: none, missing: true, orphans: true, width: 15cm)
```

- `around: "Folder/Note", depth: 2`: neighbors of a note; `folders: ("Math",)` / `hidden: (...)`: only / except these top-level folders (`"в корне"` means notes in the root); `tag: "..."`; `missing: false` hides unwritten notes; `orphans: false` hides unlinked ones; `width`: maximum width.

## Not for notes

`themes`, `customize`: editing library themes; `cetz`: the drawing package (in a note only `import cetz.draw: *` inside a canvas).
