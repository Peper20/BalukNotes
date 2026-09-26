// Смысловые блоки конспекта: определения, теоремы, примеры, замечания и т.д.
//
// Словарь врезок сознательно маленький. Если хочется новый тип — сначала
// проверь, не подходит ли существующий: пять цветов на странице — это шум,
// а не структура (главная ошибка конспекта по БД).

#import "theme.typ": current-theme, _doc-kind, box-bg, is-dark
#import "web.typ": is-web, elem

#let _box-titles = (
  definition: "Определение",
  theorem: "Теорема",
  example: "Пример",
  remark: "Замечание",
  pitfall: "Типичная ошибка",
  idea: "Идея",
  algorithm: "Алгоритм",
)

// Нумеруются только те врезки, на которые ссылаются: определения и теоремы
// (общий счётчик), примеры (свой). Номер — «глава.n», сброс в начале главы.
#let _counter(kind) = if kind in ("definition", "theorem") { counter("k-stmt") } else if kind == "example" { counter("k-example") } else { none }
#let _steps = counter("k-step")

// Классы врезок в HTML: цвет каждой — CSS-переменная темы.
#let _box-classes = (
  definition: "def", theorem: "thm", example: "example", remark: "remark",
  pitfall: "pitfall", idea: "idea", algorithm: "algo",
)

#let _number(kind) = {
  let ctr = _counter(kind)
  if ctr == none { return none }
  if _doc-kind.get() == "note" { return str(ctr.get().first()) }
  let ch = counter(heading).get().first()
  [#ch.#ctr.get().first()]
}

// ── Подпись врезки ────────────────────────────────────────────────────────
// Капитель «вручную»: у шрифтов может не быть кириллических капителей, и
// smallcaps() молча печатает строчные. Прописные поменьше + разрядка.
#let small-caps(s) = context if is-web() { elem("span", "k-caps", upper(s)) } else { text(size: 0.8em, tracking: 0.07em, upper(s)) }

#let _box-label(kind, title, number, color) = {
  let name = _box-titles.at(kind)
  text(weight: "bold", fill: color)[#name#if number != none [ #number]#if title != none [ (#title)].]
  [ ]
}

// ── Общий каркас ──────────────────────────────────────────────────────────
#let _box(kind, title, body) = {
  let ctr = _counter(kind)
  if ctr != none { ctr.step() }
  if kind == "example" { _steps.update(0) }
  context {
    let theme = current-theme()
    let color = theme.color.boxes.at(kind)
    let number = _number(kind)
    // Определения и теоремы короткие — их не рвём между страницами.
    let can-break = kind not in ("definition", "theorem")

    if is-web() {
      let head = [#_box-titles.at(kind)#if number != none [ #number]#if title != none [ (#title)].]
      return elem("div", "k-box k-" + _box-classes.at(kind), [#elem("span", "k-label", head) #body])
    }
    block(
      width: 100%, above: 1.1em, below: 1.1em, breakable: can-break,
      fill: box-bg(theme, color),
      stroke: (left: 2.5pt + color),
      inset: (x: 10pt, y: 9pt),
      radius: (right: 2pt),
    )[#_box-label(kind, title, number, color)#body]
  }
}

// ── Публичные врезки ──────────────────────────────────────────────────────
#let definition(title: none, body) = _box("definition", title, body)
#let theorem(title: none, body) = _box("theorem", title, body)
#let example(title: none, body) = _box("example", title, body)
#let remark(title: none, body) = _box("remark", title, body)
#let pitfall(title: none, body) = _box("pitfall", title, body)
#let idea(title: none, body) = _box("idea", title, body)
#let algorithm(title: none, body) = _box("algorithm", title, body)

/// Шаг решения внутри примера: «Шаг 1. Рисунок.» Нумерация — своя
/// в каждом примере.
#let step(name) = {
  _steps.step()
  context {
    let theme = current-theme()
    if is-web() { return [#elem("span", "k-step")[Шаг #_steps.get().first(). #name.] ] }
    text(weight: "bold", fill: theme.color.boxes.example.darken(if is-dark(theme) { 0% } else { 10% }))[Шаг #_steps.get().first(). #name.]
    [ ]
  }
}

/// Ответ в рамке, прижат вправо.
#let answer(body) = context {
  let theme = current-theme()
  if is-web() { return elem("div", "k-answer", [#elem("span", "k-answer-label")[Ответ:] #body]) }
  align(right, box(
    stroke: 0.7pt + theme.color.boxes.example, radius: 3pt, inset: (x: 8pt, y: 5pt),
    fill: box-bg(theme, theme.color.boxes.example),
    [#text(weight: "bold", fill: theme.color.boxes.example)[Ответ:] #body],
  ))
}

/// Эскиз доказательства: мельче, с тонкой линией, заканчивается ∎.
#let proof(body) = context {
  let theme = current-theme()
  if is-web() {
    return elem("div", "k-proof", [#elem("span", "k-proof-head")[Почему это верно.] #body #elem("span", "k-qed")[∎]])
  }
  block(
    width: 100%, above: 0.8em, below: 1em, breakable: true,
    stroke: (left: 0.6pt + theme.color.line), inset: (left: 10pt, y: 2pt),
  )[
    #set text(size: 0.94em)
    #text(style: "italic", fill: theme.color.muted)[Почему это верно.] #body #h(1fr) #text(fill: theme.color.muted)[∎]
  ]
}

// ── Ключевая формула ──────────────────────────────────────────────────────
/// То, что надо запомнить. Одна-две на раздел, не больше, иначе теряет смысл.
#let formula(label: none, body) = context {
  let theme = current-theme()
  if is-web() {
    return elem("div", "k-formula", { if label != none { elem("div", "k-formula-label", label) }; body })
  }
  let acc = theme.color.accent
  let display-text = if label != none {
    text(font: theme.font.captions, size: 0.72em, weight: "bold", tracking: 0.1em, fill: acc, upper(label))
  }
  block(width: 100%, above: 1.1em, below: 1.1em, breakable: false,
    fill: box-bg(theme, acc), radius: 3pt, inset: (x: 10pt, y: 10pt),
    stroke: 0.7pt + acc,
    { if display-text != none { block(below: 3pt, display-text) }; align(center, body) })
}

// ── Заметка сбоку ─────────────────────────────────────────────────────────
/// Мелкая заметка сбоку от мысли: то, что полезно знать, но что прервало бы
/// рассказ. Не чаще одной на полстраницы.
#let margin-note(body) = context {
  let theme = current-theme()
  if is-web() { return elem("aside", "k-aside", body) }
  block(width: 100%, above: 0.8em, below: 0.8em, inset: (left: 10pt, y: 1pt),
    stroke: (left: 0.5pt + theme.color.line), {
      set text(font: theme.font.text, size: theme.size.small, fill: theme.color.muted, style: "normal")
      set par(justify: false, first-line-indent: 0em, leading: 0.55em)
      body
    })
}

/// Текст и небольшой рисунок рядом, в две колонки.
///   #side-by-side[абзац][#fig(canvas(...), [подпись])]
#let side-by-side(main, side, width: 40%, gap: 1.2em) = context if is-web() {
  elem("div", "k-side", style: "--k-side-w: " + repr(width), {
    elem("div", "k-side-text", main)
    elem("div", "k-side-fig", side)
  })
} else {
  grid(columns: (1fr, width), column-gutter: gap, align: (top, horizon), main, side)
}

// ── Вводная часть главы ───────────────────────────────────────────────────
/// Лид: 2–4 предложения — зачем эта глава и на что опирается.
#let lead(body) = context {
  let theme = current-theme()
  if is-web() { return elem("div", "k-lead", body) }
  block(width: 100%, above: 0.4em, below: 1.2em, {
    set text(size: 1.07em, fill: theme.color.text.transparentize(if is-dark(theme) { 5% } else { 10% }))
    set par(first-line-indent: 0em)
    body
  })
}

/// «В этой главе» («В этой заметке»): что читатель научится делать (глаголами).
#let plan(..items) = context {
  let theme = current-theme()
  let acc = theme.color.accent
  let items = items.pos()
  let display-text = if _doc-kind.get() == "note" [В этой заметке] else [В этой главе]
  if is-web() {
    return elem("div", "k-plan", {
      elem("div", "k-plan-head", display-text)
      elem("ul", "k-plan-list", items.map(x => html.elem("li", x)).join())
    })
  }
  let title = text(font: theme.font.captions, size: 0.74em, weight: "bold", tracking: 0.12em, fill: acc, upper(display-text))
  let rows = items.map(x => [#text(fill: acc)[→] #h(0.3em) #x])
  let body = { block(below: 6pt, title); set par(first-line-indent: 0em); stack(spacing: 0.55em, ..rows) }
  block(width: 100%, above: 0.8em, below: 1.4em,
    stroke: (top: 0.5pt + theme.color.line, bottom: 0.5pt + theme.color.line), inset: (y: 9pt), body)
}

// ── Итог главы, самопроверка, ошибки ──────────────────────────────────────
/// Итог: 3–6 пунктов, которые должны остаться в голове после главы.
#let summary(..items) = context {
  let theme = current-theme()
  let acc = theme.color.accent
  let items = items.pos()
  if is-web() {
    return elem("div", "k-summary", {
      elem("div", "k-summary-head")[Коротко о главном]
      elem("ol", "k-summary-list", items.map(x => html.elem("li", x)).join())
    })
  }
  let title = text(font: theme.font.headings, weight: "bold", fill: acc, size: 1.05em)[Коротко о главном]
  let entries = {
    set par(first-line-indent: 0em)
    grid(columns: (auto, 1fr), column-gutter: 0.7em, row-gutter: 0.75em,
      ..items.enumerate().map(((i, x)) => (text(font: theme.font.captions, weight: "bold", fill: acc, str(i + 1)), x)).flatten())
  }
  set block(breakable: false)
  block(width: 100%, above: 1.4em, below: 1.2em, fill: theme.color.surface,
    stroke: (top: 1.5pt + acc), inset: 12pt, { block(below: 8pt, title); entries })
}

/// Вопросы для самопроверки: пары (вопрос, ответ). Ответы печатаются
/// отдельно внизу блока мелко — чтобы сначала подумать.
#let quiz(..pairs) = context {
  let theme = current-theme()
  let acc = theme.color.boxes.idea
  let items = pairs.pos()
  // В HTML ответы спрятаны в <details>: сначала подумать, потом раскрыть.
  if is-web() {
    return elem("div", "k-quiz", {
      elem("div", "k-quiz-head")[Проверь себя]
      elem("ol", "k-quiz-list", items.map(((q, _)) => html.elem("li", q)).join())
      html.elem("details", attrs: (class: "k-quiz-answers"), {
        html.elem("summary")[Ответы]
        elem("ol", "k-quiz-list", items.map(((_, ans)) => html.elem("li", ans)).join())
      })
    })
  }
  block(width: 100%, above: 1.2em, below: 1.2em, breakable: false,
    stroke: 0.8pt + acc.lighten(if is-dark(theme) { 0% } else { 50% }).transparentize(if is-dark(theme) { 55% } else { 0% }),
    radius: 6pt,
    inset: (x: 12pt, y: 10pt),
    {
      set par(first-line-indent: 0em)
      block(below: 8pt, text(font: theme.font.captions, weight: "bold", size: 0.8em, tracking: 0.1em, fill: acc, upper[Проверь себя]))
      for (i, (q, _)) in items.enumerate() {
        block(below: 0.6em, grid(columns: (1.3em, 1fr), text(weight: "bold", fill: acc)[#(i + 1).], q))
      }
      line(length: 100%, stroke: (paint: theme.color.line, dash: "dashed", thickness: 0.5pt))
      set text(size: theme.size.small, fill: theme.color.muted)
      [*Ответы.* ]
      items.enumerate().map(((i, (_, ans))) => [#(i + 1)) #ans]).join([ #h(0.4em) ])
    })
}

/// Таблица с шапкой и подсветкой клеток — для трассировок алгоритмов и
/// сравнений. Клетки идут подряд, как у обычного #table.
/// - highlight: словарь «"строка,столбец" → цвет»; строки и столбцы с 1,
///   строка 0 — шапка. Можно "2,*" (вся строка) и "*,3" (весь столбец).
///   Цвета — имена темы ("line", "second", "third", "accent") или color.
///   #data-table((auto, 1fr, auto), header: ([шаг], [l], [r]),
///     highlight: ("3,*": "third"), [1], [0], [6], ...)
#let data-table(cols, ..cells, header: none, highlight: (:), cell-align: auto) = context {
  let theme = current-theme()
  let n = cols.len()
  let hl-color(r, c) = {
    let col = highlight.at(str(r) + "," + str(c), default:
      highlight.at(str(r) + ",*", default:
        highlight.at("*," + str(c), default: none)))
    if col == none { return none }
    let rgbc = if type(col) == color { col } else if col in theme.color.fig { theme.color.fig.at(col) } else if col in theme.color.boxes { theme.color.boxes.at(col) } else { theme.color.accent }
    if is-dark(theme) { rgbc.transparentize(80%) } else { rgbc.lighten(85%) }
  }
  let entries = cells.pos()
  if is-web() {
    let cls(r, c) = {
      let col = highlight.at(str(r) + "," + str(c), default:
        highlight.at(str(r) + ",*", default: highlight.at("*," + str(c), default: none)))
      if col == none { none } else if type(col) == color { "k-hl" } else { "k-hl k-hl-" + col }
    }
    let rows = entries.chunks(n)
    return elem("table", "k-table", {
      if header != none {
        html.elem("thead", html.elem("tr", header.map(x => html.elem("th", x)).join()))
      }
      html.elem("tbody", rows.enumerate().map(((i, series)) => html.elem("tr",
        series.enumerate().map(((j, item)) => {
          let kk = cls(i + 1, j + 1)
          if kk == none { html.elem("td", item) } else { html.elem("td", attrs: (class: kk), item) }
        }).join(),
      )).join())
    })
  }
  let hl-cells = entries.enumerate().map(((i, item)) => {
    let (r, c) = (calc.div-euclid(i, n) + 1, calc.rem(i, n) + 1)
    let bg = hl-color(r, c)
    if bg == none { item } else { table.cell(fill: bg, item) }
  })
  table(
    columns: cols,
    align: if cell-align == auto { left + horizon } else { cell-align },
    ..if header != none { (table.header(..header),) },
    ..hl-cells,
  )
}

/// Таблица типичных ошибок: пары (ошибка, как избежать).
#let pitfalls(..pairs) = context {
  let theme = current-theme()
  let bad-color = theme.color.boxes.pitfall
  let good-color = theme.color.boxes.example
  let items = pairs.pos()
  if is-web() {
    return elem("table", "k-table k-pitfalls", {
      html.elem("thead", html.elem("tr", {
        elem("th", "k-bad")[✗ Ошибка]
        elem("th", "k-good")[✓ Как избежать]
      }))
      html.elem("tbody", items.map(((err, fix)) => html.elem("tr", html.elem("td", err) + html.elem("td", fix))).join())
    })
  }
  block(width: 100%, above: 1em, below: 1.2em, table(
    columns: (1fr, 1fr),
    table.header(
      text(fill: bad-color, weight: "bold")[✗ Ошибка],
      text(fill: good-color, weight: "bold")[✓ Как избежать],
    ),
    ..items.flatten(),
  ))
}
