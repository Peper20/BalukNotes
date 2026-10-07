// The document template: page, fonts, headings, running heads, tables,
// captions, title page and contents. All the "layout" lives here; chapters
// contain only text and block calls.

#import "theme.typ": _theme-state, _doc-kind, themes, current-theme, pale
#import "code.typ": tm-theme, code-theme
#import "blocks.typ": small-caps
#import "web.typ": is-web, elem, frame
#import "i18n.typ": word, _own-words, _check-words

// ── Small things ──────────────────────────────────────────────────────────
#let _margin-left(theme) = theme.page.margin.at("left", default: theme.page.margin.at("x", default: 2cm))
#let _margin-right(theme) = theme.page.margin.at("right", default: theme.page.margin.at("x", default: 2cm))
#let _margin-top(theme) = theme.page.margin.at("top", default: theme.page.margin.at("y", default: 2.5cm))

#let _chapter-number() = counter(heading).get().first()

// The current chapter title for the running head (the last chapter before
// this page or on it).
#let _current-chapter() = {
  let before = query(heading.where(level: 1).before(here()))
  let on-page = query(heading.where(level: 1)).filter(h => h.location().page() == here().page())
  if on-page.len() > 0 { on-page.first() } else if before.len() > 0 { before.last() } else { none }
}
#let _chapter-page() = query(heading.where(level: 1)).any(h => h.location().page() == here().page())

// Counter reset at a chapter start (numbering "chapter.n").
#let _reset-counters() = {
  counter("k-stmt").update(0)
  counter("k-example").update(0)
  counter(figure.where(kind: image)).update(0)
  counter(figure.where(kind: table)).update(0)
  counter(math.equation).update(0)
}

// ═════════════════════════════════════════════════════════════════════════
// Chapter headings
// ═════════════════════════════════════════════════════════════════════════
// A letter-spaced label, the title, a rule and a large pale digit on the
// right - grown from the chapter header of the calculus notes.
#let _chapter-heading(theme, it) = {
  let acc = theme.color.accent
  let numbered = it.numbering != none
  let n = if numbered { str(_chapter-number()) } else { none }
  set par(first-line-indent: 0em, justify: false)

  block(width: 100%, above: 0pt, below: 1.2em, {
    if numbered {
      place(top + right, dy: -0.4em, text(font: theme.font.headings, size: 72pt, weight: "bold", fill: pale(theme, acc), n))
      text(font: theme.font.headings, size: 9pt, tracking: 0.22em, fill: acc, upper[#word("chapter") #n])
      v(0.25em)
    }
    text(font: theme.font.headings, size: theme.size.chapter, weight: "bold", fill: acc, hyphenate: false, it.body)
    v(0.45em)
    line(length: 100%, stroke: 0.8pt + acc)
  })
}

// ═════════════════════════════════════════════════════════════════════════
// Sections and subsections
// ═════════════════════════════════════════════════════════════════════════
#let _section-heading(theme, it) = {
  let acc = theme.color.accent
  let number = if it.numbering != none { counter(heading).display(it.numbering) } else { none }
  set par(first-line-indent: 0em, justify: false)
  block(above: 1.5em, below: 0.7em, sticky: true,
    text(font: theme.font.headings, size: theme.size.section, weight: "bold", fill: acc, [#if number != none [#number #h(0.3em)]#it.body]))
}

#let _subsection-heading(theme, it) = {
  set par(first-line-indent: 0em, justify: false)
  let number = if it.numbering != none { counter(heading).display(it.numbering) } else { none }
  block(above: 1.2em, below: 0.55em, sticky: true,
    text(font: theme.font.headings, size: theme.size.subsection, weight: "bold", [#if number != none [#number #h(0.25em)]#it.body]))
}

// ═════════════════════════════════════════════════════════════════════════
// Running heads
// ═════════════════════════════════════════════════════════════════════════
#let _page-header(theme) = context {
  if _chapter-page() { return }
  let ch = _current-chapter()
  if ch == none { return }
  let n = counter(heading).at(ch.location()).first()
  let title = if ch.numbering != none [#n. #ch.body] else { ch.body }
  set text(font: theme.font.captions, size: 8pt, fill: theme.color.muted)
  grid(columns: (1fr, auto), text(style: "italic", title), [])
  place(bottom + left, dy: 0.35em, line(length: 100%, stroke: 0.4pt + theme.color.line))
}

#let _page-footer(theme) = context {
  set text(font: theme.font.captions, size: 8.5pt, fill: theme.color.muted)
  align(center, counter(page).display())
}

// ═════════════════════════════════════════════════════════════════════════
// Title page
// ═════════════════════════════════════════════════════════════════════════
#let _title-page(theme, kind, title, subtitle, author, date, description) = {
  let acc = theme.color.accent
  set par(first-line-indent: 0em, justify: false)
  page(header: none, footer: none, {
    v(5cm)
    align(center, {
      text(font: theme.font.headings, size: 9pt, tracking: 0.3em, fill: acc, upper(kind))
      v(0.9em)
      text(font: theme.font.headings, size: 30pt, weight: "bold", fill: acc, hyphenate: false, title)
      v(0.7em)
      if subtitle != none { text(size: 14pt, subtitle) }
      v(1.6cm)
      line(length: 40%, stroke: 0.6pt + acc)
    })
    v(1fr)
    if description != none { align(center, block(width: 80%, text(size: 10pt, style: "italic", fill: theme.color.muted, description))) }
    v(1cm)
    align(center, text(size: 10pt, fill: theme.color.muted, [#author#if author != none and date != none [ · ]#date]))
  })
}

// ═════════════════════════════════════════════════════════════════════════
// HTML: the app
// ═════════════════════════════════════════════════════════════════════════
// No pages, running heads, title page or contents: the app builds the
// contents. CSS sets sizes and colors, here is only the markup.

// References to headings and figures: only the number, as in PDF. In a note
// heading numbers are hidden by default - a reference to a section shows its title.
#let _ref(doc-kind, it) = {
  let el = it.element
  if el == none { return it }
  if el.func() == heading {
    if doc-kind == "note" { link(el.location(), el.body) }
    else { link(el.location(), numbering(el.numbering, ..counter(heading).at(el.location()))) }
  } else if el.func() == figure {
    let n = el.counter.at(el.location()).first()
    if doc-kind == "note" { link(el.location(), str(n)) }
    else { link(el.location(), [#counter(heading).at(el.location()).first().#n]) }
  } else { it }
}

// Figure number: "chapter.n" in a book, continuous in a note.
#let _fig-numbering(doc-kind) = if doc-kind == "note" { "1" } else {
  n => numbering("1.1", counter(heading).get().first(), n)
}

/// The first font family from `text.font` (a string, an array, a dictionary with `name`).
#let _family(font) = {
  let first = if type(font) == array { font.first() } else { font }
  if type(first) == dictionary { first.name } else { first }
}

/// A Typst length (pt, em or their sum) -> a CSS length.
#let _css-length(len) = {
  let num(x) = str(calc.round(x, digits: 4)).replace("−", "-")
  let parts = ()
  if len.em != 0 { parts.push(num(len.em) + "em") }
  if len.abs != 0pt or parts.len() == 0 { parts.push(num(len.abs.pt()) + "pt") }
  if parts.len() == 1 { parts.first() } else { "calc(" + parts.join(" + ") + ")" }
}

#let _web-template(theme, doc-kind, kind, title, subtitle, author, date, description, tags, body) = {
  // Font, size and color do not get into the HTML markup (CSS sets them),
  // but figures inherit them: html.frame lays out like a PDF page. Without
  // this, figure labels are black (invisible in a dark theme) and in a foreign font.
  set text(font: theme.font.text, size: theme.size.text, fill: theme.color.text)
  show math.equation: set text(font: theme.font.math)
  show raw: set text(font: theme.font.code)
  // Heading numbers are always emitted (span.k-num): whether to show them is
  // a client setting, without recompiling.
  set heading(numbering: "1.1")
  // A heading: <h2...h6>; <h1> is taken by the title. The class k-hN is the
  // style level: in a book `=` is a chapter (k-h1), in a note `=` is a section (k-h2).
  let shift = if doc-kind == "note" { 1 } else { 0 }
  show heading: it => {
    if it.level == 1 and doc-kind == "book" { _reset-counters() }
    let tag = "h" + str(calc.min(it.level + 1, 6))
    context {
      let number = if it.numbering != none { counter(heading).display(it.numbering) }
      // data-num of a chapter - for the large pale digit on the right (CSS ::after).
      let is-chapter = it.level == 1 and doc-kind == "book"
      let attrs = if is-chapter and number != none { ("data-num": str(counter(heading).get().first())) } else { (:) }
      // The label `= Section <label>` is the id: #see(anchor: "label") links lead to it.
      if it.has("label") { attrs.insert("id", str(it.label)) }
      elem(tag, "k-h k-h" + str(calc.min(it.level + shift, 4)), ..attrs, {
        // CSS draws "Chapter" before the chapter number (a view setting) - the word in the note's language.
        let word-attrs = if is-chapter { ("data-word": word("chapter")) } else { (:) }
        if number != none { elem("span", "k-num", ..word-attrs, number) }
        it.body
      })
    }
  }
  show ref: _ref.with(doc-kind)

  set raw(theme: tm-theme(code-theme(theme)))

  set figure(numbering: _fig-numbering(doc-kind))
  show figure.caption: it => html.elem("figcaption", {
    elem("span", "k-fig-num")[#it.supplement #context it.counter.display(it.numbering).]
    [ ]
    it.body
  })

  // Fallback: HTML export drops the content of these elements entirely.
  // Alignment becomes a class, layout an SVG frame (its text cannot be
  // selected, but nothing is lost). The library itself does not use them.
  // Inside a frame (html.frame) layout is paged again - leave it alone there.
  show align: it => context if is-web() {
    let ax = it.alignment.x
    let cls = if ax == center { "k-center" } else if ax == right or ax == end { "k-right" } else { "k-left" }
    elem("div", cls, it.body)
  } else { it }
  show grid: it => context if is-web() { frame(it, kind: "layout") } else { it }
  show stack: it => context if is-web() { frame(it, kind: "layout") } else { it }
  show place: it => context if is-web() { frame(it.body, kind: "layout") } else { it }
  // HTML export drops `h`/`v` spacing (with a warning). An absolute amount
  // (pt, em and their sum) becomes an empty element with spacing: the size is
  // the variable `--k-h`/`--k-v`, the rule is in CSS. Fractions (`1fr`) and
  // percentages make no sense without a page - left as is (with the warning).
  // In formulas Typst itself makes `h` (and `quad`, `thin`) an `<mspace>` -
  // leave them: inside a formula the font is the theme's math font.
  let math-font = lower(_family(theme.font.math))
  show h: it => context if is-web() and type(it.amount) == length and lower(_family(text.font)) != math-font {
    elem("span", "k-h", ..("style": "--k-h: " + _css-length(it.amount)), [])
  } else { it }
  show v: it => context if is-web() and type(it.amount) == length {
    elem("div", "k-v", ..("style": "--k-v: " + _css-length(it.amount)), [])
  } else { it }

  // The whole document is in <article data-doc>: by the document kind (not by
  // the file's place in the vault) CSS decides whether to show numbers and "Chapter N".
  elem("article", "k-doc", ..("data-doc": doc-kind, lang: text.lang), {
  if title != none {
    elem("header", "k-title", {
      if kind != none { elem("div", "k-title-kind", kind) }
      html.elem("h1", title)
      if subtitle != none { elem("p", "k-subtitle", subtitle) }
      if description != none { elem("p", "k-description", description) }
      if author != none or date != none {
        elem("p", "k-byline", [#author#if author != none and date != none [ · ]#date])
      }
      if tags.len() > 0 {
        elem("ul", "k-tags", tags.map(x => html.elem("li", x)).join())
      }
    })
  }
  body
  })
}

// Shared by book and note in PDF: references, lists, code, tables, figures,
// title. Headings are already set up in _pdf-template.
#let _pdf-body(theme, doc-kind, kind, title, subtitle, author, date, description, title-page, toc, depth, body) = {
  let acc = theme.color.accent

  // References: only the number, the inflected word is written in the text by
  // hand ("в разделе @sec-x"): typst would put the word in the nominative case.
  // The same for figures: write "на рис. @label", get "на рис. 2.3".
  show ref: _ref.with(doc-kind)
  show link: set text(fill: acc)

  // Lists
  set list(marker: (text(fill: acc, "•"), text(fill: acc, "–"), text(fill: acc, "·")), indent: 0.4em, body-indent: 0.55em)
  set enum(numbering: n => text(fill: acc, weight: "bold", [#n.]), indent: 0.2em, body-indent: 0.5em)
  set terms(separator: [: ], hanging-indent: 1.2em)
  show terms.item: it => block(above: 0.7em, [#text(weight: "bold", fill: acc, it.term): #it.description])

  // Code
  set raw(theme: tm-theme(theme.color.code))
  show raw: set text(font: theme.font.code)
  show raw.where(block: false): it => box(
    fill: theme.color.surface, radius: 2pt, inset: (x: 2.5pt), outset: (y: 2.5pt),
    text(size: 0.9em, it),
  )
  show raw.where(block: true): it => block(
    width: 100%, fill: theme.color.surface, radius: 3pt, inset: (x: 9pt, y: 7pt),
    text(size: theme.size.code, it),
  )

  // Tables
  // Rules instead of a grid: bold at top and bottom, thin under the header.
  set table(
    inset: (x: 7pt, y: 5pt),
    align: left + horizon,
    stroke: (x, y) => if y == 0 {
      (bottom: 0.5pt + theme.color.text.transparentize(30%))
    } else {
      (bottom: 0.3pt + theme.color.line)
    },
  )
  show table: set text(size: 0.94em)
  show table: set par(justify: false, first-line-indent: 0em)
  show table.cell.where(y: 0): set text(font: theme.font.captions, weight: "bold", fill: theme.color.text)
  // Short tables are not split; wrap a long one (over half a page) in
  // block(breakable: true) and repeat the header via table.header.
  show table: it => block(width: 100%, breakable: false,
    stroke: (top: 1pt + theme.color.text.transparentize(20%), bottom: 1pt + theme.color.text.transparentize(20%)), it)

  // Figures and captions: numbering "chapter.n"
  set figure(numbering: _fig-numbering(doc-kind), gap: 0.7em)
  set figure.caption(separator: [. ])
  show figure: set block(above: 1.3em, below: 1.3em, breakable: false)
  show figure.caption: it => context {
    set text(font: theme.font.captions, size: theme.size.small)
    set par(justify: false, first-line-indent: 0em)
    let number = [#it.supplement #it.counter.display(it.numbering)]
    block(width: 90%, [#text(weight: "bold", fill: acc, number). #it.body])
  }

  // A note: the title as a line on top, no title page.
  if doc-kind == "note" and title != none {
    block(below: 1.4em, {
      text(font: theme.font.headings, size: theme.size.chapter, weight: "bold", fill: acc, hyphenate: false, title)
      v(0.3em)
      line(length: 100%, stroke: 0.8pt + acc)
    })
  }

  // Title page and contents
  if title-page { _title-page(theme, kind, title, subtitle, author, date, description) }
  if toc {
    page(header: none, footer: none, {
      block(below: 1.2em, text(font: theme.font.headings, size: 20pt, weight: "bold", fill: acc, word("contents")))
      set par(leading: 0.55em, first-line-indent: 0em)
      show outline.entry.where(level: 1): it => block(above: 1em, text(weight: "bold", fill: acc, it))
      outline(title: none, depth: depth, indent: 1.3em)
    })
  }
  if title-page or toc { counter(page).update(1) }

  body
}

// ═════════════════════════════════════════════════════════════════════════
// PDF: pages, running heads, title page and contents
// ═════════════════════════════════════════════════════════════════════════
#let _pdf-template(theme, doc-kind, kind, title, subtitle, author, date, description, title-page, toc, depth, body) = {
  let acc = theme.color.accent
  set page(
    paper: "a4",
    margin: theme.page.margin,
    fill: theme.color.bg,
    header: _page-header(theme),
    footer: _page-footer(theme),
    header-ascent: 35%,
  )
  set text(font: theme.font.text, size: theme.size.text, fill: theme.color.text, hyphenate: auto)
  show math.equation: set text(font: theme.font.math)
  set par(
    justify: theme.par.justify,
    leading: theme.par.leading,
    first-line-indent: theme.par.indent,
    spacing: theme.par.spacing,
  )
  set block(spacing: theme.par.spacing + 0.25em)

  // Headings. In a book `=` is a chapter on a new page, in a note a section:
  // a note is short and unnumbered (as in Obsidian).
  if doc-kind == "note" {
    show heading.where(level: 1): it => context _section-heading(theme, it)
    show heading.where(level: 2): it => context _subsection-heading(theme, it)
    show heading: it => block(above: 1em, below: 0.5em, sticky: true, text(weight: "bold", it.body))
    _pdf-body(theme, doc-kind, kind, title, subtitle, author, date, description, title-page, toc, depth, body)
  } else {
    set heading(numbering: "1.1")
    show heading.where(level: 1): it => {
      pagebreak(weak: true)
      _reset-counters()
      context _chapter-heading(theme, it)
    }
    show heading.where(level: 2): it => context _section-heading(theme, it)
    show heading.where(level: 3): it => context _subsection-heading(theme, it)
    show heading.where(level: 4): it => block(above: 1em, below: 0.5em, sticky: true, text(weight: "bold", it.body))
    _pdf-body(theme, doc-kind, kind, title, subtitle, author, date, description, title-page, toc, depth, body)
  }
}


// ═════════════════════════════════════════════════════════════════════════
// Main templates
// ═════════════════════════════════════════════════════════════════════════
// The default theme comes from `--input theme=...` (the app builds a note
// once per theme), else "classic". A theme name that does not exist is an error.
#let _theme-from-input() = {
  let name = sys.inputs.at("theme", default: "classic")
  assert(name in themes, message: "no theme \"" + name + "\"; known: " + themes.keys().join(", "))
  themes.at(name)
}

#let _document(doc-kind, theme, lang, words, kind, title, subtitle, author, date, description, tags, title-page, toc, depth, body) = {
  assert(type(lang) == str, message: "lang is a language code as a string, e.g. \"en\"")
  let theme = if theme == auto { _theme-from-input() } else { theme }
  set text(lang: lang)
  _theme-state.update(theme)
  _doc-kind.update(doc-kind)
  // Without own words the state is left alone: the document is the same as without `words:`.
  if _check-words(words) != (:) { _own-words.update(words) }

  set document(title: title, author: if author == none { () } else if type(author) == str { author } else { () })
  show: it => context {
    let kind = if kind == auto { word("book-kind") } else { kind }
    if is-web() {
      _web-template(theme, doc-kind, kind, title, subtitle, author, date, description, tags, it)
    } else {
      _pdf-template(theme, doc-kind, kind, title, subtitle, author, date, description, title-page, toc, depth, it)
    }
  }
  body
}

/// A book - large notes made of chapters: `=` is a numbered chapter.
/// #show: book.with(
///   theme: auto,              // auto - from --input theme=...; or themes.night, your own
///   lang: "ru",               // language: layout words ("Глава", "Рис.") and hyphenation
///   words: (:),               // own words over the dictionary: (chapter: "Kapitel", figure: "Abb.")
///   kind: auto,               // the label above the title (auto - "Конспект"): [Задачник], [Шпаргалка]...
///   title: [...], subtitle: [...], author: [...], date: [...],
///   description: [...],          // 2-3 sentences for the title page: for whom and how to read
///   title-page: true, toc: true, depth: 2,   // PDF only
/// )
#let book(
  theme: auto,
  lang: "ru",
  words: (:),
  kind: auto,
  title: none,
  subtitle: none,
  author: none,
  date: none,
  description: none,
  tags: (),
  title-page: true,
  toc: true,
  depth: 2,
  body,
) = _document("book", theme, lang, words, kind, title, subtitle, author, date, description, tags, title-page, toc, depth, body)

/// A book chapter - its own properties, as of a note: title and tags. What all
/// chapters share (language, theme, words, book tags) is at the book root,
/// `book.with` in `main.typ`: the chapter inherits it. Sets the chapter
/// heading (`=`) and its own tags under it (HTML only: book and note tags are
/// not printed in PDF either).
///   #import "/_baluk/lib.typ": *
///   #show: chapter.with(title: [Integrals], tags: ("integrals",))
#let chapter(title: none, tags: (), label: none, body) = {
  assert(title != none, message: "chapter: needs a chapter title - title: [...]")
  assert(type(tags) == array and tags.all(t => type(t) == str), message: "chapter: tags is an array of strings, e.g. (\"tag\",)")
  assert(label == none or type(label) == str, message: "chapter: label is a string, e.g. \"ch-integrals\"")
  context assert(
    _doc-kind.get() == "book",
    message: "chapter is only for a book chapter (a file included with #include from main.typ with book.with); in a note a section is `=`",
  )
  if label == none { heading(level: 1, title) } else { [#heading(level: 1, title)#std.label(label)] }
  if tags.len() > 0 {
    context if is-web() { elem("ul", "k-tags k-chapter-tags", tags.map(x => html.elem("li", x)).join()) }
  }
  body
}

/// A note - one whole topic: `=` is a section, numbering is continuous.
///   #import "/_baluk/lib.typ": *
///   #show: note.with(title: [SSH], tags: ("security",))
#let note(
  theme: auto,
  lang: "ru",
  words: (:),
  title: none,
  description: none,
  tags: (),
  body,
) = _document("note", theme, lang, words, none, title, none, none, none, description, tags, false, false, 2, body)
