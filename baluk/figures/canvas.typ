// Холст, рисунок с подписью, рисунки в ряд и цвет по имени — основа
// остальных помощников рисунков (`figures.typ` собирает их вместе).

#import "@preview/cetz:0.4.2"
#import "../theme.typ": current-theme
#import "../web.typ": is-web, elem, frame
#import "../i18n.typ": word

#let _draw = cetz.draw

// ── Цвет по имени: "line", "second", "third", "accent" или color ─────────
#let _color(theme, col, fallback: "line") = {
  let col = if col == auto { fallback } else { col }
  if type(col) == color { return col }
  if col == "accent" { return theme.color.accent }
  if col in theme.color.fig { return theme.color.fig.at(col) }
  if col in theme.color.boxes { return theme.color.boxes.at(col) }
  theme.color.fig.line
}

// ── Холст ─────────────────────────────────────────────────────────────────
/// cetz.canvas с настройками темы. Тело — блок { ... } или функция theme => { ... }.
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
  // В HTML — SVG. Его размер задан в em основного текста, поэтому рисунок
  // масштабируется вместе с кеглем страницы.
  if is-web() { frame(drawing) } else { drawing }
}

/// Рисунок с подписью. Подпись — утверждение о том, что видно на рисунке,
/// а не название («Спица входит через φ₁ и выходит через φ₂», а не «Область»).
/// floating: true — typst сам поставит рисунок вверх или вниз страницы
/// (как float в LaTeX). Для крупных рисунков, на которые есть ссылка
/// «рис. N» в тексте: они перестают оставлять дыры внизу страниц.
#let fig(body, caption, label: none, floating: false) = context {
  // В HTML плавающий рисунок пропал бы целиком: у страницы нет верха и низа.
  let fig-el = figure(body, caption: caption, kind: image, supplement: word("figure"),
    placement: if floating and not is-web() { auto } else { none })
  if label != none [#fig-el#std.label(label)] else { fig-el }
}

/// Несколько рисунков в ряд с общей подписью (small multiples).
/// разделители — содержимое между рисунками, например ($+$, $-$, $-$).
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
  // без align(center): рисунок и так центрирует тело, а естественная
  // ширина нужна шаблону, чтобы узнать «широкий» рисунок
  stack(dir: ltr, spacing: gap, ..cells.map(x => box(x, baseline: 50%)))
}
