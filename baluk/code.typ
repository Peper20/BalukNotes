// Listings: highlighting in theme colors, line numbers, highlighted lines,
// callout marks ①②③ the text refers to via #callout(n).
//
// Notes rule: a listing goes AFTER the idea is explained and is under 25
// lines. Long code goes to a file next to the chapter (see `code-from-file`),
// the text gets only the key fragment.

#import "theme.typ": current-theme, box-bg
#import "web.typ": is-web, elem
#import "i18n.typ": word

// ── The highlighting theme (tmTheme) is generated from the palette ────────
#let _hex(c) = c.to-hex()

#let _rule(scope, color, style: "") = (
  "<dict><key>scope</key><string>" + scope + "</string><key>settings</key><dict>"
    + "<key>foreground</key><string>" + _hex(color) + "</string>"
    + (if style != "" { "<key>fontStyle</key><string>" + style + "</string>" } else { "" })
    + "</dict></dict>"
)

#let tm-theme(pal) = bytes(
  "<?xml version=\"1.0\" encoding=\"UTF-8\"?>"
    + "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">"
    + "<plist version=\"1.0\"><dict><key>name</key><string>baluk</string><key>settings</key><array>"
    + "<dict><key>settings</key><dict><key>foreground</key><string>" + _hex(pal.text) + "</string></dict></dict>"
    + _rule("comment, punctuation.definition.comment", pal.comment, style: "italic")
    + _rule("string, constant.character", pal.string)
    + _rule("constant.numeric, constant.language", pal.number)
    + _rule("keyword, keyword.control, storage.modifier, keyword.other", pal.keyword, style: "bold")
    + _rule("keyword.operator", pal.text)
    + _rule("storage.type, support.type, entity.name.type, entity.name.class", pal.type)
    + _rule("entity.name.function, support.function, variable.function", pal.function)
    + _rule("meta.preprocessor, keyword.control.import", pal.comment)
    + "</array></dict></plist>",
)

// ── Reference highlighting colors for HTML ────────────────────────────────
// In HTML Typst bakes the highlighting color into style="color: #...". For
// code to recolor with the theme, HTML mode highlights with these stand-in
// colors, and the app replaces them with CSS variables (--k-code-<key>).
// The values must not occur in real themes.
#let code-ref-colors = (
  text: rgb("#010100"), keyword: rgb("#010101"), type: rgb("#010102"),
  string: rgb("#010103"), number: rgb("#010104"), comment: rgb("#010105"),
  function: rgb("#010106"), highlight: rgb("#010107"),
)

/// The highlighting theme for the current mode: reference colors in HTML.
#let code-theme(theme) = if is-web() { code-ref-colors } else { theme.color.code }

// ── Callout mark ──────────────────────────────────────────────────────────
/// A numbered circle. In a listing it is set by the `callouts` parameter, in
/// text - `#callout(2)`, to refer to a line.
#let callout(n) = context {
  let theme = current-theme()
  if is-web() { return elem("span", "k-mark", str(n)) }
  // A box as tall as the x-height, the circle over it: the mark does not
  // widen the line spacing of its line.
  box(width: 1.05em, height: 0.5em, place(center + horizon, circle(
    radius: 0.5em, fill: theme.color.accent, stroke: none,
    align(center + horizon, text(
      font: theme.font.captions, size: 0.62em, weight: "bold", fill: theme.color.bg, str(n),
    )),
  )))
}

#let _lang-name(lg) = (
  cpp: "C++", c: "C", py: "Python", python: "Python", sql: "SQL", rs: "Rust", rust: "Rust",
  js: "JS", ts: "TS", java: "Java", go: "Go", sh: "Shell", bash: "Bash", asm: "ASM",
  typ: "Typst", hs: "Haskell", kt: "Kotlin",
).at(lg, default: upper(lg))

// ── Listing ───────────────────────────────────────────────────────────────
/// - src: a string or a raw block (```cpp ... ```);
/// - lang: if the code is passed as a string;
/// - caption: the title above the listing (function name, what it does);
/// - highlight: line numbers to highlight with a background, e.g. (4, 5);
/// - callouts: a dictionary "line number -> mark number", e.g. ("4": 1, "7": 2);
/// - line-numbers: print line numbers;
/// - complexity: an optional badge under the listing, e.g. [$O(n m)$ / $O(1)$].
#let listing(
  src,
  lang: "cpp",
  caption: none,
  highlight: (),
  callouts: (:),
  line-numbers: true,
  complexity: none,
) = context {
  let theme = current-theme()
  let (code-text, lg) = if type(src) == str {
    (src, lang)
  } else {
    (src.text, if src.has("lang") and src.lang != none { src.lang } else { lang })
  }
  let bg = theme.color.surface
  let total = code-text.trim("\n", at: end).split("\n").len()
  let num-w = if total >= 10 { 1.6em } else { 1.1em }

  if is-web() { return {
    show raw.line: it => {
      let co = callouts.at(str(it.number), default: none)
      let cls = "k-line" + if it.number in highlight { " k-hl" } else { "" }
      elem("span", cls, { it.body; if co != none { callout(co) } })
    }
    set raw(theme: tm-theme(code-ref-colors))
    elem("div", "k-listing" + if line-numbers { " k-numbered" } else { "" }, {
      if caption != none {
        elem("div", "k-listing-cap", [#elem("span", "k-lang", _lang-name(lg)) #caption])
      }
      raw(code-text.trim("\n", at: end), lang: lg, block: true)
      if complexity != none { elem("div", "k-listing-cx")[#word("complexity"): #complexity] }
    })
  } }

  show raw.line: it => {
    let n = it.number
    let hl = n in highlight
    let co = callouts.at(str(n), default: none)
    box(
      width: 100%,
      fill: if hl { theme.color.code.highlight } else { none },
      outset: (x: 4pt, y: 2.9pt),
      grid(
        columns: if line-numbers { (num-w, 1fr, auto) } else { (1fr, auto) },
        column-gutter: 0.8em,
        ..if line-numbers {
          (align(right, text(fill: theme.color.muted.transparentize(25%), size: 0.9em, str(n))),)
        },
        it.body,
        if co != none { callout(co) },
      ),
    )
  }
  set raw(theme: tm-theme(theme.color.code))
  // Ligatures are off: "<=" must not turn into "≤" - in a listing the reader
  // sees what to type.
  set text(font: theme.font.code, size: theme.size.code, ligatures: false, features: ("calt": 0))
  set par(justify: false, leading: 0.62em)

  // A short listing (up to 15 lines) is not split between pages: half a
  // function at the bottom of a page is worse than empty space.
  let can-break = total > 15
  let body = block(
    width: 100%,
    fill: bg,
    stroke: none,
    radius: 2pt,
    inset: (x: 9pt, y: 7pt),
    breakable: can-break,
    raw(code-text.trim("\n", at: end), lang: lg, block: true),
  )

  block(width: 100%, above: 0.9em, below: 0.9em, breakable: can-break, {
    if caption != none {
      block(below: 0.45em, sticky: true, text(
        font: theme.font.captions, size: theme.size.small, fill: theme.color.muted,
        [#text(fill: theme.color.accent, weight: "bold", _lang-name(lg)) #h(0.4em) #caption],
      ))
    }
    body
    if complexity != none {
      block(above: 0.4em, text(font: theme.font.captions, size: theme.size.small, fill: theme.color.muted)[
        #word("complexity"): #complexity
      ])
    }
  })
}

/// A listing from a file by region:
///   // region: name
///   ...
///   // endregion: name
/// IMPORTANT: the path is from the project root (starts with "/"), and the build runs with --root:
///   #code-from-file("/code/prefix.cpp", region: "build")
/// This way the code lives in a real .cpp that is compiled and tested.
#let code-from-file(file-path, region: none, ..rest) = {
  let whole = read(file-path)
  let txt = if region == none { whole } else {
    let begin-marker = "// region: " + region
    let end-marker = "// endregion: " + region
    let lines = whole.split("\n")
    let i = lines.position(s => s.trim() == begin-marker)
    assert(i != none, message: "region \"" + region + "\" not found in " + file-path)
    let j = lines.slice(i + 1).position(s => s.trim() == end-marker)
    assert(j != none, message: "no end of region \"" + region + "\" in " + file-path)
    lines.slice(i + 1, i + 1 + j).join("\n")
  }
  let lg = file-path.split(".").last()
  listing(txt, lang: lg, ..rest)
}
