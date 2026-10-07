// Canvas, captioned figure, figures in a row and color by name - the base of
// the other figure helpers (`figures.typ` gathers them).

#import "@preview/cetz:0.4.2"
#import "../theme.typ": current-theme
#import "../web.typ": is-web, elem, frame
#import "../i18n.typ": word

#let _draw = cetz.draw

// ── Color by name: "line", "second", "third", "accent" or color ─────────
#let _color(theme, col, fallback: "line") = {
  let col = if col == auto { fallback } else { col }
  if type(col) == color { return col }
  if col == "accent" { return theme.color.accent }
  if col in theme.color.fig { return theme.color.fig.at(col) }
  if col in theme.color.boxes { return theme.color.boxes.at(col) }
  theme.color.fig.line
}

// ── Canvas ────────────────────────────────────────────────────────────────
/// cetz.canvas with theme settings. The body is a block { ... } or a function theme => { ... }.
#let canvas(body, unit: 1cm) = context {
  let theme = current-theme()
  let fc = theme.color.fig
  let elements = if type(body) == function { body(theme) } else { body }
  let drawing = {
    set text(size: theme.size.small)
    cetz.canvas(length: unit, {
      _draw.set-style(stroke: 0.6pt + fc.axis, content: (padding: 2pt))
      elements
    })
  }
  // In HTML - SVG. Its size is set in em of the main text, so the figure
  // scales with the page font size.
  if is-web() { frame(drawing) } else { drawing }
}

/// A captioned figure. The caption states what the figure shows, it is not
/// a title ("The spoke enters through φ₁ and leaves through φ₂", not "Region").
/// floating: true - typst itself puts the figure at the top or bottom of the
/// page (like a LaTeX float). For large figures referenced as "fig. N" in the
/// text: they stop leaving holes at the bottom of pages.
#let fig(body, caption, label: none, floating: false) = context {
  // In HTML a floating figure would vanish entirely: a page has no top or bottom.
  let fig-el = figure(body, caption: caption, kind: image, supplement: word("figure"),
    placement: if floating and not is-web() { auto } else { none })
  if label != none [#fig-el#std.label(label)] else { fig-el }
}

/// Several figures in a row with a shared caption (small multiples).
/// separators - content between figures, e.g. ($+$, $-$, $-$).
#let in-row(..elements, separators: none, gap: 0.8em) = context {
  let parts = elements.pos()
  if is-web() {
    return elem("div", "k-row", parts.enumerate().map(((i, x)) => {
      x
      if i < parts.len() - 1 and separators != none {
        elem("span", "k-row-sep", separators.at(i, default: []))
      }
    }).join())
  }
  let cells = ()
  for (i, x) in parts.enumerate() {
    cells.push(x)
    if i < parts.len() - 1 and separators != none {
      cells.push(text(size: 1.4em, separators.at(i, default: [])))
    }
  }
  // no align(center): the figure centers the body anyway, and the template
  // needs the natural width to detect a "wide" figure
  stack(dir: ltr, spacing: gap, ..cells.map(x => box(x, baseline: 50%)))
}
