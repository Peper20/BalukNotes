// Arrays and matrices (DP tables): highlights, brackets, pointers, arrows.

#import "@preview/cetz:0.4.2"
#import "../theme.typ": current-theme, is-dark
#import "canvas.typ": _draw, _color

// ═════════════════════════════════════════════════════════════════════════
// Arrays and value tables (axis down: row 0 is the top one)
// ═════════════════════════════════════════════════════════════════════════

/// A one-dimensional array.
/// - highlight: a dictionary "index -> color" ("accent", "second", ... or color);
/// - spans: ((l, r, color, label), ...) - a bracket under cells l..r inclusive;
/// - pointers: ((i, label), ...) - an arrow from above to cell i;
/// - index-from: the number of the first cell (0 or 1);
/// - block-size: the block size - block borders are drawn bolder (sqrt decomposition);
/// - block-values: per-block values - boxes above the array the full block width
///   (block sums, maximums, "lazy" marks);
/// - arcs: ((i, j, label, color), ...) - an arc above the array between cells
///   (a swap, a reference, a pair of two pointers).
#let array-cells(
  values, cell: 0.62, highlight: (:), spans: (), pointers: (),
  indices: true, index-from: 0, block-size: none, block-values: (), arcs: (),
) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  import cetz.draw: *
  let k = cell
  for (i, v) in values.enumerate() {
    let col = highlight.at(str(i + index-from), default: none)
    let fill-color = if col == none { theme.color.bg } else {
      let c = _color(theme, col)
      if is-dark(theme) { c.transparentize(65%) } else { c.lighten(72%) }
    }
    rect((i * k, 0), ((i + 1) * k, k), fill: fill-color, stroke: 0.6pt + fc.line.transparentize(10%))
    content(((i + 0.5) * k, k / 2), [#v])
    if indices {
      content(((i + 0.5) * k, -0.02), anchor: "north", text(size: 0.72em, fill: theme.color.muted, str(i + index-from)))
    }
  }
  for (l, r, col, label) in spans {
    let c = _color(theme, col)
    let y = if indices { -0.42 } else { -0.12 }
    let (a, b) = ((l - index-from) * k + 0.05, (r - index-from + 1) * k - 0.05)
    line((a, y + 0.1), (a, y), (b, y), (b, y + 0.1), stroke: 0.9pt + c)
    content(((a + b) / 2, y - 0.04), anchor: "north", text(fill: c, size: 0.85em, label))
  }
  for (i, label) in pointers {
    let x = (i - index-from + 0.5) * k
    line((x, k + 0.45), (x, k + 0.06), stroke: 0.9pt + fc.second, mark: (end: "stealth", fill: fc.second, stroke: 0pt, scale: 0.45))
    content((x, k + 0.45), anchor: "south", text(fill: fc.second, size: 0.85em, label))
  }
  // Block borders over the cells.
  if block-size != none {
    let n = values.len()
    let nblocks = calc.ceil(n / block-size)
    for j in range(nblocks + 1) {
      let x = calc.min(j * block-size, n) * k
      line((x, 0), (x, k), stroke: 1.4pt + fc.line)
    }
  }
  // Block values: a box above each block.
  if block-values.len() > 0 {
    let n = values.len()
    let block-w = if block-size != none { block-size } else { calc.ceil(n / block-values.len()) }
    for (j, v) in block-values.enumerate() {
      let a = j * block-w * k
      let b = calc.min((j + 1) * block-w, n) * k
      if b <= a { continue }
      rect((a, k + 0.18), (b, k + 0.18 + k * 0.85),
        fill: if is-dark(theme) { fc.line.transparentize(78%) } else { fc.line.lighten(85%) },
        stroke: 0.6pt + fc.line)
      content(((a + b) / 2, k + 0.18 + k * 0.425), text(size: 0.85em, [#v]))
    }
  }
  for bow in arcs {
    let (i, j) = (bow.at(0), bow.at(1))
    let label = bow.at(2, default: none)
    let c = _color(theme, bow.at(3, default: "second"))
    let (xi, xj) = ((i - index-from + 0.5) * k, (j - index-from + 0.5) * k)
    let peak = k + 0.12 + calc.abs(xj - xi) * 0.32
    bezier((xi, k + 0.04), (xj, k + 0.04), ((xi + xj) / 2, peak),
      stroke: 0.9pt + c, mark: (end: "stealth", fill: c, stroke: 0pt, scale: 0.45))
    if label != none {
      content(((xi + xj) / 2, peak * 0.88), anchor: "south", text(fill: c, size: 0.8em, label))
    }
  }
})

/// A two-dimensional table of values (a matrix). Rows go top to bottom.
/// - rects: ((r1, c1, r2, c2, color), ...) - inclusive, numbered from
///   `index-from`; drawn as a translucent fill with a border;
/// - cells: a dictionary "r,c" -> color - a single-cell highlight;
/// - indices: row and column number labels;
/// - values: false - only cells, no numbers (for small diagrams);
/// - arrows: ((r1, c1, r2, c2, color), ...) - a transition from cell to cell
///   (exactly what DP tables need).
#let matrix-cells(
  mat, cell: 0.55, rects: (), cells: (:), indices: true,
  index-from: 1, compact: false, values: true, arrows: (),
) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  import cetz.draw: *
  let k = cell
  let n = mat.len()
  let w = mat.at(0).len()
  let cell-pos(r, c) = (c * k, -r * k) // top left corner of cell (r, c), from 0
  for r in range(n) {
    for c in range(w) {
      let key = str(r + index-from) + "," + str(c + index-from)
      let col = cells.at(key, default: none)
      let (x, y) = cell-pos(r, c)
      rect((x, y), (x + k, y - k), stroke: 0.45pt + fc.grid,
        fill: if col == none { theme.color.bg } else { let cc = _color(theme, col); if is-dark(theme) { cc.transparentize(60%) } else { cc.lighten(70%) } })
    }
  }
  for (r1, c1, r2, c2, col) in rects {
    let c = _color(theme, col)
    let (xa, ya) = cell-pos(r1 - index-from, c1 - index-from)
    let (xb, yb) = cell-pos(r2 - index-from + 1, c2 - index-from + 1)
    rect((xa, ya), (xb, yb), fill: c.transparentize(if is-dark(theme) { 70% } else { 78% }), stroke: 1.1pt + c)
  }
  if values {
    for r in range(n) {
      for c in range(w) {
        let (x, y) = cell-pos(r, c)
        content((x + k / 2, y - k / 2), text(size: if compact { 0.78em } else { 0.9em }, [#mat.at(r).at(c)]))
      }
    }
  }
  for ar in arrows {
    let (r1, c1, r2, c2) = (ar.at(0), ar.at(1), ar.at(2), ar.at(3))
    let c = _color(theme, ar.at(4, default: "second"))
    let (x1, y1) = cell-pos(r1 - index-from, c1 - index-from)
    let (x2, y2) = cell-pos(r2 - index-from, c2 - index-from)
    let (a, b) = ((x1 + k / 2, y1 - k / 2), (x2 + k / 2, y2 - k / 2))
    let (dx, dy) = (b.at(0) - a.at(0), b.at(1) - a.at(1))
    let d = calc.sqrt(dx * dx + dy * dy)
    let pad-len = k * 0.34
    line(
      (a.at(0) + dx / d * pad-len, a.at(1) + dy / d * pad-len),
      (b.at(0) - dx / d * pad-len, b.at(1) - dy / d * pad-len),
      stroke: 1pt + c, mark: (end: "stealth", fill: c, stroke: 0pt, scale: 0.4),
    )
  }
  if indices {
    for c in range(w) {
      let (x, y) = cell-pos(0, c)
      content((x + k / 2, 0.04), anchor: "south", text(size: 0.68em, fill: theme.color.muted, str(c + index-from)))
    }
    for r in range(n) {
      let (x, y) = cell-pos(r, 0)
      content((-0.06, y - k / 2), anchor: "east", text(size: 0.68em, fill: theme.color.muted, str(r + index-from)))
    }
  }
})
