// A rotatable surface: interactive-surface.

#import "@preview/cetz:0.4.2"
#import "../theme.typ": fig-shades
#import "../figures/canvas.typ": canvas
#import "formula.typ": _formula, _eval
#import "common.typ": *

// ── 3D ───────────────────────────────────────────────────────────────────
//
// The surface is normalized into the box [-1, 1] × [-1, 1] × [-0.7, 0.7] and
// drawn orthographically: a turn around the vertical by `rotation` (azimuth),
// then a tilt by `tilt`. The client rotates from the same view.

#let _Z = 0.7

/// Screen coordinates and depth (larger is closer to the viewer) of a box point.
#let _project(p, θ, φ) = {
  let (x, y, z) = p
  let xr = x * calc.cos(θ) - y * calc.sin(θ)
  let yr = x * calc.sin(θ) + y * calc.cos(θ)
  (xr, z * calc.cos(φ) + yr * calc.sin(φ), -yr * calc.cos(φ) + z * calc.sin(φ))
}

/// Illumination 0..1 of a face with normal n (in screen axes: u, v, depth).
/// Light comes from the viewer's top left; both sides of a face are lit alike.
#let _illum(n) = {
  let (a, b, c) = n
  let len = calc.sqrt(a * a + b * b + c * c)
  if len == 0 { return 0.5 }
  let (lu, lv, ld) = (-0.38, 0.62, 0.69)
  calc.abs(a * lu + b * lv + c * ld) / len
}

/// Face color: a mix of the theme's shadow and light (as `--k-fig-*-dark/-light` in CSS).
#let _tone(theme, base, k) = {
  let (shadow, light) = fig-shades(theme, base)
  color.mix((shadow, 100% - k * 100%), (light, k * 100%), space: rgb)
}

/// A surface z = f(x, y), rotated by dragging, with sliders.
/// formula: "calc.sin(a * x) * calc.cos(y)"; xr, yr - ranges (x0, x1).
/// z: (z0, z1) or auto (from the frame at default values).
/// style: "shaded" - faces with light and shade, "wire" - a wireframe.
/// color: "face" (default), "line", "second", "third".
/// rotation, tilt - the initial view (in degrees).
#let interactive-surface(formula, xr, yr, params: (:), z: auto, labels: ("x", "y", "z"), n: 24, style: "shaded", color: "face", rotation: -30, tilt: 28, size: 3) = {
  let plist = _params(params, ("x", "y", "calc"))
  let names = ("x", "y") + plist.map(prm => prm.name)
  let ast = _formula(formula, names)
  let vars = _env(plist)
  let (x0, x1) = xr.map(float)
  let (y0, y1) = yr.map(float)
  if not (x1 > x0 and y1 > y0) { panic("interactive-surface: needs x0 < x1 and y0 < y1") }
  if color not in ("face", "line", "second", "third") { panic("interactive-surface: color is face, line, second or third") }
  // values at grid nodes (n + 1) × (n + 1)
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
  if not (z1 > z0) { panic("interactive-surface: needs z0 < z1") }
  let data = (
    kind: "3d", f: formula, x: (x0, x1), y: (y0, y1), z: (z0, z1), params: plist,
    labels: labels, n: n, style: style, color: color, view: (rotation, tilt), size: size,
  )
  let (θ, φ) = (rotation * 1deg, tilt * 1deg)
  let S = size / 2
  // node -> box point
  let to-box(i, j, v) = (-1 + 2 * i / n, -1 + 2 * j / n, -_Z + 2 * _Z * (v - z0) / (z1 - z0))
  let to-screen(p) = { let (u, vv, _) = _project(p, θ, φ); (u * S, vv * S) }
  let frame = canvas(theme => {
    import cetz.draw: *
    let fc = theme.color.fig
    let base = fc.at(color)
    let axis = 0.5pt + fc.axis
    // the box floor and the vertical edge in the far corner
    let corners = ((-1, -1), (1, -1), (1, 1), (-1, 1))
    let far = corners.sorted(key: ((x, y)) => _project((x, y, 0), θ, φ).at(2)).first()
    line(..corners.map(((x, y)) => to-screen((x, y, -_Z))), close: true, stroke: axis)
    line(to-screen((..far, -_Z)), to-screen((..far, _Z)), stroke: axis)
    // faces: far ones first (painter's algorithm); points outside [z0, z1] are not drawn
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
        // normal from the face diagonals (in screen axes)
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
    // axis labels - over the faces
    let small-text(src) = text(fill: fc.axis, emph(src))
    // x - at the edge along x nearest to the viewer, y - at the nearest one along y
    let nearer(a, b) = if _project(a, θ, φ).at(2) > _project(b, θ, φ).at(2) { a } else { b }
    content(to-screen(nearer((0, -1.22, -_Z), (0, 1.22, -_Z))), small-text(labels.at(0)))
    content(to-screen(nearer((-1.22, 0, -_Z), (1.22, 0, -_Z))), small-text(labels.at(1)))
    content(to-screen((far.at(0), far.at(1), _Z + 0.14)), small-text(labels.at(2)))
  })
  _figure(data, frame, plist)
}
