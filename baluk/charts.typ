// Data charts: axes with ticks, a grid, a legend and series.
//
// Unlike `axes` + `plot` from figures.typ: there the axes cross at the origin
// and draw a math picture in canvas units. Here - a plot area framed on the
// left and bottom, data in its own units (seconds, megabytes, counts), and
// everything scales by itself.
//
//   #fig(canvas(chart(
//     (kind: "bars", data: (([n²], 120), ([n log n], 14), ([n], 3))),
//   )), [Running time])
//
// cetz-plot is not used here: the cached version does not run on Typst 0.15.

#import "@preview/cetz:0.4.2"
#import "theme.typ": current-theme, is-dark
#import "figures.typ": _color

#let _draw = cetz.draw

// ── Numbers and ticks ─────────────────────────────────────────────────────
#let _num(v) = {
  let r = calc.round(v, digits: 3)
  let s = str(r)
  let s = if s.ends-with(".0") { s.slice(0, -2) } else { s }
  s.replace(".", ",") // decimal comma: axis labels are plain text
}

// A "round" tick step: 1, 2, 5 × 10^k - the nearest to span / count.
#let _tick-step(span, count) = {
  if span <= 0 { return 1 }
  let rough = span / calc.max(count, 1)
  let p = calc.pow(10.0, calc.floor(calc.log(rough, base: 10)))
  let m = rough / p
  let mult = if m <= 1 { 1 } else if m <= 2 { 2 } else if m <= 5 { 5 } else { 10 }
  mult * p
}

#let _ticks(vmin, vmax, count) = {
  let step-v = _tick-step(vmax - vmin, count)
  let first-tick = calc.ceil(vmin / step-v) * step-v
  let result = ()
  let v = first-tick
  while v <= vmax + step-v * 0.001 {
    result.push(v)
    v += step-v
  }
  result
}

// ── Ranges from series ────────────────────────────────────────────────────
#let _series-points(series) = {
  let kind = series.at("kind", default: "line")
  if kind == "function" {
    let (from, to) = (series.at("from", default: 0), series.at("to", default: 1))
    range(41).map(i => { let x = from + (to - from) * i / 40; (x, (series.f)(x)) })
  } else if kind == "bars" {
    series.data.enumerate().map(((i, d)) => (i, if type(d) == array { d.at(1) } else { d }))
  } else {
    series.data
  }
}

// ── Chart ─────────────────────────────────────────────────────────────────
/// Series are positional dictionaries:
///   (kind: "line",  data: ((x, y), ...), label:, color:, points: true, dashed: false)
///   (kind: "points",    data: ((x, y), ...), label:, color:)
///   (kind: "steps",  data: ((x, y), ...), label:, color:)
///   (kind: "bars",  data: ((label, val), ...) or (val, ...), color:)
///   (kind: "function",  f: x => ..., from:, to:, label:, color:)
/// The x and y ranges are computed from the data by default (auto).
/// legend: "right" | "inside" | none.
#let chart(
  ..series-list,
  x: auto,
  y: auto,
  width: 6,
  height: 3.6,
  labels: (none, none),
  ticks-x: auto,
  ticks-y: auto,
  format-x: _num,
  format-y: _num,
  gridlines: true,
  legend: "right",
  zero: true,
) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  let u = 1pt / ctx.length
  import cetz.draw: *

  let all-series = series-list.pos()
  let bar-series = all-series.filter(srs => srs.at("kind", default: "line") == "bars")
  let all = ()
  for srs in all-series { all += _series-points(srs) }
  let xs = all.map(p => p.at(0))
  let ys = all.map(p => p.at(1))

  let (x0, x1) = if x != auto { x } else if bar-series.len() > 0 {
    (-0.6, calc.max(..xs) + 0.6)
  } else {
    (calc.min(..xs), calc.max(..xs))
  }
  let (y0, y1) = if y != auto { y } else {
    let lo = calc.min(..ys)
    let hi = calc.max(..ys)
    if zero { lo = calc.min(lo, 0) }
    if hi == lo { hi = lo + 1 }
    (lo, hi + (hi - lo) * 0.08)
  }
  let to-canvas(px, py) = (
    (px - x0) / (x1 - x0) * width,
    (py - y0) / (y1 - y0) * height,
  )

  // ── grid and ticks
  let tx = if ticks-x != auto { ticks-x } else if bar-series.len() > 0 {
    bar-series.first().data.enumerate().map(((i, _)) => i)
  } else { _ticks(x0, x1, 5) }
  let ty = if ticks-y != auto { ticks-y } else { _ticks(y0, y1, 4) }

  if gridlines {
    for v in ty {
      line(to-canvas(x0, v), to-canvas(x1, v), stroke: 0.4pt + fc.grid.transparentize(15%))
    }
  }
  line(to-canvas(x0, y0), to-canvas(x1, y0), stroke: 0.7pt + fc.axis)
  line(to-canvas(x0, y0), to-canvas(x0, y1), stroke: 0.7pt + fc.axis)

  for v in tx {
    let (px, py) = to-canvas(v, y0)
    line((px, py), (px, py - 3 * u), stroke: 0.6pt + fc.axis)
    let label = if bar-series.len() > 0 {
      let d = bar-series.first().data.at(v, default: none)
      if type(d) == array { d.at(0) } else { [#_num(v)] }
    } else { [#(format-x)(v)] }
    content((px, py - 4 * u), anchor: "north", text(size: 0.82em, fill: theme.color.muted, label))
  }
  for v in ty {
    let (px, py) = to-canvas(x0, v)
    line((px, py), (px - 3 * u, py), stroke: 0.6pt + fc.axis)
    content((px - 4 * u, py), anchor: "east", text(size: 0.82em, fill: theme.color.muted, [#(format-y)(v)]))
  }
  if labels.at(0) != none {
    content((width / 2, -0.62), anchor: "north", text(size: 0.85em, labels.at(0)))
  }
  if labels.at(1) != none {
    content((-0.72, height / 2), anchor: "south", angle: 90deg, text(size: 0.85em, labels.at(1)))
  }

  // ── series
  let colors = ("line", "second", "third", "accent")
  let legend-items = ()
  for (i, series) in all-series.enumerate() {
    let kind = series.at("kind", default: "line")
    let col = _color(theme, series.at("color", default: colors.at(calc.rem(i, 4))))
    let label = series.at("label", default: none)
    if label != none { legend-items.push((kind, col, label)) }

    if kind == "bars" {
      let n = series.data.len()
      let bar-w = series.at("bar-width", default: 0.62) * width / n
      for (j, d) in series.data.enumerate() {
        let v = if type(d) == array { d.at(1) } else { d }
        let (px, _) = to-canvas(j, y0)
        let (_, py) = to-canvas(0, v)
        let (_, base-y) = to-canvas(0, calc.max(y0, 0))
        rect((px - bar-w / 2, base-y), (px + bar-w / 2, py), fill: col.transparentize(if is-dark(theme) { 55% } else { 70% }), stroke: 0.8pt + col)
        if series.at("values", default: true) {
          content((px, py), anchor: "south", padding: 2pt, text(size: 0.78em, fill: col, [#_num(v)]))
        }
      }
    } else if kind == "points" {
      for p in series.data {
        circle(to-canvas(p.at(0), p.at(1)), radius: 2.2pt / ctx.length, fill: col, stroke: 0.4pt + theme.color.bg)
      }
    } else if kind == "steps" {
      let step-pts = ()
      for (j, p) in series.data.enumerate() {
        if j > 0 { step-pts.push(to-canvas(p.at(0), series.data.at(j - 1).at(1))) }
        step-pts.push(to-canvas(p.at(0), p.at(1)))
      }
      line(..step-pts, stroke: (paint: col, thickness: 1.1pt, join: "round"))
    } else {
      let points = _series-points(series).map(p => to-canvas(p.at(0), p.at(1)))
      line(..points, stroke: (
        paint: col, thickness: 1.2pt, join: "round",
        dash: if series.at("dashed", default: false) { "dashed" } else { none },
      ))
      if series.at("points", default: false) {
        for p in series.data { circle(to-canvas(p.at(0), p.at(1)), radius: 2pt / ctx.length, fill: col, stroke: none) }
      }
    }
  }

  // ── legend
  if legend != none and legend-items.len() > 0 {
    let (lx, ly) = if legend == "inside" { (0.25, height - 0.25) } else { (width + 0.35, height) }
    for (j, (kind, col, label)) in legend-items.enumerate() {
      let y = ly - j * 0.42
      if kind == "points" {
        circle((lx + 0.14, y), radius: 2.2pt / ctx.length, fill: col, stroke: none)
      } else if kind == "bars" {
        rect((lx, y - 0.07), (lx + 0.28, y + 0.07), fill: col.transparentize(70%), stroke: 0.8pt + col)
      } else {
        line((lx, y), (lx + 0.3, y), stroke: 1.2pt + col)
      }
      content((lx + 0.42, y), anchor: "west", text(size: 0.82em, label))
    }
  }
})
