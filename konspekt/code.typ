// Листинги: подсветка в цветах темы, номера строк, выделение строк,
// метки-выноски ①②③, на которые текст ссылается через #метка(n).
//
// Правило конспекта: листинг идёт ПОСЛЕ объяснения идеи и короче 25 строк.
// Длинный код — в файл рядом с главой (см. `код-из-файла`), в текст — только
// ключевой фрагмент.

#import "theme.typ": тема, фон-врезки
#import "web.typ": веб, эл

// ── Тема подсветки (tmTheme) генерируется из палитры ──────────────────────
#let _hex(c) = c.to-hex()

#let _правило(scope, цвет, стиль: "") = (
  "<dict><key>scope</key><string>" + scope + "</string><key>settings</key><dict>"
    + "<key>foreground</key><string>" + _hex(цвет) + "</string>"
    + (if стиль != "" { "<key>fontStyle</key><string>" + стиль + "</string>" } else { "" })
    + "</dict></dict>"
)

#let tm-тема(к) = bytes(
  "<?xml version=\"1.0\" encoding=\"UTF-8\"?>"
    + "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">"
    + "<plist version=\"1.0\"><dict><key>name</key><string>konspekt</string><key>settings</key><array>"
    + "<dict><key>settings</key><dict><key>foreground</key><string>" + _hex(к.текст) + "</string></dict></dict>"
    + _правило("comment, punctuation.definition.comment", к.коммент, стиль: "italic")
    + _правило("string, constant.character", к.строка)
    + _правило("constant.numeric, constant.language", к.число)
    + _правило("keyword, keyword.control, storage.modifier, keyword.other", к.ключ, стиль: "bold")
    + _правило("keyword.operator", к.текст)
    + _правило("storage.type, support.type, entity.name.type, entity.name.class", к.тип)
    + _правило("entity.name.function, support.function, variable.function", к.функция)
    + _правило("meta.preprocessor, keyword.control.import", к.коммент)
    + "</array></dict></plist>",
)

// ── Опорные цвета подсветки для HTML ──────────────────────────────────────
// В HTML Typst вшивает цвет подсветки в style="color: #…". Чтобы код
// перекрашивался вместе с темой, в HTML-режиме подсветка идёт этими
// условными цветами, а приложение заменяет их на CSS-переменные
// (--k-code-<ключ>). Значения не должны встречаться в настоящих темах.
#let опорные-цвета-кода = (
  текст: rgb("#010100"), ключ: rgb("#010101"), тип: rgb("#010102"),
  строка: rgb("#010103"), число: rgb("#010104"), коммент: rgb("#010105"),
  функция: rgb("#010106"), выделение: rgb("#010107"),
)

/// Тема подсветки для текущего режима: в HTML — опорные цвета.
#let тема-кода(т) = if веб() { опорные-цвета-кода } else { т.цвет.код }

// ── Метка-выноска ─────────────────────────────────────────────────────────
/// Кружок с номером. В листинге ставится параметром `метки`, в тексте —
/// `#метка(2)`, чтобы сослаться на строку.
#let метка(n) = context {
  let т = тема()
  if веб() { return эл("span", "k-mark", str(n)) }
  // Коробка высотой с x-высоту, кружок поверх неё: метка не раздвигает
  // интерлиньяж строки, в которой стоит.
  box(width: 1.05em, height: 0.5em, place(center + horizon, circle(
    radius: 0.5em, fill: т.цвет.акцент, stroke: none,
    align(center + horizon, text(
      font: т.шрифт.подписи, size: 0.62em, weight: "bold", fill: т.цвет.фон, str(n),
    )),
  )))
}

#let _имя-языка(яз) = (
  cpp: "C++", c: "C", py: "Python", python: "Python", sql: "SQL", rs: "Rust", rust: "Rust",
  js: "JS", ts: "TS", java: "Java", go: "Go", sh: "Shell", bash: "Bash", asm: "ASM",
  typ: "Typst", hs: "Haskell", kt: "Kotlin",
).at(яз, default: upper(яз))

// ── Листинг ───────────────────────────────────────────────────────────────
/// - код: строка или raw-блок (```cpp ... ```);
/// - язык: если код передан строкой;
/// - подпись: заголовок над листингом (имя функции, что делает);
/// - выделить: номера строк для подсветки фоном, например (4, 5);
/// - метки: словарь «номер строки → номер метки», например ("4": 1, "7": 2);
/// - номера: печатать номера строк;
/// - сложность: необязательная плашка под листингом, например [$O(n m)$ / $O(1)$].
#let листинг(
  код,
  язык: "cpp",
  подпись: none,
  выделить: (),
  метки: (:),
  номера: true,
  сложность: none,
) = context {
  let т = тема()
  let (текст-кода, яз) = if type(код) == str {
    (код, язык)
  } else {
    (код.text, if код.has("lang") and код.lang != none { код.lang } else { язык })
  }
  let фон = т.цвет.поверхность
  let всего = текст-кода.trim("\n", at: end).split("\n").len()
  let ширина-номера = if всего >= 10 { 1.6em } else { 1.1em }

  if веб() { return {
    show raw.line: it => {
      let м = метки.at(str(it.number), default: none)
      let класс = "k-line" + if it.number in выделить { " k-hl" } else { "" }
      эл("span", класс, { it.body; if м != none { метка(м) } })
    }
    set raw(theme: tm-тема(опорные-цвета-кода))
    эл("div", "k-listing" + if номера { " k-numbered" } else { "" }, {
      if подпись != none {
        эл("div", "k-listing-cap", [#эл("span", "k-lang", _имя-языка(яз)) #подпись])
      }
      raw(текст-кода.trim("\n", at: end), lang: яз, block: true)
      if сложность != none { эл("div", "k-listing-cx")[Сложность: #сложность] }
    })
  } }

  show raw.line: it => {
    let n = it.number
    let hl = n in выделить
    let м = метки.at(str(n), default: none)
    box(
      width: 100%,
      fill: if hl { т.цвет.код.выделение } else { none },
      outset: (x: 4pt, y: 2.9pt),
      grid(
        columns: if номера { (ширина-номера, 1fr, auto) } else { (1fr, auto) },
        column-gutter: 0.8em,
        ..if номера {
          (align(right, text(fill: т.цвет.приглушённый.transparentize(25%), size: 0.9em, str(n))),)
        },
        it.body,
        if м != none { метка(м) },
      ),
    )
  }
  set raw(theme: tm-тема(т.цвет.код))
  // Лигатуры выключены: «<=» не должно превращаться в «≤» — в листинге
  // читатель видит то, что надо набрать.
  set text(font: т.шрифт.код, size: т.кегль.код, ligatures: false, features: ("calt": 0))
  set par(justify: false, leading: 0.62em)

  // Короткий листинг (до 15 строк) не рвём между страницами: половина
  // функции внизу страницы хуже, чем пустое место.
  let рвать = всего > 15
  let тело = block(
    width: 100%,
    fill: фон,
    stroke: none,
    radius: 2pt,
    inset: (x: 9pt, y: 7pt),
    breakable: рвать,
    raw(текст-кода.trim("\n", at: end), lang: яз, block: true),
  )

  block(width: 100%, above: 0.9em, below: 0.9em, breakable: рвать, {
    if подпись != none {
      block(below: 0.45em, sticky: true, text(
        font: т.шрифт.подписи, size: т.кегль.мелкий, fill: т.цвет.приглушённый,
        [#text(fill: т.цвет.акцент, weight: "bold", _имя-языка(яз)) #h(0.4em) #подпись],
      ))
    }
    тело
    if сложность != none {
      block(above: 0.4em, text(font: т.шрифт.подписи, size: т.кегль.мелкий, fill: т.цвет.приглушённый)[
        Сложность: #сложность
      ])
    }
  })
}

/// Листинг из файла по региону:
///   // region: имя
///   ...
///   // endregion: имя
/// ВАЖНО: путь — от корня проекта (начинается с «/»), и сборка идёт с --root:
///   #код-из-файла("/code/prefix.cpp", регион: "build")
/// Так код лежит в настоящем .cpp, который компилируется и тестируется.
#let код-из-файла(путь, регион: none, ..аргументы) = {
  let весь = read(путь)
  let текст = if регион == none { весь } else {
    let начало = "// region: " + регион
    let конец = "// endregion: " + регион
    let строки = весь.split("\n")
    let i = строки.position(s => s.trim() == начало)
    assert(i != none, message: "регион «" + регион + "» не найден в " + путь)
    let j = строки.slice(i + 1).position(s => s.trim() == конец)
    assert(j != none, message: "нет конца региона «" + регион + "» в " + путь)
    строки.slice(i + 1, i + 1 + j).join("\n")
  }
  let яз = путь.split(".").last()
  листинг(текст, язык: яз, ..аргументы)
}
