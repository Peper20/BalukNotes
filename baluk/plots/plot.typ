// График с ползунками: interactive-plot.

#import "@preview/cetz:0.4.2"
#import "../figures/canvas.typ": canvas
#import "formula.typ": _formula, _eval
#import "common.typ": *

// ── 2D ───────────────────────────────────────────────────────────────────

/// Участки кривой внутри полосы y ∈ [y0, y1]: точки — (x, y) или (x, none).
/// Отрезок, пересекающий полосу, обрезается по её краю; отрезок между
/// точками по разные стороны от полосы — разрыв (асимптота, как у tg x).
#let _segments(pts, y0, y1) = {
  let all = ()
  let cur-seg = ()
  for i in range(pts.len() - 1) {
    let (xa, ya) = pts.at(i)
    let (xb, yb) = pts.at(i + 1)
    if ya == none or yb == none or (ya > y1 and yb < y0) or (ya < y0 and yb > y1) {
      if cur-seg.len() > 1 { all.push(cur-seg) }
      cur-seg = ()
      continue
    }
    // обрезка отрезка по полосе: параметры входа и выхода t ∈ [0, 1]
    let (t0, t1) = (0.0, 1.0)
    for (bound, above) in ((y0, false), (y1, true)) {
      let out-a = if above { ya > bound } else { ya < bound }
      let out-b = if above { yb > bound } else { yb < bound }
      if out-a and out-b { t0 = 2.0 }
      else if out-a { t0 = calc.max(t0, (bound - ya) / (yb - ya)) }
      else if out-b { t1 = calc.min(t1, (bound - ya) / (yb - ya)) }
    }
    if t0 > t1 {
      if cur-seg.len() > 1 { all.push(cur-seg) }
      cur-seg = ()
      continue
    }
    let p(t) = (xa + (xb - xa) * t, ya + (yb - ya) * t)
    if cur-seg.len() == 0 or t0 > 0 {
      if cur-seg.len() > 1 { all.push(cur-seg) }
      cur-seg = (p(t0),)
    }
    cur-seg.push(p(t1))
    if t1 < 1 {
      if cur-seg.len() > 1 { all.push(cur-seg) }
      cur-seg = ()
    }
  }
  if cur-seg.len() > 1 { all.push(cur-seg) }
  all
}

/// Точки кривой: n + 1 точка на [a, b].
#let _sample(ff, a, b, n, vars) = range(n + 1).map(i => {
  let x = a + (b - a) * i / n
  (x, _eval(ff, vars + (x: x)))
})

/// Диапазон y по точкам: без выбросов (2 % с каждой стороны) и с полями.
#let _range-of(values) = {
  let zz = values.filter(v => v != none).sorted()
  if zz.len() == 0 { return (-1.0, 1.0) }
  let k = calc.floor(zz.len() * 0.02)
  let (lo, hi) = (zz.at(k), zz.at(zz.len() - 1 - k))
  if hi - lo < 1e-9 { return (lo - 1, hi + 1) }
  let slack = (hi - lo) * 0.08
  (lo - slack, hi + slack)
}

/// Кривые: строка, словарь (f:, цвет:, подпись:, пунктир:) или массив таких.
#let _curves(spec) = {
  let entries = if type(spec) == array { spec } else { (spec,) }
  let colors = ("line", "second", "third")
  entries.enumerate().map(((i, crv)) => {
    let crv = if type(crv) == str { (f: crv) } else { crv }
    (
      f: crv.f,
      color: crv.at("color", default: colors.at(calc.rem(i, 3))),
      label: crv.at("label", default: none),
      dashed: crv.at("dashed", default: false),
    )
  })
}

/// График y = f(x) с ползунками параметров.
/// formula: "a * calc.sin(x)" | (f: "…", color: "second", label: "sin", dashed: true) | массив.
/// params: (a: (from: 0, to: 2, step: 0.1, value: 1), …) — step по умолчанию 1/100 диапазона,
///   знач — от.
/// y: (y0, y1) или auto (по кадру при значениях по умолчанию; лучше задать).
/// ширина, высота — размер поля графика в см.
#let interactive-plot(formula, a, b, params: (:), y: auto, labels: ("x", "y"), width: 8, height: 5, n: 160) = {
  let plist = _params(params, ("x", "y", "calc"))
  let names = ("x",) + plist.map(prm => prm.name)
  let curves = _curves(formula)
  let trees = curves.map(crv => _formula(crv.f, names))
  let vars = _env(plist)
  let (a, b) = (float(a), float(b))
  if not (b > a) { panic("interactive-plot: нужно a < b") }
  let points = trees.map(item => _sample(item, a, b, n, vars))
  let (y0, y1) = if y == auto { _range-of(points.join().map(p => p.at(1))) } else { y.map(float) }
  if not (y1 > y0) { panic("interactive-plot: нужно y0 < y1") }
  let data = (
    kind: "2d", curves: curves, x: (a, b), y: (y0, y1), params: plist,
    labels: labels, width: width, height: height,
  )
  let (sx, sy) = (width / (b - a), height / (y1 - y0))
  let to-canvas(p) = ((p.at(0) - a) * sx, (p.at(1) - y0) * sy)
  let frame = canvas(theme => {
    import cetz.draw: *
    let fc = theme.color.fig
    // оси: через ноль, если он в диапазоне, иначе по краю
    let ox = if a <= 0 and 0 <= b { 0.0 } else { a }
    let oy = if y0 <= 0 and 0 <= y1 { 0.0 } else { y0 }
    let (hx, hy) = (_tick-step(b - a), _tick-step(y1 - y0))
    let arrow-mark = (end: "stealth", fill: fc.axis, stroke: 0pt, scale: 0.55)
    let axis-stroke = 0.6pt + fc.axis
    let grid-stroke = 0.3pt + fc.grid
    for x in _divisions(a, b, hx) {
      line(to-canvas((x, y0)), to-canvas((x, y1)), stroke: grid-stroke)
    }
    for yy in _divisions(y0, y1, hy) {
      line(to-canvas((a, yy)), to-canvas((b, yy)), stroke: grid-stroke)
    }
    line(to-canvas((a, oy)), to-canvas((b, oy)), stroke: axis-stroke, mark: arrow-mark)
    line(to-canvas((ox, y0)), to-canvas((ox, y1)), stroke: axis-stroke, mark: arrow-mark)
    let small-text(src) = text(size: 0.85em, fill: fc.axis, src)
    for x in _divisions(a, b, hx) {
      if calc.abs(x - ox) > hx / 2 and a + hx / 3 < x and x < b - hx / 3 {
        content(to-canvas((x, oy)), anchor: "north", padding: 2pt, small-text(_num(x, hx)))
      }
    }
    for yy in _divisions(y0, y1, hy) {
      if calc.abs(yy - oy) > hy / 2 and y0 + hy / 3 < yy and yy < y1 - hy / 3 {
        content(to-canvas((ox, yy)), anchor: "east", padding: 2pt, small-text(_num(yy, hy)))
      }
    }
    content(to-canvas((b, oy)), anchor: "north-east", padding: (top: 7pt), emph(labels.at(0)))
    content(to-canvas((ox, y1)), anchor: "north-west", padding: (left: 5pt), emph(labels.at(1)))
    for (crv, cpts) in curves.zip(points) {
      let col = _fig-color(theme, crv.color)
      let segs = _segments(cpts, y0, y1)
      for seg in segs {
        line(..seg.map(to-canvas), stroke: (paint: col, thickness: 1.1pt, join: "round", dash: if crv.dashed { "dashed" } else { none }))
      }
    }
  })
  _figure(data, frame, plist, legend: curves.filter(crv => crv.label != none))
}
