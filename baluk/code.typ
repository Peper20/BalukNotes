// Листинги: подсветка в цветах темы, номера строк, выделение строк,
// метки-выноски ①②③, на которые текст ссылается через #callout(n).
//
// Правило конспекта: листинг идёт ПОСЛЕ объяснения идеи и короче 25 строк.
// Длинный код — в файл рядом с главой (см. `code-from-file`), в текст — только
// ключевой фрагмент.

#import "theme.typ": current-theme, box-bg
#import "web.typ": is-web, elem

// ── Тема подсветки (tmTheme) генерируется из палитры ──────────────────────
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

// ── Опорные цвета подсветки для HTML ──────────────────────────────────────
// В HTML Typst вшивает цвет подсветки в style="color: #…". Чтобы код
// перекрашивался вместе с темой, в HTML-режиме подсветка идёт этими
// условными цветами, а приложение заменяет их на CSS-переменные
// (--k-code-<ключ>). Значения не должны встречаться в настоящих темах.
#let code-ref-colors = (
  text: rgb("#010100"), keyword: rgb("#010101"), type: rgb("#010102"),
  string: rgb("#010103"), number: rgb("#010104"), comment: rgb("#010105"),
  function: rgb("#010106"), highlight: rgb("#010107"),
)

/// Тема подсветки для текущего режима: в HTML — опорные цвета.
#let code-theme(theme) = if is-web() { code-ref-colors } else { theme.color.code }

// ── Метка-выноска ─────────────────────────────────────────────────────────
/// Кружок с номером. В листинге ставится параметром `callouts`, в тексте —
/// `#callout(2)`, чтобы сослаться на строку.
#let callout(n) = context {
  let theme = current-theme()
  if is-web() { return elem("span", "k-mark", str(n)) }
  // Коробка высотой с x-высоту, кружок поверх неё: метка не раздвигает
  // интерлиньяж строки, в которой стоит.
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

// ── Листинг ───────────────────────────────────────────────────────────────
/// - src: строка или raw-блок (```cpp ... ```);
/// - lang: если код передан строкой;
/// - caption: заголовок над листингом (имя функции, что делает);
/// - highlight: номера строк для подсветки фоном, например (4, 5);
/// - callouts: словарь «номер строки → номер метки», например ("4": 1, "7": 2);
/// - line-numbers: печатать номера строк;
/// - complexity: необязательная плашка под листингом, например [$O(n m)$ / $O(1)$].
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
      if complexity != none { elem("div", "k-listing-cx")[Сложность: #complexity] }
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
  // Лигатуры выключены: «<=» не должно превращаться в «≤» — в листинге
  // читатель видит то, что надо набрать.
  set text(font: theme.font.code, size: theme.size.code, ligatures: false, features: ("calt": 0))
  set par(justify: false, leading: 0.62em)

  // Короткий листинг (до 15 строк) не рвём между страницами: половина
  // функции внизу страницы хуже, чем пустое место.
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
        Сложность: #complexity
      ])
    }
  })
}

/// Листинг из файла по региону:
///   // region: имя
///   ...
///   // endregion: имя
/// ВАЖНО: путь — от корня проекта (начинается с «/»), и сборка идёт с --root:
///   #code-from-file("/code/prefix.cpp", region: "build")
/// Так код лежит в настоящем .cpp, который компилируется и тестируется.
#let code-from-file(file-path, region: none, ..rest) = {
  let whole = read(file-path)
  let txt = if region == none { whole } else {
    let begin-marker = "// region: " + region
    let end-marker = "// endregion: " + region
    let lines = whole.split("\n")
    let i = lines.position(s => s.trim() == begin-marker)
    assert(i != none, message: "регион «" + region + "» не найден в " + file-path)
    let j = lines.slice(i + 1).position(s => s.trim() == end-marker)
    assert(j != none, message: "нет конца региона «" + region + "» в " + file-path)
    lines.slice(i + 1, i + 1 + j).join("\n")
  }
  let lg = file-path.split(".").last()
  listing(txt, lang: lg, ..rest)
}
