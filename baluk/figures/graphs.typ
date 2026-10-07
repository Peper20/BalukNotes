// Graphs and trees: drawing a graph and vertex layouts.

#import "@preview/cetz:0.4.2"
#import "../theme.typ": current-theme
#import "canvas.typ": _draw

// ═════════════════════════════════════════════════════════════════════════
// Graphs (y axis up)
// ═════════════════════════════════════════════════════════════════════════

/// A graph.
/// - vertices: a dictionary name -> (x, y);
/// - edges: an array of (u, v) | (u, v, style) | (u, v, style, label);
///   styles: "normal", "bold", "second", "dim", "dashed";
/// - highlight: names of vertices filled with the accent; dimmed: muted ones;
/// - labels: a dictionary name -> content (the name itself by default);
/// - marks: a dictionary name -> (content, anchor) - a mark next to a vertex;
/// - shape: "circle" or "rect" (for nodes with two lines - the key and
///   priority of a treap, an automaton state and so on);
///   the rectangle size is `size: (width, height)`.
/// Opposite edges u->v and v->u in a directed graph are separated automatically.
#let graph(
  vertices, edges, directed: false, radius: 0.27, highlight: (),
  dimmed: (), labels: (:), marks: (:), shape: "circle", size: (0.72, 0.46),
) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  import cetz.draw: *
  let pairs = edges.map(e => (e.at(0), e.at(1)))
  for e in edges {
    let (u, v) = (e.at(0), e.at(1))
    let style = e.at(2, default: "normal")
    let label = e.at(3, default: none)
    // Edge label: the side relative to the direction u -> v and the place along the edge (0 - at u, 1 - at v).
    let opts = e.at(4, default: (:))
    assert(type(opts) == dictionary, message: "graph: the fifth edge element is a dictionary (side: \"left\"/\"right\", at: 0..1)")
    let side = opts.at("side", default: "left")
    assert(side in ("left", "right"), message: "graph: side is \"left\" or \"right\"")
    let at = opts.at("at", default: 0.5)
    let (x1, y1) = vertices.at(u)
    let (x2, y2) = vertices.at(v)
    let (dx, dy) = (x2 - x1, y2 - y1)
    let d = calc.sqrt(dx * dx + dy * dy)
    let (ux, uy) = (dx / d, dy / d)
    // offset of opposite edges
    let s = if directed and (v, u) in pairs { 0.09 } else { 0 }
    let (ox, oy) = (-uy * s, ux * s)
    // to the node edge: the radius for a circle, the intersection with a side for a rectangle
    let edge = if shape == "rect" {
      let (a, b) = (size.at(0) / 2, size.at(1) / 2)
      calc.min(
        if calc.abs(ux) < 0.001 { 1e9 } else { a / calc.abs(ux) },
        if calc.abs(uy) < 0.001 { 1e9 } else { b / calc.abs(uy) },
      )
    } else { radius }
    let p = (x1 + ux * edge + ox, y1 + uy * edge + oy)
    let q = (x2 - ux * edge + ox, y2 - uy * edge + oy)
    let (col, thickness, dash) = if style == "bold" { (fc.line, 1.5pt, none) } else if style == "second" { (fc.second, 1.5pt, none) } else if style == "dim" { (theme.color.muted.transparentize(45%), 0.6pt, none) } else if style == "dashed" { (theme.color.muted, 0.7pt, "dashed") } else { (fc.axis.transparentize(15%), 0.8pt, none) }
    line(p, q, stroke: (paint: col, thickness: thickness, dash: dash),
      mark: if directed { (end: "stealth", fill: col, stroke: 0pt, scale: 0.55) } else { none })
    if label != none {
      let sgn = if side == "left" { 1 } else { -1 }
      let m = (p.at(0) + (q.at(0) - p.at(0)) * at - sgn * uy * 0.16, p.at(1) + (q.at(1) - p.at(1)) * at + sgn * ux * 0.16)
      content(m, box(fill: theme.color.bg, inset: 1.2pt, radius: 1.5pt, text(size: 0.78em, fill: if style in ("bold", "second") { col } else { theme.color.text }, label)))
    }
  }
  for (name, pos) in vertices {
    let sel = name in highlight
    let dim = name in dimmed
    let fill-color = if sel { fc.line } else { theme.color.bg }
    let border = (if dim { 0.6pt } else { 0.9pt }) + (if dim { theme.color.muted.transparentize(40%) } else { fc.line })
    if shape == "rect" {
      let (a, b) = (size.at(0) / 2, size.at(1) / 2)
      rect((pos.at(0) - a, pos.at(1) - b), (pos.at(0) + a, pos.at(1) + b),
        fill: fill-color, stroke: border, radius: 0.06)
    } else {
      circle(pos, radius: radius, fill: fill-color, stroke: border)
    }
    content(pos, text(size: 0.85em, weight: if sel { "bold" } else { "regular" },
      fill: if sel { theme.color.bg } else if dim { theme.color.muted } else { theme.color.text },
      labels.at(name, default: name)))
    if name in marks {
      let (mk, label-anchor) = marks.at(name)
      content(pos, anchor: label-anchor, text(size: 0.75em, fill: fc.second, mk),
        padding: (if shape == "rect" { size.at(0) / 2 } else { radius }) * 1cm + 2pt)
    }
  }
})

/// A tree layout by levels: the root on top, children below. Each subtree
/// gets a strip of its own width - neighbors do not overlap.
#let tree-layout(edges, root, dx: 0.9, dy: 0.95) = {
  let children = (:)
  for e in edges {
    let (u, v) = (e.at(0), e.at(1))
    children.insert(u, children.at(u, default: ()) + (v,))
  }
  let width(v) = {
    let item = children.at(v, default: ())
    if item.len() == 0 { 1 } else { item.map(width).sum() }
  }
  let cell-pos = (:)
  let lay-out(v, x0, dep) = {
    let item = children.at(v, default: ())
    let w = width(v)
    let summary = ((v, ((x0 + w / 2 - 0.5) * dx, -dep * dy)),)
    let x = x0
    for c in item {
      summary += lay-out(c, x, dep + 1)
      x += width(c)
    }
    summary
  }
  for (v, p) in lay-out(root, 0, 0) { cell-pos.insert(v, p) }
  cell-pos
}

/// A binary tree layout: x is the left-to-right traversal order (in-order),
/// y is the depth. This is how search trees, treaps and segment trees are
/// drawn: keys go left to right.
/// children: a dictionary name -> (left, right) - `none` where there is no child.
#let binary-layout(children, root, dx: 0.85, dy: 0.95) = {
  // in-order: the left subtree first, then the node, then the right one.
  // The counter is passed in and returned - closures in typst do not change
  // variables of the outer scope.
  let visit(v, depth, i) = {
    if v == none { return ((), i) }
    let (cur, prm) = children.at(v, default: (none, none))
    let (on-left, i2) = visit(cur, depth + 1, i)
    let me = ((v, (i2 * dx, -depth * dy)),)
    let (on-right, i3) = visit(prm, depth + 1, i2 + 1)
    (on-left + me + on-right, i3)
  }
  let (pairs, _) = visit(root, 0, 0)
  let cell-pos = (:)
  for (v, p) in pairs { cell-pos.insert(v, p) }
  cell-pos
}

/// Binary tree edges from the children dictionary - in the form `graph` expects.
#let binary-edges(children, style: "normal") = {
  let edges = ()
  for (v, (cur, prm)) in children {
    if cur != none { edges.push((v, cur, style)) }
    if prm != none { edges.push((v, prm, style)) }
  }
  edges
}

/// A circle layout (the first vertex on top, then clockwise).
#let circle-layout(names, radius: 1.3, start-angle: 90deg) = {
  let n = names.len()
  let cell-pos = (:)
  for (i, v) in names.enumerate() {
    let a = start-angle - 360deg * i / n
    cell-pos.insert(v, (radius * calc.cos(a), radius * calc.sin(a)))
  }
  cell-pos
}
