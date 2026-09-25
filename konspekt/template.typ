// Шаблон документа: страница, шрифты, заголовки, колонтитулы, таблицы,
// подписи, титул и оглавление. Вся «вёрстка» живёт здесь; главы содержат
// только текст и вызовы блоков.

#import "theme.typ": _тема, _вид, темы, тема, бледный
#import "code.typ": tm-тема, тема-кода
#import "blocks.typ": капитель
#import "web.typ": веб, эл, кадр

// ── Мелочи ────────────────────────────────────────────────────────────────
#let _левое(т) = т.страница.поля.at("left", default: т.страница.поля.at("x", default: 2cm))
#let _правое(т) = т.страница.поля.at("right", default: т.страница.поля.at("x", default: 2cm))
#let _верх(т) = т.страница.поля.at("top", default: т.страница.поля.at("y", default: 2.5cm))

#let _номер-главы() = counter(heading).get().first()

// Название текущей главы для колонтитула (последняя глава до этой страницы
// или на ней).
#let _текущая-глава() = {
  let до = query(heading.where(level: 1).before(here()))
  let на = query(heading.where(level: 1)).filter(h => h.location().page() == here().page())
  if на.len() > 0 { на.first() } else if до.len() > 0 { до.last() } else { none }
}
#let _страница-главы() = query(heading.where(level: 1)).any(h => h.location().page() == here().page())

// Сброс счётчиков в начале главы (нумерация «глава.n»).
#let _сброс() = {
  counter("k-утв").update(0)
  counter("k-пример").update(0)
  counter(figure.where(kind: image)).update(0)
  counter(figure.where(kind: table)).update(0)
  counter(math.equation).update(0)
}

// ═════════════════════════════════════════════════════════════════════════
// Заголовки глав
// ═════════════════════════════════════════════════════════════════════════
// Надпись разрядкой, название, линейка и бледная крупная цифра справа —
// развитие шапки главы из конспекта по матану.
#let _глава(т, it) = {
  let а = т.цвет.акцент
  let нум = it.numbering != none
  let n = if нум { str(_номер-главы()) } else { none }
  set par(first-line-indent: 0em, justify: false)

  block(width: 100%, above: 0pt, below: 1.2em, {
    if нум {
      place(top + right, dy: -0.4em, text(font: т.шрифт.заголовки, size: 72pt, weight: "bold", fill: бледный(т, а), n))
      text(font: т.шрифт.заголовки, size: 9pt, tracking: 0.22em, fill: а, upper[Глава #n])
      v(0.25em)
    }
    text(font: т.шрифт.заголовки, size: т.кегль.глава, weight: "bold", fill: а, hyphenate: false, it.body)
    v(0.45em)
    line(length: 100%, stroke: 0.8pt + а)
  })
}

// ═════════════════════════════════════════════════════════════════════════
// Разделы и подразделы
// ═════════════════════════════════════════════════════════════════════════
#let _раздел(т, it) = {
  let а = т.цвет.акцент
  let номер = if it.numbering != none { counter(heading).display(it.numbering) } else { none }
  set par(first-line-indent: 0em, justify: false)
  block(above: 1.5em, below: 0.7em, sticky: true,
    text(font: т.шрифт.заголовки, size: т.кегль.раздел, weight: "bold", fill: а, [#if номер != none [#номер #h(0.3em)]#it.body]))
}

#let _подраздел(т, it) = {
  set par(first-line-indent: 0em, justify: false)
  let номер = if it.numbering != none { counter(heading).display(it.numbering) } else { none }
  block(above: 1.2em, below: 0.55em, sticky: true,
    text(font: т.шрифт.заголовки, size: т.кегль.подраздел, weight: "bold", [#if номер != none [#номер #h(0.25em)]#it.body]))
}

// ═════════════════════════════════════════════════════════════════════════
// Колонтитулы
// ═════════════════════════════════════════════════════════════════════════
#let _верхний(т) = context {
  if _страница-главы() { return }
  let гл = _текущая-глава()
  if гл == none { return }
  let n = counter(heading).at(гл.location()).first()
  let название = if гл.numbering != none [#n. #гл.body] else { гл.body }
  set text(font: т.шрифт.подписи, size: 8pt, fill: т.цвет.приглушённый)
  grid(columns: (1fr, auto), text(style: "italic", название), [])
  place(bottom + left, dy: 0.35em, line(length: 100%, stroke: 0.4pt + т.цвет.линия))
}

#let _нижний(т) = context {
  set text(font: т.шрифт.подписи, size: 8.5pt, fill: т.цвет.приглушённый)
  align(center, counter(page).display())
}

// ═════════════════════════════════════════════════════════════════════════
// Титульный лист
// ═════════════════════════════════════════════════════════════════════════
#let _титул(т, вид, название, подзаголовок, автор, дата, описание) = {
  let а = т.цвет.акцент
  set par(first-line-indent: 0em, justify: false)
  page(header: none, footer: none, {
    v(5cm)
    align(center, {
      text(font: т.шрифт.заголовки, size: 9pt, tracking: 0.3em, fill: а, upper(вид))
      v(0.9em)
      text(font: т.шрифт.заголовки, size: 30pt, weight: "bold", fill: а, hyphenate: false, название)
      v(0.7em)
      if подзаголовок != none { text(size: 14pt, подзаголовок) }
      v(1.6cm)
      line(length: 40%, stroke: 0.6pt + а)
    })
    v(1fr)
    if описание != none { align(center, block(width: 80%, text(size: 10pt, style: "italic", fill: т.цвет.приглушённый, описание))) }
    v(1cm)
    align(center, text(size: 10pt, fill: т.цвет.приглушённый, [#автор#if автор != none and дата != none [ · ]#дата]))
  })
}

// ═════════════════════════════════════════════════════════════════════════
// HTML: сайт и приложение
// ═════════════════════════════════════════════════════════════════════════
// Страниц, колонтитулов, титула и оглавления нет: оглавление строит
// приложение. Размеры и цвета задаёт CSS, здесь — только разметка.

// Ссылки на заголовки и рисунки: только номер, как в PDF. В заметке номера
// заголовков по умолчанию скрыты — ссылка на раздел показывает его название.
#let _ссылка(вид-док, it) = {
  let el = it.element
  if el == none { return it }
  if el.func() == heading {
    if вид-док == "заметка" { link(el.location(), el.body) }
    else { link(el.location(), numbering(el.numbering, ..counter(heading).at(el.location()))) }
  } else if el.func() == figure {
    let n = el.counter.at(el.location()).first()
    if вид-док == "заметка" { link(el.location(), str(n)) }
    else { link(el.location(), [#counter(heading).at(el.location()).first().#n]) }
  } else { it }
}

// Номер рисунка: в книге «глава.n», в заметке — сквозной.
#let _нумерация-рисунков(вид-док) = if вид-док == "заметка" { "1" } else {
  n => numbering("1.1", counter(heading).get().first(), n)
}

#let _веб-шаблон(т, вид-док, вид, название, подзаголовок, автор, дата, описание, теги, тело) = {
  // Шрифт, кегль и цвет в HTML-разметку не попадают (их задаёт CSS), но их
  // наследуют рисунки: html.frame верстается как страница PDF. Без этого
  // подписи на рисунках — чёрные (не видно в тёмной теме) и чужим шрифтом.
  set text(font: т.шрифт.текст, size: т.кегль.текст, lang: "ru", fill: т.цвет.текст)
  show math.equation: set text(font: т.шрифт.матем)
  show raw: set text(font: т.шрифт.код)
  // Номера заголовков выдаются всегда (span.k-num): показывать ли их, решает
  // настройка клиента, без перекомпиляции.
  set heading(numbering: "1.1")
  // Заголовок: <h2…h6>; <h1> занят названием. Класс k-hN — уровень
  // оформления: в книге `=` — глава (k-h1), в заметке `=` — раздел (k-h2).
  let сдвиг = if вид-док == "заметка" { 1 } else { 0 }
  show heading: it => {
    if it.level == 1 and вид-док == "книга" { _сброс() }
    let тег = "h" + str(calc.min(it.level + 1, 6))
    context {
      let номер = if it.numbering != none { counter(heading).display(it.numbering) }
      // data-num у главы — для крупной бледной цифры справа (CSS ::after).
      let глава = it.level == 1 and вид-док == "книга"
      let атр = if глава and номер != none { ("data-num": str(counter(heading).get().first())) } else { (:) }
      // Метка `= Раздел <метка>` — это id: по нему ведут ссылки #см(якорь: "метка").
      if it.has("label") { атр.insert("id", str(it.label)) }
      эл(тег, "k-h k-h" + str(calc.min(it.level + сдвиг, 4)), ..атр, {
        if номер != none { эл("span", "k-num", номер) }
        it.body
      })
    }
  }
  show ref: _ссылка.with(вид-док)

  set raw(theme: tm-тема(тема-кода(т)))

  set figure(numbering: _нумерация-рисунков(вид-док))
  show figure.caption: it => html.elem("figcaption", {
    эл("span", "k-fig-num")[#it.supplement #context it.counter.display(it.numbering).]
    [ ]
    it.body
  })

  // Страховка: в HTML-экспорте содержимое этих элементов пропадает целиком.
  // Выравнивание — классом, раскладку — SVG-кадром (текст в нём не
  // выделяется, но ничего не теряется). Своё в библиотеке их не использует.
  // Внутри кадра (html.frame) вёрстка снова постраничная — там не трогаем.
  show align: it => context if веб() {
    let г = it.alignment.x
    let к = if г == center { "k-center" } else if г == right or г == end { "k-right" } else { "k-left" }
    эл("div", к, it.body)
  } else { it }
  show grid: it => context if веб() { кадр(it, вид: "вёрстка") } else { it }
  show stack: it => context if веб() { кадр(it, вид: "вёрстка") } else { it }
  show place: it => context if веб() { кадр(it.body, вид: "вёрстка") } else { it }

  // Весь документ — в <article data-doc>: по виду документа (а не по месту
  // файла в хранилище) CSS решает, показывать ли номера и «Главу N».
  эл("article", "k-doc", ..("data-doc": вид-док), {
  if название != none {
    эл("header", "k-title", {
      if вид != none { эл("div", "k-title-kind", вид) }
      html.elem("h1", название)
      if подзаголовок != none { эл("p", "k-subtitle", подзаголовок) }
      if описание != none { эл("p", "k-description", описание) }
      if автор != none or дата != none {
        эл("p", "k-byline", [#автор#if автор != none and дата != none [ · ]#дата])
      }
      if теги.len() > 0 {
        эл("ul", "k-tags", теги.map(x => html.elem("li", x)).join())
      }
    })
  }
  тело
  })
}

// Общее для книги и заметки в PDF: ссылки, списки, код, таблицы, рисунки,
// титул. Заголовки уже настроены в _pdf-шаблон.
#let _pdf-тело(т, вид-док, вид, название, подзаголовок, автор, дата, описание, титул, оглавление, глубина, тело) = {
  let а = т.цвет.акцент

  // Ссылки: только номер, падежное слово пишется в тексте руками
  // («в разделе @sec-x»): typst ставит слово в именительном падеже.
  // То же для рисунков: пишем «на рис. @метка», получаем «на рис. 2.3».
  show ref: _ссылка.with(вид-док)
  show link: set text(fill: а)

  // Списки
  set list(marker: (text(fill: а, "•"), text(fill: а, "–"), text(fill: а, "·")), indent: 0.4em, body-indent: 0.55em)
  set enum(numbering: n => text(fill: а, weight: "bold", [#n.]), indent: 0.2em, body-indent: 0.5em)
  set terms(separator: [: ], hanging-indent: 1.2em)
  show terms.item: it => block(above: 0.7em, [#text(weight: "bold", fill: а, it.term): #it.description])

  // Код
  set raw(theme: tm-тема(т.цвет.код))
  show raw: set text(font: т.шрифт.код)
  show raw.where(block: false): it => box(
    fill: т.цвет.поверхность, radius: 2pt, inset: (x: 2.5pt), outset: (y: 2.5pt),
    text(size: 0.9em, it),
  )
  show raw.where(block: true): it => block(
    width: 100%, fill: т.цвет.поверхность, radius: 3pt, inset: (x: 9pt, y: 7pt),
    text(size: т.кегль.код, it),
  )

  // Таблицы
  // Линейки вместо сетки: жирная сверху и снизу, тонкая под шапкой.
  set table(
    inset: (x: 7pt, y: 5pt),
    align: left + horizon,
    stroke: (x, y) => if y == 0 {
      (bottom: 0.5pt + т.цвет.текст.transparentize(30%))
    } else {
      (bottom: 0.3pt + т.цвет.линия)
    },
  )
  show table: set text(size: 0.94em)
  show table: set par(justify: false, first-line-indent: 0em)
  show table.cell.where(y: 0): set text(font: т.шрифт.подписи, weight: "bold", fill: т.цвет.текст)
  // Короткие таблицы не рвём; длинную (больше полстраницы) оборачивай в
  // block(breakable: true) и повторяй шапку через table.header.
  show table: it => block(width: 100%, breakable: false,
    stroke: (top: 1pt + т.цвет.текст.transparentize(20%), bottom: 1pt + т.цвет.текст.transparentize(20%)), it)

  // Рисунки и подписи: нумерация «глава.n»
  set figure(numbering: _нумерация-рисунков(вид-док), gap: 0.7em)
  set figure.caption(separator: [. ])
  show figure: set block(above: 1.3em, below: 1.3em, breakable: false)
  show figure.caption: it => context {
    set text(font: т.шрифт.подписи, size: т.кегль.мелкий)
    set par(justify: false, first-line-indent: 0em)
    let номер = [#it.supplement #it.counter.display(it.numbering)]
    block(width: 90%, [#text(weight: "bold", fill: а, номер). #it.body])
  }

  // Заметка: название строкой сверху, без титульного листа.
  if вид-док == "заметка" and название != none {
    block(below: 1.4em, {
      text(font: т.шрифт.заголовки, size: т.кегль.глава, weight: "bold", fill: а, hyphenate: false, название)
      v(0.3em)
      line(length: 100%, stroke: 0.8pt + а)
    })
  }

  // Титул и оглавление
  if титул { _титул(т, вид, название, подзаголовок, автор, дата, описание) }
  if оглавление {
    page(header: none, footer: none, {
      block(below: 1.2em, text(font: т.шрифт.заголовки, size: 20pt, weight: "bold", fill: а)[Содержание])
      set par(leading: 0.55em, first-line-indent: 0em)
      show outline.entry.where(level: 1): it => block(above: 1em, text(weight: "bold", fill: а, it))
      outline(title: none, depth: глубина, indent: 1.3em)
    })
  }
  if титул or оглавление { counter(page).update(1) }

  тело
}

// ═════════════════════════════════════════════════════════════════════════
// PDF: страницы, колонтитулы, титул и оглавление
// ═════════════════════════════════════════════════════════════════════════
#let _pdf-шаблон(т, вид-док, вид, название, подзаголовок, автор, дата, описание, титул, оглавление, глубина, тело) = {
  let а = т.цвет.акцент
  set page(
    paper: "a4",
    margin: т.страница.поля,
    fill: т.цвет.фон,
    header: _верхний(т),
    footer: _нижний(т),
    header-ascent: 35%,
  )
  set text(font: т.шрифт.текст, size: т.кегль.текст, lang: "ru", fill: т.цвет.текст, hyphenate: auto)
  show math.equation: set text(font: т.шрифт.матем)
  set par(
    justify: т.абзац.выключка,
    leading: т.абзац.интерлиньяж,
    first-line-indent: т.абзац.отступ,
    spacing: т.абзац.интервал,
  )
  set block(spacing: т.абзац.интервал + 0.25em)

  // Заголовки. В книге `=` — глава с новой страницы, в заметке — раздел:
  // заметка короткая и без нумерации (как в Obsidian).
  if вид-док == "заметка" {
    show heading.where(level: 1): it => context _раздел(т, it)
    show heading.where(level: 2): it => context _подраздел(т, it)
    show heading: it => block(above: 1em, below: 0.5em, sticky: true, text(weight: "bold", it.body))
    _pdf-тело(т, вид-док, вид, название, подзаголовок, автор, дата, описание, титул, оглавление, глубина, тело)
  } else {
    set heading(numbering: "1.1")
    show heading.where(level: 1): it => {
      pagebreak(weak: true)
      _сброс()
      context _глава(т, it)
    }
    show heading.where(level: 2): it => context _раздел(т, it)
    show heading.where(level: 3): it => context _подраздел(т, it)
    show heading.where(level: 4): it => block(above: 1em, below: 0.5em, sticky: true, text(weight: "bold", it.body))
    _pdf-тело(т, вид-док, вид, название, подзаголовок, автор, дата, описание, титул, оглавление, глубина, тело)
  }
}


// ═════════════════════════════════════════════════════════════════════════
// Главные шаблоны
// ═════════════════════════════════════════════════════════════════════════
// Тема по умолчанию — из `--input тема=…` (приложение собирает заметку по
// разу на тему), иначе «классика». Имя темы, которой нет, — ошибка.
#let _тема-из-входа() = {
  let имя = sys.inputs.at("тема", default: "классика")
  assert(имя in темы, message: "нет темы «" + имя + "»; есть: " + темы.keys().join(", "))
  темы.at(имя)
}

#let _документ(вид-док, т, вид, название, подзаголовок, автор, дата, описание, теги, титул, оглавление, глубина, тело) = {
  let т = if т == auto { _тема-из-входа() } else { т }
  _тема.update(т)
  _вид.update(вид-док)

  set document(title: название, author: if автор == none { () } else if type(автор) == str { автор } else { () })
  show: it => context if веб() {
    _веб-шаблон(т, вид-док, вид, название, подзаголовок, автор, дата, описание, теги, it)
  } else {
    _pdf-шаблон(т, вид-док, вид, название, подзаголовок, автор, дата, описание, титул, оглавление, глубина, it)
  }
  тело
}

/// Книга — большой конспект из глав: `=` — глава с номером.
/// #show: конспект.with(
///   тема: auto,               // auto — из --input тема=…; или темы.ночь, своя
///   вид: [Конспект],          // надпись над названием: Задачник, Шпаргалка…
///   название: [...], подзаголовок: [...], автор: [...], дата: [...],
///   описание: [...],          // 2–3 фразы на титул: для кого и как читать
///   титул: true, оглавление: true, глубина: 2,   // только PDF
/// )
#let конспект(
  тема: auto,
  вид: [Конспект],
  название: none,
  подзаголовок: none,
  автор: none,
  дата: none,
  описание: none,
  теги: (),
  титул: true,
  оглавление: true,
  глубина: 2,
  тело,
) = _документ("книга", тема, вид, название, подзаголовок, автор, дата, описание, теги, титул, оглавление, глубина, тело)

/// Заметка — одна тема целиком: `=` — раздел, нумерация сквозная.
///   #import "/_konspekt/lib.typ": *
///   #show: заметка.with(название: [SSH], теги: ("безопасность",))
#let заметка(
  тема: auto,
  название: none,
  описание: none,
  теги: (),
  тело,
) = _документ("заметка", тема, none, название, none, none, none, описание, теги, false, false, 2, тело)
