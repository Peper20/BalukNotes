// HTML mode helpers.
//
// One note builds both to PDF (paged layout) and to HTML (the app). Typst's
// HTML export has no pages and drops `grid`, `stack`, `align`, `place`, `v`,
// `h` together with their content. So every library block has two branches:
// PDF - A4 paged layout, HTML - elements with `k-...` classes styled by CSS
// (`app/src/baluk-css/`, built into `assets/baluk.css`).
//
// HTML has no baked-in colors: theme CSS variables set them. The exception is
// figures (SVG from `html.frame`): a note is compiled once per theme and the
// app merges the variants of each `div.k-frame`.

/// Whether this is an HTML build. Only inside `context`.
#let is-web() = target() == "html"

/// An element with a class: elem("div", "k-box k-def", body, style: "...").
#let elem(tag, cls, body, ..attrs) = html.elem(tag, attrs: (class: cls) + attrs.named(), body)

/// A figure as SVG in the wrapper the theme merge looks for.
/// kind: "fig" - a library figure (slightly larger on screen, see CSS),
///      "layout" - the fallback for grid/stack/place: laid out at the PDF
///      text width, otherwise 1fr columns collapse to zero.
#let frame(body, kind: "fig") = if kind == "fig" {
  elem("div", "k-frame k-fig", html.frame(body))
} else {
  elem("div", "k-frame k-layout", html.frame(block(width: 21cm - 2 * 2.4cm, body)))
}
