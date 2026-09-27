// Псевдо-3D: оси, поверхности, призмы, тела вращения, сечения.

#import "@preview/cetz:0.4.2"
#import "../theme.typ": current-theme, is-dark
#import "canvas.typ": _draw, _color

// ═════════════════════════════════════════════════════════════════════════
// Псевдо-3D (косая проекция). Ось x вправо, z вверх, y уходит ВЛЕВО-вверх
// («от нас»). Зритель смотрит слева-спереди-сверху, поэтому видны грани
// x = min, y = min и верх. Так все три оси идут по видимым рёбрам тела в
// первом октанте или выходят за его контур — подписи осей не прячутся за
// телом (при y вправо-вверх ось y уходила за столбики).
// Глубина для алгоритма художника: −0.45x − y + 0.32z (больше — ближе).
// Меняешь проекцию — меняй и `_depth`, и видимые грани в `_prism`.
// ═════════════════════════════════════════════════════════════════════════

#let p3(x, y, z) = (x - 0.45 * y, z + 0.32 * y)
#let _depth(x, y, z) = -0.45 * x - y + 0.32 * z

/// Оси x, y, z. Подписи ставятся за концами стрелок.
/// hidden: (x: a, y: b, z: c) — длина участка оси, спрятанного за телом.
/// Такие оси рисуют ПОСЛЕ тела: скрытый участок — тонким пунктиром поверх
/// (как невидимые рёбра в учебнике), остальное — сплошной линией.
#let axes3d(x: 3, y: 2.6, z: 2.4, labels: ($x$, $y$, $z$), hidden: (:)) = _draw.get-ctx(ctx => {
  let fc = current-theme().color.fig
  import cetz.draw: *
  let axis-stroke = 0.6pt + fc.axis
  let dashed = (paint: fc.axis.transparentize(35%), thickness: 0.5pt, dash: "dashed")
  let arrow-mark = (end: "stealth", fill: fc.axis, stroke: 0pt, scale: 0.55)
  for (axis, len, e) in (("x", x, (1, 0, 0)), ("y", y, (0, 1, 0)), ("z", z, (0, 0, 1))) {
    let h = hidden.at(axis, default: 0)
    let along(s) = p3(e.at(0) * s, e.at(1) * s, e.at(2) * s)
    if h > 0 { line(along(0), along(h), stroke: dashed) }
    line(along(h), along(len), stroke: axis-stroke, mark: arrow-mark)
  }
  content(p3(x, 0, 0), anchor: "west", labels.at(0), padding: 4pt)
  content(p3(0, y, 0), anchor: "south-east", labels.at(1), padding: 3pt)
  content(p3(0, 0, z), anchor: "south", labels.at(2), padding: 3pt)
})

// Освещённость грани с нормалью n (не обязательно единичной): 0..1.
#let _lighting(n) = {
  let (a, b, c) = n
  let len = calc.sqrt(a * a + b * b + c * c)
  let (lx, ly, lz) = (-0.35, -0.55, 0.76) // свет спереди-слева-сверху
  calc.max(0, (a * lx + b * ly + c * lz) / len)
}

#let _shade(theme, base, k) = {
  // k ∈ [0,1]: 0 — тень, 1 — полностью освещено
  let shadow = if is-dark(theme) { base.darken(35%) } else { base.darken(28%) }
  let light = if is-dark(theme) { base.lighten(18%) } else { base.lighten(45%) }
  color.mix((shadow, 100% - k * 100%), (light, k * 100%))
}

/// Поверхность z = f(x, y) над прямоугольником.
/// style: "shaded" — непрозрачные грани со светотенью (основной вариант),
///      "flat" — непрозрачные грани одного цвета,
///      "wire" — прозрачный каркас (видно, что под поверхностью).
#let surface(f, xr, yr, n: 14, m: auto, style: "shaded", color: auto, edge: auto) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  import cetz.draw: *
  let m = if m == auto { n } else { m }
  let (x0, x1) = xr
  let (y0, y1) = yr
  let hx = (x1 - x0) / n
  let hy = (y1 - y0) / m
  let base = if color == auto { fc.face } else { _color(theme, color) }
  let edge-st = if edge == auto { 0.3pt + fc.line.transparentize(35%) } else { edge }
  if style == "wire" {
    let st = 0.5pt + fc.line.transparentize(30%)
    for j in range(m + 1) {
      let y = y0 + hy * j
      line(..range(n * 3 + 1).map(i => { let x = x0 + (x1 - x0) * i / (n * 3); p3(x, y, f(x, y)) }), stroke: st)
    }
    for i in range(n + 1) {
      let x = x0 + hx * i
      line(..range(m * 3 + 1).map(j => { let y = y0 + (y1 - y0) * j / (m * 3); p3(x, y, f(x, y)) }), stroke: st)
    }
    return
  }
  let cells = ()
  for i in range(n) {
    for j in range(m) {
      let (xa, xb) = (x0 + hx * i, x0 + hx * (i + 1))
      let (ya, yb) = (y0 + hy * j, y0 + hy * (j + 1))
      let (xc, yc) = ((xa + xb) / 2, (ya + yb) / 2)
      cells.push((_depth(xc, yc, f(xc, yc)), xa, xb, ya, yb))
    }
  }
  for (_, xa, xb, ya, yb) in cells.sorted(key: q => q.at(0)) {
    let (za, zb, zc, zd) = (f(xa, ya), f(xb, ya), f(xb, yb), f(xa, yb))
    let face = if style == "shaded" {
      // нормаль по двум диагоналям ячейки
      let u = (xb - xa, yb - ya, zc - za)
      let v = (xa - xb, yb - ya, zd - zb)
      let nrm = (u.at(1) * v.at(2) - u.at(2) * v.at(1), u.at(2) * v.at(0) - u.at(0) * v.at(2), u.at(0) * v.at(1) - u.at(1) * v.at(0))
      _shade(theme, base, _lighting(nrm))
    } else { base }
    line(p3(xa, ya, za), p3(xb, ya, zb), p3(xb, yb, zc), p3(xa, yb, zd), close: true, fill: face, stroke: edge-st)
  }
})

/// Прямоугольный параллелепипед [xa,xb]×[ya,yb]×[0,h] с видимыми гранями
/// (верх, перед, левый бок). Годится для столбиков интегральной суммы.
#let _prism(theme, xa, xb, ya, yb, h, base, edge-st) = {
  import cetz.draw: *
  // левый бок (x = xa), перед (y = ya), верх (z = h)
  line(p3(xa, ya, 0), p3(xa, yb, 0), p3(xa, yb, h), p3(xa, ya, h), close: true, fill: _shade(theme, base, 0.4), stroke: edge-st)
  line(p3(xa, ya, 0), p3(xb, ya, 0), p3(xb, ya, h), p3(xa, ya, h), close: true, fill: _shade(theme, base, 0.68), stroke: edge-st)
  line(p3(xa, ya, h), p3(xb, ya, h), p3(xb, yb, h), p3(xa, yb, h), close: true, fill: _shade(theme, base, 0.95), stroke: edge-st)
}

/// Столбики интегральной суммы: разбиение n × m, высота — значение f
/// в центре ячейки. выделить: ((i, j), ...) — ячейки другим цветом.
#let prisms(f, xr, yr, n: 5, m: 4, gap: 0.0, highlight: (), color: auto, highlight-color: "second") = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  let (x0, x1) = xr
  let (y0, y1) = yr
  let hx = (x1 - x0) / n
  let hy = (y1 - y0) / m
  let base = if color == auto { fc.face } else { _color(theme, color) }
  let bright = _color(theme, highlight-color)
  let edge-st = 0.35pt + fc.line.transparentize(20%)
  let all = ()
  for i in range(n) {
    for j in range(m) {
      let xc = x0 + hx * (i + 0.5)
      let yc = y0 + hy * (j + 0.5)
      all.push((_depth(xc, yc, 0), i, j, xc, yc))
    }
  }
  for (_, i, j, xc, yc) in all.sorted(key: q => q.at(0)) {
    let half-gap = gap / 2
    _prism(theme, x0 + hx * i + half-gap, x0 + hx * (i + 1) - half-gap, y0 + hy * j + half-gap, y0 + hy * (j + 1) - half-gap, f(xc, yc),
      if (i, j) in highlight { bright.lighten(if is-dark(theme) { 0% } else { 25% }) } else { base }, edge-st)
  }
})

/// Плоская фигура в плоскости z = 0 по вершинам (x, y).
#let base-shape(..points, color: auto, label: none) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  import cetz.draw: *
  let pts = points.pos()
  line(..pts.map(p => p3(p.at(0), p.at(1), 0)), close: true,
    fill: if color == auto { theme.color.fig.fill } else { _color(theme, color) },
    stroke: 0.7pt + theme.color.fig.line)
  if label != none {
    let cx = pts.map(p => p.at(0)).sum() / pts.len()
    let cy = pts.map(p => p.at(1)).sum() / pts.len()
    content(p3(cx, cy, 0), label)
  }
})

/// Тело вращения: горизонтальные «обручи» радиуса r(z) и силуэт. Обручи —
/// это буквально линии r = const, то есть идея цилиндрических координат.
#let revolution(r, zmin: 0, zmax: 2, rings: 8, n: 40, color: auto) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  for k in range(rings + 1) {
    let z = zmin + (zmax - zmin) * k / rings
    let rr = r(z)
    if rr > 0.001 {
      line(..range(n + 1).map(i => {
        let t = 2 * calc.pi * i / n
        p3(rr * calc.cos(t), rr * calc.sin(t), z)
      }), close: true, stroke: 0.5pt + col.transparentize(35%))
    }
  }
  for s in (1, -1) {
    line(..range(n + 1).map(i => {
      let z = zmin + (zmax - zmin) * i / n
      p3(s * r(z), 0, z)
    }), stroke: 1pt + col)
  }
})

/// Сечение поверхности z = f(x, y) плоскостью y = y0 — «слой», из которых
/// набирается объём.
#let cross-section(f, y0, xr: (0, 3), n: 40, color: "second", fill-color: true) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  let (a, b) = xr
  let hi = range(n + 1).map(i => { let x = a + (b - a) * i / n; p3(x, y0, f(x, y0)) })
  if fill-color {
    line(..hi, p3(b, y0, 0), p3(a, y0, 0), close: true,
      fill: col.transparentize(if is-dark(theme) { 75% } else { 85% }), stroke: none)
  }
  line(..hi, stroke: 1.1pt + col)
})
