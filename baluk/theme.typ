// Layout themes.
//
// A theme is a plain dictionary. It is chosen once at the start of a document:
//   #show: book.with(theme: themes.classic, ...)
// The template puts it into state, and all boxes, listings and figures take
// colors and fonts from there (inside `context`). So the same chapter lays
// out in any theme without edits.
//
// Own theme: #let mine = customize(themes.classic, (color: (accent: rgb("#7a1f5c"))))
// - a deep merge, only what changes needs to be given.

// ── Deep dictionary merge ─────────────────────────────────────────────────
#let customize(base, changes) = {
  let result = base
  for (key, val) in changes {
    if key in result and type(result.at(key)) == dictionary and type(val) == dictionary {
      result.insert(key, customize(result.at(key), val))
    } else {
      result.insert(key, val)
    }
  }
  result
}

// ── Fonts ─────────────────────────────────────────────────────────────────
// The first in a list is the main one, the rest are fallbacks for missing glyphs.
#let _text-fonts = ("Gentium Plus", "New Computer Modern")
#let _code-fonts = ("JetBrains Mono", "DejaVu Sans Mono")

// Semantic box colors: each has a saturated color (rule, caption) and a
// background. Kinds differ in HUE: notes are read from a screen, a box
// should be recognized by peripheral vision before reading its caption.
#let _boxes(accent, example, remark, pitfall, idea, algorithm, bg: 94%) = (
  definition: accent,
  theorem: accent.darken(15%),
  example: example,
  remark: remark,
  pitfall: pitfall,
  idea: idea,
  algorithm: algorithm,
  bg-share: bg, // how much to lighten the color for the fill
)

// ═════════════════════════════════════════════════════════════════════════
// A. CLASSIC - the main theme: grown from the calculus notes. Blue accent,
//    boxes with a left rule, book paragraphs with indent.
// ═════════════════════════════════════════════════════════════════════════
#let classic = (
  name: "classic",
  title: "Классика", // title in the interface
  font: (
    text: _text-fonts,
    headings: _text-fonts,
    captions: _text-fonts,
    math: ("New Computer Modern Math",),
    code: _code-fonts,
  ),
  size: (text: 11pt, code: 8.8pt, small: 9.2pt, chapter: 25pt, section: 14pt, subsection: 11.5pt),
  par: (leading: 0.72em, indent: 1.2em, spacing: 0.72em, justify: true),
  page: (margin: (x: 2.4cm, top: 2.6cm, bottom: 2.4cm)),
  color: (
    bg: white,
    text: rgb("#1b1b1b"),
    muted: rgb("#5f6670"),
    line: rgb("#c9ced6"),
    surface: rgb("#f4f6f9"), // background of listings, table headers
    accent: rgb("#1f4e79"),
    secondary: rgb("#c62828"), // "spokes", highlights in figures
    boxes: _boxes(
      rgb("#1f4e79"), rgb("#2e7d32"), rgb("#b8860b"),
      rgb("#b3261e"), rgb("#00796b"), rgb("#6a4c93"),
      bg: 94%,
    ),
    code: (
      text: rgb("#1b1b1b"), keyword: rgb("#1f4e79"), type: rgb("#6a4c93"),
      string: rgb("#2e7d32"), number: rgb("#b5561b"), comment: rgb("#7b8490"),
      function: rgb("#8a3b12"), highlight: rgb("#fff3c4"),
    ),
    fig: (
      line: rgb("#1f4e79"), fill: rgb("#1f4e79").lighten(82%),
      face: rgb("#dce6f2"), second: rgb("#c62828"), third: rgb("#2e7d32"),
      axis: rgb("#1b1b1b"), grid: rgb("#c9ced6"),
    ),
  ),
)

// ═════════════════════════════════════════════════════════════════════════
// ═════════════════════════════════════════════════════════════════════════
// B. NIGHT - the same "classic", recolored for reading from a screen in the
//    dark. ONLY the palette differs: fonts, sizes, margins, headings, boxes
//    and running heads are the same. One set of notes in two looks should
//    look like one set of notes, not two different ones.
// ═════════════════════════════════════════════════════════════════════════
#let night = customize(classic, (
  name: "night",
  title: "Ночь",
  color: (
    bg: rgb("#16181e"),
    text: rgb("#dde2ea"),
    muted: rgb("#8a93a5"),
    line: rgb("#343a48"),
    surface: rgb("#1f232c"),
    accent: rgb("#7fb4ff"),
    secondary: rgb("#ff9e64"),
    boxes: _boxes(
      rgb("#7fb4ff"), rgb("#69d3a0"), rgb("#f2c46d"),
      rgb("#ff8585"), rgb("#5fd0d6"), rgb("#b69cff"),
      // On a dark background the box fill is transparency, not lightening.
      bg: 0%,
    ),
    code: (
      text: rgb("#d4d9e2"), keyword: rgb("#c099ff"), type: rgb("#7fb4ff"),
      string: rgb("#9ece6a"), number: rgb("#ff9e64"), comment: rgb("#6b7489"),
      function: rgb("#5fd0d6"), highlight: rgb("#2e3446"),
    ),
    fig: (
      line: rgb("#7fb4ff"), fill: rgb("#7fb4ff").transparentize(80%),
      face: rgb("#26324a"), second: rgb("#ff9e64"), third: rgb("#69d3a0"),
      axis: rgb("#aab2c2"), grid: rgb("#3a4152"),
    ),
  ),
))

#let themes = (classic: classic, night: night)

// ── Access to the current theme ───────────────────────────────────────────
#let _theme-state = state("baluk-theme", none)

/// Document kind: "book" (chapters, numbering "chapter.n") or "note"
/// (sections, continuous numbering). Set by the template.
#let _doc-kind = state("baluk-doc-kind", "book")

/// The current theme. Call only inside `context`.
#let current-theme() = {
  let theme = _theme-state.get()
  if theme == none { classic } else { theme }
}

/// Whether the theme is dark (by background lightness).
#let is-dark(theme) = theme.color.bg.components().at(0) < 50%

/// A pale shade of a color for large "watermark" details (chapter number)
/// and figure fills: transparency in a dark theme, lightening in a light one.
#let pale(theme, col) = if is-dark(theme) { col.transparentize(88%) } else { col.lighten(88%) }

/// The fill color of a box of this kind for the theme (light or dark).
#let box-bg(theme, color) = {
  if theme.color.boxes.bg-share == 0% { color.transparentize(90%) } else { color.lighten(theme.color.boxes.bg-share) }
}

/// Shadow and light of a face of color `base` for solid figures (interactive
/// surface): a face mixes them by illumination. In HTML - the variables
/// `--k-fig-<color>-dark/-light`, so the client paints the same way.
#let fig-shades(theme, base) = if is-dark(theme) {
  (base.darken(35%), base.lighten(18%))
} else {
  (base.darken(28%), base.lighten(45%))
}

// ── Colors for CSS (HTML mode) ────────────────────────────────────────────
// In HTML CSS sets the colors, but the source of truth is this same theme:
// the build takes the dictionary from css.typ (typst query) and writes CSS
// variables --k-<name>. Derived colors (box fill, pale chapter number) are
// computed here by the same functions as in PDF, so the look matches.
#let _css-boxes = (
  definition: "def", theorem: "thm", example: "example", remark: "remark",
  pitfall: "pitfall", idea: "idea", algorithm: "algo",
)

#let css-colors(theme) = {
  let pal = theme.color
  let bx = pal.boxes
  let dark = is-dark(theme)
  let result = (
    bg: pal.bg, text: pal.text, muted: pal.muted, line: pal.line,
    surface: pal.surface, accent: pal.accent, second: pal.secondary,
    accent-pale: pale(theme, pal.accent),
    accent-bg: box-bg(theme, pal.accent),
    lead: pal.text.transparentize(if dark { 5% } else { 10% }),
    step: bx.example.darken(if dark { 0% } else { 10% }),
    quiz-border: bx.idea.lighten(if dark { 0% } else { 50% }).transparentize(if dark { 55% } else { 0% }),
    table-rule: pal.text.transparentize(20%),
    table-head-rule: pal.text.transparentize(30%),
  )
  for (kind, cls) in _css-boxes {
    result.insert("box-" + cls, bx.at(kind))
    result.insert("box-" + cls + "-bg", box-bg(theme, bx.at(kind)))
  }
  for (name, color) in (second: pal.fig.second, third: pal.fig.third, line: pal.fig.line) {
    result.insert("hl-" + name, if dark { color.transparentize(80%) } else { color.lighten(85%) })
  }
  // figures: colors of interactive figures (the client draws them itself)
  for (name, cls) in (line: "line", second: "second", third: "third", face: "face") {
    let (shadow, light) = fig-shades(theme, pal.fig.at(name))
    result.insert("fig-" + cls, pal.fig.at(name))
    result.insert("fig-" + cls + "-dark", shadow)
    result.insert("fig-" + cls + "-light", light)
  }
  result.insert("fig-axis", pal.fig.axis)
  result.insert("fig-grid", pal.fig.grid)
  for (key, name) in (text: "text", keyword: "key", type: "type", string: "string", number: "number", comment: "comment", function: "function", highlight: "hl") {
    result.insert("code-" + name, pal.code.at(key))
  }
  result.pairs().map(((kk, v)) => (kk, v.to-hex())).to-dict()
}
