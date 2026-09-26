// Поверхность с вращением: interactive-surface.

#import "@preview/cetz:0.4.2"
#import "../theme.typ": fig-shades
#import "../figures/canvas.typ": canvas
#import "formula.typ": _formula, _eval
#import "common.typ": *

// ── 3D ───────────────────────────────────────────────────────────────────
//
// Поверхность нормируется в коробку [-1, 1] × [-1, 1] × [-0.7, 0.7] и
// рисуется ортогонально: поворот вокруг вертикали на угол `rotation`
// (азимут), потом наклон на `tilt`. Клиент вращает с того же вида.

#let _Z = 0.7

/// Экранные координаты и глубина (больше — ближе к зрителю) точки коробки.
#let _project(p, θ, φ) = {
  let (x, y, z) = p
  let xr = x * calc.cos(θ) - y * calc.sin(θ)
  let yr = x * calc.sin(θ) + y * calc.cos(θ)
  (xr, z * calc.cos(φ) + yr * calc.sin(φ), -yr * calc.cos(φ) + z * calc.sin(φ))
}

/// Освещённость 0..1 грани с нормалью n (в осях экрана: u, v, глубина).
/// Свет — от зрителя слева сверху; обе стороны грани освещены одинаково.
#let _illum(n) = {
  let (a, b, c) = n
  let len = calc.sqrt(a * a + b * b + c * c)
  if len == 0 { return 0.5 }
  let (lu, lv, ld) = (-0.38, 0.62, 0.69)
  calc.abs(a * lu + b * lv + c * ld) / len
}

/// Цвет грани: смесь тени и света темы (как `--k-fig-*-dark/-light` в CSS).
#let _tone(theme, base, k) = {
  let (shadow, light) = fig-shades(theme, base)
  color.mix((shadow, 100% - k * 100%), (light, k * 100%), space: rgb)
}

/// Поверхность z = f(x, y) с вращением перетаскиванием и ползунками.
/// formula: "calc.sin(a * x) * calc.cos(y)"; xr, yr — диапазоны (x0, x1).
/// z: (z0, z1) или auto (по кадру при значениях по умолчанию).
/// style: "shaded" — грани со светотенью, "wire" — каркас.
/// color: "face" (по умолчанию), "line", "second", "third".
/// поворот, наклон — начальный вид (в градусах).
#let interactive-surface(formula, xr, yr, params: (:), z: auto, labels: ("x", "y", "z"), n: 24, style: "shaded", color: "face", rotation: -30, tilt: 28, size: 3) = {
  let plist = _params(params, ("x", "y", "calc"))
  let names = ("x", "y") + plist.map(prm => prm.name)
  let ast = _formula(formula, names)
  let vars = _env(plist)
  let (x0, x1) = xr.map(float)
  let (y0, y1) = yr.map(float)
  if not (x1 > x0 and y1 > y0) { panic("interactive-surface: нужно x0 < x1 и y0 < y1") }
  if color not in ("face", "line", "second", "third") { panic("interactive-surface: color — face, line, second или third") }
  // значения в узлах сетки (n + 1) × (n + 1)
  let grid-vals = range(n + 1).map(i => range(n + 1).map(j => {
    let x = x0 + (x1 - x0) * i / n
    let y = y0 + (y1 - y0) * j / n
    _eval(ast, vars + (x: x, y: y))
  }))
  let (z0, z1) = if z == auto {
    let zs = grid-vals.join().filter(v => v != none)
    if zs.len() == 0 { (-1.0, 1.0) } else {
      let (lo, hi) = (calc.min(..zs), calc.max(..zs))
      if hi - lo < 1e-9 { (lo - 1, hi + 1) } else { (lo, hi) }
    }
  } else { z.map(float) }
  if not (z1 > z0) { panic("interactive-surface: нужно z0 < z1") }
  let data = (
    kind: "3d", f: formula, x: (x0, x1), y: (y0, y1), z: (z0, z1), params: plist,
    labels: labels, n: n, style: style, color: color, view: (rotation, tilt), size: size,
  )
  let (θ, φ) = (rotation * 1deg, tilt * 1deg)
  let S = size / 2
  // узел → точка коробки
  let to-box(i, j, v) = (-1 + 2 * i / n, -1 + 2 * j / n, -_Z + 2 * _Z * (v - z0) / (z1 - z0))
  let to-screen(p) = { let (u, vv, _) = _project(p, θ, φ); (u * S, vv * S) }
  let frame = canvas(theme => {
    import cetz.draw: *
    let fc = theme.color.fig
    let base = fc.at(color)
    let axis = 0.5pt + fc.axis
    // пол коробки и вертикальное ребро в дальнем углу
    let corners = ((-1, -1), (1, -1), (1, 1), (-1, 1))
    let far = corners.sorted(key: ((x, y)) => _project((x, y, 0), θ, φ).at(2)).first()
    line(..corners.map(((x, y)) => to-screen((x, y, -_Z))), close: true, stroke: axis)
    line(to-screen((..far, -_Z)), to-screen((..far, _Z)), stroke: axis)
    // грани: дальние раньше (алгоритм художника); точки вне [z0, z1] — не рисуем
    let faces = ()
    for i in range(n) {
      for j in range(n) {
        let quad = (grid-vals.at(i).at(j), grid-vals.at(i + 1).at(j), grid-vals.at(i + 1).at(j + 1), grid-vals.at(i).at(j + 1))
        if quad.any(v => v == none or v < z0 or v > z1) { continue }
        let p = (to-box(i, j, quad.at(0)), to-box(i + 1, j, quad.at(1)), to-box(i + 1, j + 1, quad.at(2)), to-box(i, j + 1, quad.at(3)))
        let proj = p.map(q => _project(q, θ, φ))
        let dep = proj.map(q => q.at(2)).sum() / 4
        faces.push((dep, proj))
      }
    }
    let edge = 0.3pt + fc.line.transparentize(35%)
    for (_, proj) in faces.sorted(key: ax => ax.at(0)) {
      let pts = proj.map(q => (q.at(0) * S, q.at(1) * S))
      if style == "wire" {
        line(..pts, close: true, stroke: 0.5pt + base)
      } else {
        // нормаль по диагоналям грани (в осях экрана)
        let d1 = range(3).map(c => proj.at(2).at(c) - proj.at(0).at(c))
        let d2 = range(3).map(c => proj.at(3).at(c) - proj.at(1).at(c))
        let nrm = (
          d1.at(1) * d2.at(2) - d1.at(2) * d2.at(1),
          d1.at(2) * d2.at(0) - d1.at(0) * d2.at(2),
          d1.at(0) * d2.at(1) - d1.at(1) * d2.at(0),
        )
        line(..pts, close: true, fill: _tone(theme, base, _illum(nrm)), stroke: edge)
      }
    }
    // подписи осей — поверх граней
    let small-text(src) = text(fill: fc.axis, emph(src))
    // x — у ближнего к зрителю ребра вдоль x, y — у ближнего вдоль y
    let nearer(a, b) = if _project(a, θ, φ).at(2) > _project(b, θ, φ).at(2) { a } else { b }
    content(to-screen(nearer((0, -1.22, -_Z), (0, 1.22, -_Z))), small-text(labels.at(0)))
    content(to-screen(nearer((-1.22, 0, -_Z), (1.22, 0, -_Z))), small-text(labels.at(1)))
    content(to-screen((far.at(0), far.at(1), _Z + 0.14)), small-text(labels.at(2)))
  })
  _figure(data, frame, plist)
}
