// Смысловые блоки конспекта: определения, теоремы, примеры, замечания и т.д.
//
// Словарь врезок сознательно маленький. Если хочется новый тип — сначала
// проверь, не подходит ли существующий: пять цветов на странице — это шум,
// а не структура (главная ошибка конспекта по БД).

#import "theme.typ": тема, фон-врезки, тёмная
#import "web.typ": веб, эл

#let _подписи = (
  опр: "Определение",
  теорема: "Теорема",
  пример: "Пример",
  замечание: "Замечание",
  ошибка: "Типичная ошибка",
  идея: "Идея",
  алгоритм: "Алгоритм",
)

// Нумеруются только те врезки, на которые ссылаются: определения и теоремы
// (общий счётчик), примеры (свой). Номер — «глава.n», сброс в начале главы.
#let _счётчик(вид) = if вид in ("опр", "теорема") { counter("k-утв") } else if вид == "пример" { counter("k-пример") } else { none }
#let _шаги = counter("k-шаг")

// Классы врезок в HTML: цвет каждой — CSS-переменная темы.
#let _классы = (
  опр: "def", теорема: "thm", пример: "example", замечание: "remark",
  ошибка: "pitfall", идея: "idea", алгоритм: "algo",
)

#let _номер(вид) = {
  let с = _счётчик(вид)
  if с == none { return none }
  let гл = counter(heading).get().first()
  [#гл.#с.get().first()]
}

// ── Подпись врезки ────────────────────────────────────────────────────────
// Капитель «вручную»: у шрифтов может не быть кириллических капителей, и
// smallcaps() молча печатает строчные. Прописные поменьше + разрядка.
#let капитель(s) = context if веб() { эл("span", "k-caps", upper(s)) } else { text(size: 0.8em, tracking: 0.07em, upper(s)) }

#let _ярлык(вид, заголовок, номер, цвет) = {
  let имя = _подписи.at(вид)
  text(weight: "bold", fill: цвет)[#имя#if номер != none [ #номер]#if заголовок != none [ (#заголовок)].]
  [ ]
}

// ── Общий каркас ──────────────────────────────────────────────────────────
#let _врезка(вид, заголовок, тело) = {
  let с = _счётчик(вид)
  if с != none { с.step() }
  if вид == "пример" { _шаги.update(0) }
  context {
    let т = тема()
    let цвет = т.цвет.врезки.at(вид)
    let номер = _номер(вид)
    // Определения и теоремы короткие — их не рвём между страницами.
    let рвать = вид not in ("опр", "теорема")

    if веб() {
      let ярлык = [#_подписи.at(вид)#if номер != none [ #номер]#if заголовок != none [ (#заголовок)].]
      return эл("div", "k-box k-" + _классы.at(вид), [#эл("span", "k-label", ярлык) #тело])
    }
    block(
      width: 100%, above: 1.1em, below: 1.1em, breakable: рвать,
      fill: фон-врезки(т, цвет),
      stroke: (left: 2.5pt + цвет),
      inset: (x: 10pt, y: 9pt),
      radius: (right: 2pt),
    )[#_ярлык(вид, заголовок, номер, цвет)#тело]
  }
}

// ── Публичные врезки ──────────────────────────────────────────────────────
#let опр(заголовок: none, тело) = _врезка("опр", заголовок, тело)
#let теорема(заголовок: none, тело) = _врезка("теорема", заголовок, тело)
#let пример(заголовок: none, тело) = _врезка("пример", заголовок, тело)
#let замечание(заголовок: none, тело) = _врезка("замечание", заголовок, тело)
#let ошибка(заголовок: none, тело) = _врезка("ошибка", заголовок, тело)
#let идея(заголовок: none, тело) = _врезка("идея", заголовок, тело)
#let алгоритм(заголовок: none, тело) = _врезка("алгоритм", заголовок, тело)

/// Шаг решения внутри примера: «Шаг 1. Рисунок.» Нумерация — своя
/// в каждом примере.
#let шаг(название) = {
  _шаги.step()
  context {
    let т = тема()
    if веб() { return [#эл("span", "k-step")[Шаг #_шаги.get().first(). #название.] ] }
    text(weight: "bold", fill: т.цвет.врезки.пример.darken(if тёмная(т) { 0% } else { 10% }))[Шаг #_шаги.get().first(). #название.]
    [ ]
  }
}

/// Ответ в рамке, прижат вправо.
#let ответ(тело) = context {
  let т = тема()
  if веб() { return эл("div", "k-answer", [#эл("span", "k-answer-label")[Ответ:] #тело]) }
  align(right, box(
    stroke: 0.7pt + т.цвет.врезки.пример, radius: 3pt, inset: (x: 8pt, y: 5pt),
    fill: фон-врезки(т, т.цвет.врезки.пример),
    [#text(weight: "bold", fill: т.цвет.врезки.пример)[Ответ:] #тело],
  ))
}

/// Эскиз доказательства: мельче, с тонкой линией, заканчивается ∎.
#let доказательство(тело) = context {
  let т = тема()
  if веб() {
    return эл("div", "k-proof", [#эл("span", "k-proof-head")[Почему это верно.] #тело #эл("span", "k-qed")[∎]])
  }
  block(
    width: 100%, above: 0.8em, below: 1em, breakable: true,
    stroke: (left: 0.6pt + т.цвет.линия), inset: (left: 10pt, y: 2pt),
  )[
    #set text(size: 0.94em)
    #text(style: "italic", fill: т.цвет.приглушённый)[Почему это верно.] #тело #h(1fr) #text(fill: т.цвет.приглушённый)[∎]
  ]
}

// ── Ключевая формула ──────────────────────────────────────────────────────
/// То, что надо запомнить. Одна-две на раздел, не больше, иначе теряет смысл.
#let формула(подпись: none, тело) = context {
  let т = тема()
  if веб() {
    return эл("div", "k-formula", { if подпись != none { эл("div", "k-formula-label", подпись) }; тело })
  }
  let а = т.цвет.акцент
  let надпись = if подпись != none {
    text(font: т.шрифт.подписи, size: 0.72em, weight: "bold", tracking: 0.1em, fill: а, upper(подпись))
  }
  block(width: 100%, above: 1.1em, below: 1.1em, breakable: false,
    fill: фон-врезки(т, а), radius: 3pt, inset: (x: 10pt, y: 10pt),
    stroke: 0.7pt + а,
    { if надпись != none { block(below: 3pt, надпись) }; align(center, тело) })
}

// ── Заметка сбоку ─────────────────────────────────────────────────────────
/// Мелкая заметка сбоку от мысли: то, что полезно знать, но что прервало бы
/// рассказ. Не чаще одной на полстраницы.
#let на-полях(тело) = context {
  let т = тема()
  if веб() { return эл("aside", "k-aside", тело) }
  block(width: 100%, above: 0.8em, below: 0.8em, inset: (left: 10pt, y: 1pt),
    stroke: (left: 0.5pt + т.цвет.линия), {
      set text(font: т.шрифт.текст, size: т.кегль.мелкий, fill: т.цвет.приглушённый, style: "normal")
      set par(justify: false, first-line-indent: 0em, leading: 0.55em)
      тело
    })
}

/// Текст и небольшой рисунок рядом, в две колонки.
///   #рядом[абзац][#рис(холст(...), [подпись])]
#let рядом(текст, рисунок, ширина: 40%, зазор: 1.2em) = context if веб() {
  эл("div", "k-side", style: "--k-side-w: " + repr(ширина), {
    эл("div", "k-side-text", текст)
    эл("div", "k-side-fig", рисунок)
  })
} else {
  grid(columns: (1fr, ширина), column-gutter: зазор, align: (top, horizon), текст, рисунок)
}

// ── Вводная часть главы ───────────────────────────────────────────────────
/// Лид: 2–4 предложения — зачем эта глава и на что опирается.
#let лид(тело) = context {
  let т = тема()
  if веб() { return эл("div", "k-lead", тело) }
  block(width: 100%, above: 0.4em, below: 1.2em, {
    set text(size: 1.07em, fill: т.цвет.текст.transparentize(if тёмная(т) { 5% } else { 10% }))
    set par(first-line-indent: 0em)
    тело
  })
}

/// «В этой главе»: что читатель научится делать (глаголами).
#let план(..пункты) = context {
  let т = тема()
  let а = т.цвет.акцент
  let п = пункты.pos()
  if веб() {
    return эл("div", "k-plan", {
      эл("div", "k-plan-head")[В этой главе]
      эл("ul", "k-plan-list", п.map(x => html.elem("li", x)).join())
    })
  }
  let заголовок = text(font: т.шрифт.подписи, size: 0.74em, weight: "bold", tracking: 0.12em, fill: а, upper[В этой главе])
  let строки = п.map(x => [#text(fill: а)[→] #h(0.3em) #x])
  let тело = { block(below: 6pt, заголовок); set par(first-line-indent: 0em); stack(spacing: 0.55em, ..строки) }
  block(width: 100%, above: 0.8em, below: 1.4em,
    stroke: (top: 0.5pt + т.цвет.линия, bottom: 0.5pt + т.цвет.линия), inset: (y: 9pt), тело)
}

// ── Итог главы, самопроверка, ошибки ──────────────────────────────────────
/// Итог: 3–6 пунктов, которые должны остаться в голове после главы.
#let итог(..пункты) = context {
  let т = тема()
  let а = т.цвет.акцент
  let п = пункты.pos()
  if веб() {
    return эл("div", "k-summary", {
      эл("div", "k-summary-head")[Коротко о главном]
      эл("ol", "k-summary-list", п.map(x => html.elem("li", x)).join())
    })
  }
  let заголовок = text(font: т.шрифт.заголовки, weight: "bold", fill: а, size: 1.05em)[Коротко о главном]
  let список = {
    set par(first-line-indent: 0em)
    grid(columns: (auto, 1fr), column-gutter: 0.7em, row-gutter: 0.75em,
      ..п.enumerate().map(((i, x)) => (text(font: т.шрифт.подписи, weight: "bold", fill: а, str(i + 1)), x)).flatten())
  }
  set block(breakable: false)
  block(width: 100%, above: 1.4em, below: 1.2em, fill: т.цвет.поверхность,
    stroke: (top: 1.5pt + а), inset: 12pt, { block(below: 8pt, заголовок); список })
}

/// Вопросы для самопроверки: пары (вопрос, ответ). Ответы печатаются
/// отдельно внизу блока мелко — чтобы сначала подумать.
#let вопросы(..пары) = context {
  let т = тема()
  let а = т.цвет.врезки.идея
  let п = пары.pos()
  // В HTML ответы спрятаны в <details>: сначала подумать, потом раскрыть.
  if веб() {
    return эл("div", "k-quiz", {
      эл("div", "k-quiz-head")[Проверь себя]
      эл("ol", "k-quiz-list", п.map(((в, _)) => html.elem("li", в)).join())
      html.elem("details", attrs: (class: "k-quiz-answers"), {
        html.elem("summary")[Ответы]
        эл("ol", "k-quiz-list", п.map(((_, о)) => html.elem("li", о)).join())
      })
    })
  }
  block(width: 100%, above: 1.2em, below: 1.2em, breakable: false,
    stroke: 0.8pt + а.lighten(if тёмная(т) { 0% } else { 50% }).transparentize(if тёмная(т) { 55% } else { 0% }),
    radius: 6pt,
    inset: (x: 12pt, y: 10pt),
    {
      set par(first-line-indent: 0em)
      block(below: 8pt, text(font: т.шрифт.подписи, weight: "bold", size: 0.8em, tracking: 0.1em, fill: а, upper[Проверь себя]))
      for (i, (в, _)) in п.enumerate() {
        block(below: 0.6em, grid(columns: (1.3em, 1fr), text(weight: "bold", fill: а)[#(i + 1).], в))
      }
      line(length: 100%, stroke: (paint: т.цвет.линия, dash: "dashed", thickness: 0.5pt))
      set text(size: т.кегль.мелкий, fill: т.цвет.приглушённый)
      [*Ответы.* ]
      п.enumerate().map(((i, (_, о))) => [#(i + 1)) #о]).join([ #h(0.4em) ])
    })
}

/// Таблица с шапкой и подсветкой клеток — для трассировок алгоритмов и
/// сравнений. Клетки идут подряд, как у обычного #table.
/// - выделить: словарь «"строка,столбец" → цвет»; строки и столбцы с 1,
///   строка 0 — шапка. Можно "2,*" (вся строка) и "*,3" (весь столбец).
///   Цвета — имена темы ("линия", "второй", "третий", "акцент") или color.
///   #таблица((auto, 1fr, auto), шапка: ([шаг], [l], [r]),
///     выделить: ("3,*": "третий"), [1], [0], [6], ...)
#let таблица(колонки, ..ячейки, шапка: none, выделить: (:), выравнивание: auto) = context {
  let т = тема()
  let n = колонки.len()
  let подсветка(r, c) = {
    let ц = выделить.at(str(r) + "," + str(c), default:
      выделить.at(str(r) + ",*", default:
        выделить.at("*," + str(c), default: none)))
    if ц == none { return none }
    let цв = if type(ц) == color { ц } else if ц in т.цвет.рис { т.цвет.рис.at(ц) } else if ц in т.цвет.врезки { т.цвет.врезки.at(ц) } else { т.цвет.акцент }
    if тёмная(т) { цв.transparentize(80%) } else { цв.lighten(85%) }
  }
  let список = ячейки.pos()
  if веб() {
    let класс(r, c) = {
      let ц = выделить.at(str(r) + "," + str(c), default:
        выделить.at(str(r) + ",*", default: выделить.at("*," + str(c), default: none)))
      if ц == none { none } else if type(ц) == color { "k-hl" } else { "k-hl k-hl-" + ц }
    }
    let строки = список.chunks(n)
    return эл("table", "k-table", {
      if шапка != none {
        html.elem("thead", html.elem("tr", шапка.map(x => html.elem("th", x)).join()))
      }
      html.elem("tbody", строки.enumerate().map(((i, ряд)) => html.elem("tr",
        ряд.enumerate().map(((j, я)) => {
          let к = класс(i + 1, j + 1)
          if к == none { html.elem("td", я) } else { html.elem("td", attrs: (class: к), я) }
        }).join(),
      )).join())
    })
  }
  let с-подсветкой = список.enumerate().map(((i, я)) => {
    let (r, c) = (calc.div-euclid(i, n) + 1, calc.rem(i, n) + 1)
    let ф = подсветка(r, c)
    if ф == none { я } else { table.cell(fill: ф, я) }
  })
  table(
    columns: колонки,
    align: if выравнивание == auto { left + horizon } else { выравнивание },
    ..if шапка != none { (table.header(..шапка),) },
    ..с-подсветкой,
  )
}

/// Таблица типичных ошибок: пары (ошибка, как избежать).
#let ошибки(..пары) = context {
  let т = тема()
  let кр = т.цвет.врезки.ошибка
  let зл = т.цвет.врезки.пример
  let п = пары.pos()
  if веб() {
    return эл("table", "k-table k-pitfalls", {
      html.elem("thead", html.elem("tr", {
        эл("th", "k-bad")[✗ Ошибка]
        эл("th", "k-good")[✓ Как избежать]
      }))
      html.elem("tbody", п.map(((о, к)) => html.elem("tr", html.elem("td", о) + html.elem("td", к))).join())
    })
  }
  block(width: 100%, above: 1em, below: 1.2em, table(
    columns: (1fr, 1fr),
    table.header(
      text(fill: кр, weight: "bold")[✗ Ошибка],
      text(fill: зл, weight: "bold")[✓ Как избежать],
    ),
    ..п.flatten(),
  ))
}
