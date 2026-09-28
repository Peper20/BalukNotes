// Шаблон документа: страница, шрифты, заголовки, колонтитулы, таблицы,
// подписи, титул и оглавление. Вся «вёрстка» живёт здесь; главы содержат
// только текст и вызовы блоков.

#import "theme.typ": _theme-state, _doc-kind, themes, current-theme, pale
#import "code.typ": tm-theme, code-theme
#import "blocks.typ": small-caps
#import "web.typ": is-web, elem, frame
#import "i18n.typ": word, _own-words, _check-words

// ── Мелочи ────────────────────────────────────────────────────────────────
#let _margin-left(theme) = theme.page.margin.at("left", default: theme.page.margin.at("x", default: 2cm))
#let _margin-right(theme) = theme.page.margin.at("right", default: theme.page.margin.at("x", default: 2cm))
#let _margin-top(theme) = theme.page.margin.at("top", default: theme.page.margin.at("y", default: 2.5cm))

#let _chapter-number() = counter(heading).get().first()

// Название текущей главы для колонтитула (последняя глава до этой страницы
// или на ней).
#let _current-chapter() = {
  let before = query(heading.where(level: 1).before(here()))
  let on-page = query(heading.where(level: 1)).filter(h => h.location().page() == here().page())
  if on-page.len() > 0 { on-page.first() } else if before.len() > 0 { before.last() } else { none }
}
#let _chapter-page() = query(heading.where(level: 1)).any(h => h.location().page() == here().page())

// Сброс счётчиков в начале главы (нумерация «глава.n»).
#let _reset-counters() = {
  counter("k-stmt").update(0)
  counter("k-example").update(0)
  counter(figure.where(kind: image)).update(0)
  counter(figure.where(kind: table)).update(0)
  counter(math.equation).update(0)
}

// ═════════════════════════════════════════════════════════════════════════
// Заголовки глав
// ═════════════════════════════════════════════════════════════════════════
// Надпись разрядкой, название, линейка и бледная крупная цифра справа —
// развитие шапки главы из конспекта по матану.
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
// Разделы и подразделы
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
// Колонтитулы
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
// Титульный лист
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
// HTML: приложение
// ═════════════════════════════════════════════════════════════════════════
// Страниц, колонтитулов, титула и оглавления нет: оглавление строит
// приложение. Размеры и цвета задаёт CSS, здесь — только разметка.

// Ссылки на заголовки и рисунки: только номер, как в PDF. В заметке номера
// заголовков по умолчанию скрыты — ссылка на раздел показывает его название.
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

// Номер рисунка: в книге «глава.n», в заметке — сквозной.
#let _fig-numbering(doc-kind) = if doc-kind == "note" { "1" } else {
  n => numbering("1.1", counter(heading).get().first(), n)
}

/// Первое семейство шрифта из `text.font` (строка, массив, словарь с `name`).
#let _family(font) = {
  let first = if type(font) == array { font.first() } else { font }
  if type(first) == dictionary { first.name } else { first }
}

/// Длина Typst (pt, em или их сумма) → длина CSS.
#let _css-length(len) = {
  let num(x) = str(calc.round(x, digits: 4)).replace("−", "-")
  let parts = ()
  if len.em != 0 { parts.push(num(len.em) + "em") }
  if len.abs != 0pt or parts.len() == 0 { parts.push(num(len.abs.pt()) + "pt") }
  if parts.len() == 1 { parts.first() } else { "calc(" + parts.join(" + ") + ")" }
}

#let _web-template(theme, doc-kind, kind, title, subtitle, author, date, description, tags, body) = {
  // Шрифт, кегль и цвет в HTML-разметку не попадают (их задаёт CSS), но их
  // наследуют рисунки: html.frame верстается как страница PDF. Без этого
  // подписи на рисунках — чёрные (не видно в тёмной теме) и чужим шрифтом.
  set text(font: theme.font.text, size: theme.size.text, fill: theme.color.text)
  show math.equation: set text(font: theme.font.math)
  show raw: set text(font: theme.font.code)
  // Номера заголовков выдаются всегда (span.k-num): показывать ли их, решает
  // настройка клиента, без перекомпиляции.
  set heading(numbering: "1.1")
  // Заголовок: <h2…h6>; <h1> занят названием. Класс k-hN — уровень
  // оформления: в книге `=` — глава (k-h1), в заметке `=` — раздел (k-h2).
  let shift = if doc-kind == "note" { 1 } else { 0 }
  show heading: it => {
    if it.level == 1 and doc-kind == "book" { _reset-counters() }
    let tag = "h" + str(calc.min(it.level + 1, 6))
    context {
      let number = if it.numbering != none { counter(heading).display(it.numbering) }
      // data-num у главы — для крупной бледной цифры справа (CSS ::after).
      let is-chapter = it.level == 1 and doc-kind == "book"
      let attrs = if is-chapter and number != none { ("data-num": str(counter(heading).get().first())) } else { (:) }
      // Метка `= Раздел <метка>` — это id: по нему ведут ссылки #see(anchor: "метка").
      if it.has("label") { attrs.insert("id", str(it.label)) }
      elem(tag, "k-h k-h" + str(calc.min(it.level + shift, 4)), ..attrs, {
        // «Глава» перед номером главы рисует CSS (настройка вида) — слово на языке заметки.
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

  // Страховка: в HTML-экспорте содержимое этих элементов пропадает целиком.
  // Выравнивание — классом, раскладку — SVG-кадром (текст в нём не
  // выделяется, но ничего не теряется). Своё в библиотеке их не использует.
  // Внутри кадра (html.frame) вёрстка снова постраничная — там не трогаем.
  show align: it => context if is-web() {
    let ax = it.alignment.x
    let cls = if ax == center { "k-center" } else if ax == right or ax == end { "k-right" } else { "k-left" }
    elem("div", cls, it.body)
  } else { it }
  show grid: it => context if is-web() { frame(it, kind: "layout") } else { it }
  show stack: it => context if is-web() { frame(it, kind: "layout") } else { it }
  show place: it => context if is-web() { frame(it.body, kind: "layout") } else { it }
  // Отступы `h`/`v` HTML-экспорт выбрасывает (с предупреждением). Абсолютная
  // величина (pt, em и их сумма) — пустой элемент с отступом: размер —
  // переменной `--k-h`/`--k-v`, правило — в CSS. Доли (`1fr`) и проценты
  // без страницы смысла не имеют — остаются как есть (и предупреждение).
  // В формулах `h` (и `quad`, `thin`) Typst сам делает `<mspace>` — их не
  // трогаем: внутри формулы шрифт — математический темы.
  let math-font = lower(_family(theme.font.math))
  show h: it => context if is-web() and type(it.amount) == length and lower(_family(text.font)) != math-font {
    elem("span", "k-h", ..("style": "--k-h: " + _css-length(it.amount)), [])
  } else { it }
  show v: it => context if is-web() and type(it.amount) == length {
    elem("div", "k-v", ..("style": "--k-v: " + _css-length(it.amount)), [])
  } else { it }

  // Весь документ — в <article data-doc>: по виду документа (а не по месту
  // файла в хранилище) CSS решает, показывать ли номера и «Главу N».
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

// Общее для книги и заметки в PDF: ссылки, списки, код, таблицы, рисунки,
// титул. Заголовки уже настроены в _pdf-шаблон.
#let _pdf-body(theme, doc-kind, kind, title, subtitle, author, date, description, title-page, toc, depth, body) = {
  let acc = theme.color.accent

  // Ссылки: только номер, падежное слово пишется в тексте руками
  // («в разделе @sec-x»): typst ставит слово в именительном падеже.
  // То же для рисунков: пишем «на рис. @метка», получаем «на рис. 2.3».
  show ref: _ref.with(doc-kind)
  show link: set text(fill: acc)

  // Списки
  set list(marker: (text(fill: acc, "•"), text(fill: acc, "–"), text(fill: acc, "·")), indent: 0.4em, body-indent: 0.55em)
  set enum(numbering: n => text(fill: acc, weight: "bold", [#n.]), indent: 0.2em, body-indent: 0.5em)
  set terms(separator: [: ], hanging-indent: 1.2em)
  show terms.item: it => block(above: 0.7em, [#text(weight: "bold", fill: acc, it.term): #it.description])

  // Код
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

  // Таблицы
  // Линейки вместо сетки: жирная сверху и снизу, тонкая под шапкой.
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
  // Короткие таблицы не рвём; длинную (больше полстраницы) оборачивай в
  // block(breakable: true) и повторяй шапку через table.header.
  show table: it => block(width: 100%, breakable: false,
    stroke: (top: 1pt + theme.color.text.transparentize(20%), bottom: 1pt + theme.color.text.transparentize(20%)), it)

  // Рисунки и подписи: нумерация «глава.n»
  set figure(numbering: _fig-numbering(doc-kind), gap: 0.7em)
  set figure.caption(separator: [. ])
  show figure: set block(above: 1.3em, below: 1.3em, breakable: false)
  show figure.caption: it => context {
    set text(font: theme.font.captions, size: theme.size.small)
    set par(justify: false, first-line-indent: 0em)
    let number = [#it.supplement #it.counter.display(it.numbering)]
    block(width: 90%, [#text(weight: "bold", fill: acc, number). #it.body])
  }

  // Заметка: название строкой сверху, без титульного листа.
  if doc-kind == "note" and title != none {
    block(below: 1.4em, {
      text(font: theme.font.headings, size: theme.size.chapter, weight: "bold", fill: acc, hyphenate: false, title)
      v(0.3em)
      line(length: 100%, stroke: 0.8pt + acc)
    })
  }

  // Титул и оглавление
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
// PDF: страницы, колонтитулы, титул и оглавление
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

  // Заголовки. В книге `=` — глава с новой страницы, в заметке — раздел:
  // заметка короткая и без нумерации (как в Obsidian).
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
// Главные шаблоны
// ═════════════════════════════════════════════════════════════════════════
// Тема по умолчанию — из `--input theme=…` (приложение собирает заметку по
// разу на тему), иначе «классика». Имя темы, которой нет, — ошибка.
#let _theme-from-input() = {
  let name = sys.inputs.at("theme", default: "classic")
  assert(name in themes, message: "нет темы «" + name + "»; есть: " + themes.keys().join(", "))
  themes.at(name)
}

#let _document(doc-kind, theme, lang, words, kind, title, subtitle, author, date, description, tags, title-page, toc, depth, body) = {
  assert(type(lang) == str, message: "lang — код языка строкой, например \"en\"")
  let theme = if theme == auto { _theme-from-input() } else { theme }
  set text(lang: lang)
  _theme-state.update(theme)
  _doc-kind.update(doc-kind)
  // Без своих слов состояние не трогаем: документ тот же, что и без `words:`.
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

/// Книга — большой конспект из глав: `=` — глава с номером.
/// #show: book.with(
///   theme: auto,              // auto — из --input theme=…; или themes.night, своя
///   lang: "ru",               // язык: слова оформления («Глава», «Рис.») и переносы
///   words: (:),               // свои слова поверх словаря: (chapter: "Kapitel", figure: "Abb.")
///   kind: auto,               // надпись над названием (auto — «Конспект»): Задачник, Шпаргалка…
///   title: [...], subtitle: [...], author: [...], date: [...],
///   description: [...],          // 2–3 фразы на титул: для кого и как читать
///   title-page: true, toc: true, depth: 2,   // только PDF
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

/// Заметка — одна тема целиком: `=` — раздел, нумерация сквозная.
///   #import "/_baluk/lib.typ": *
///   #show: note.with(title: [SSH], tags: ("безопасность",))
#let note(
  theme: auto,
  lang: "ru",
  words: (:),
  title: none,
  description: none,
  tags: (),
  body,
) = _document("note", theme, lang, words, none, title, none, none, none, description, tags, false, false, 2, body)
