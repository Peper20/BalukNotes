// 2D: axes through the origin, functions, parametric curves, fills, spokes,
// points, contour lines. The y axis points up.

#import "@preview/cetz:0.4.2"
#import "../theme.typ": current-theme
#import "canvas.typ": _draw, _color

// How many canvas units in 1pt: tick, point and label offset sizes are in pt
// so they do not grow with the canvas scale.
#let _pt(ctx) = 1pt / ctx.length

/// Coordinate axes with arrows and labels.
#let axes(x: (-0.5, 4), y: (-0.5, 3), labels: ($x$, $y$), origin: $O$) = _draw.get-ctx(ctx => {
  let fc = current-theme().color.fig
  let u = _pt(ctx)
  import cetz.draw: *
  let axis-stroke = 0.6pt + fc.axis
  let arrow-mark = (end: "stealth", fill: fc.axis, stroke: 0pt, scale: 0.55)
  line((x.at(0), 0), (x.at(1), 0), stroke: axis-stroke, mark: arrow-mark)
  line((0, y.at(0)), (0, y.at(1)), stroke: axis-stroke, mark: arrow-mark)
  content((x.at(1), -2 * u), anchor: "north-east", labels.at(0))
  content((-3 * u, y.at(1)), anchor: "north-east", labels.at(1))
  if origin != none { content((-1 * u, -1 * u), anchor: "north-east", text(size: 0.85em, origin)) }
})

/// A tick on the x axis with a label and (optionally) a dashed line up to height `to`.
#let tick(x, label, up-to: none) = _draw.get-ctx(ctx => {
  let fc = current-theme().color.fig
  let u = _pt(ctx)
  import cetz.draw: *
  if up-to != none {
    line((x, 0), (x, up-to), stroke: (paint: fc.axis.transparentize(40%), thickness: 0.45pt, dash: "dashed"))
  }
  line((x, -2.2 * u), (x, 2.2 * u), stroke: 0.6pt + fc.axis)
  content((x, -3 * u), anchor: "north", label)
})

/// The plot of y = f(x) on [a, b]. color: "line" | "second" | "third" | color.
/// The label goes at the right end of the curve or, if `label-x` is given, at
/// the point (label-x, f(label-x)) - this separates labels of curves meeting
/// at one point.
#let plot(f, a, b, n: 80, color: auto, thickness: 1.1pt, dashed: false, label: none, label-anchor: "south-west", label-x: none) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  let pts = range(n + 1).map(i => { let x = a + (b - a) * i / n; (x, f(x)) })
  line(..pts, stroke: (paint: col, thickness: thickness, dash: if dashed { "dashed" } else { none }, join: "round"))
  if label != none {
    let p = if label-x == none { pts.last() } else { (label-x, f(label-x)) }
    content(p, anchor: label-anchor, padding: 3pt, text(fill: col, label))
  }
})

/// A parametric curve t ↦ (x(t), y(t)).
#let parametric(fx, fy, t0, t1, n: 90, color: auto, thickness: 1.1pt, closed: false, fill-color: none) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  import cetz.draw: *
  let pts = range(n + 1).map(i => { let t = t0 + (t1 - t0) * i / n; (fx(t), fy(t)) })
  line(..pts, close: closed, fill: if fill-color == auto { theme.color.fig.fill } else { fill-color },
    stroke: (paint: _color(theme, color), thickness: thickness, join: "round"))
})

/// A fill of the region a ≤ x ≤ b, lo(x) ≤ y ≤ hi(x).
#let fill-between(lo, hi, a, b, n: 60, color: auto) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  import cetz.draw: *
  let top = range(n + 1).map(i => { let x = a + (b - a) * i / n; (x, hi(x)) })
  let bot = range(n + 1).map(i => { let x = b - (b - a) * i / n; (x, lo(x)) })
  line(..top, ..bot, close: true, stroke: none,
    fill: if color == auto { theme.color.fig.fill } else { _color(theme, color) })
})

/// A "spoke" - a double-headed arrow: vertical (x, y0 -> y1) or, with
/// horizontal: true, horizontal (y, x0 -> x1).
#let spoke(c, from, to, horizontal: false, color: "second") = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  let (p, q) = if horizontal { ((from, c), (to, c)) } else { ((c, from), (c, to)) }
  line(p, q, stroke: 1.1pt + col, mark: (start: "stealth", end: "stealth", fill: col, stroke: 0pt, scale: 0.5))
})

/// A point with a label.
#let point(p, label: none, label-anchor: "south-west", color: "line", radius: 1.9pt) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  circle(p, radius: radius / ctx.length, fill: col, stroke: 0.5pt + theme.color.bg)
  if label != none { content(p, anchor: label-anchor, label, padding: 3pt) }
})

/// Contour lines f(x, y) = c by marching squares: the very picture from which
/// a student grasps the shape of a surface looking from above.
#let contours(f, levels, xr: (-2, 2), yr: (-2, 2), n: 40, color: auto) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  let (x0, x1) = xr
  let (y0, y1) = yr
  let dx = (x1 - x0) / n
  let dy = (y1 - y0) / n
  for c in levels {
    for i in range(n) {
      for j in range(n) {
        let (xa, ya) = (x0 + dx * i, y0 + dy * j)
        let ang = ((xa, ya), (xa + dx, ya), (xa + dx, ya + dy), (xa, ya + dy))
        let corner-vals = ang.map(p => f(p.at(0), p.at(1)) - c)
        let points = ()
        for k in range(4) {
          let (a, b) = (corner-vals.at(k), corner-vals.at(calc.rem(k + 1, 4)))
          if (a < 0) != (b < 0) {
            let t = a / (a - b)
            let (pa, pb) = (ang.at(k), ang.at(calc.rem(k + 1, 4)))
            points.push((pa.at(0) + (pb.at(0) - pa.at(0)) * t, pa.at(1) + (pb.at(1) - pa.at(1)) * t))
          }
        }
        if points.len() == 2 { line(points.at(0), points.at(1), stroke: 0.7pt + col) }
      }
    }
  }
})
