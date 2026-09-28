// Граф хранилища в заметке: заметки — узлы, ссылки #see — рёбра.
//
//   #vault-graph()                                   // всё хранилище
//   #vault-graph(around: "Сеть/SSH", depth: 2)       // соседи заметки
//   #vault-graph(folders: ("Математика",), missing: false)
//   #vault-graph(tag: "линал", orphans: false)
//
// Фильтр и раскладку считает приложение (`notes-core::vault_graph`) — те
// же, что у страницы графа: заметка читает готовый граф из виртуального
// файла `/_vault/graph/<фильтр>.json`. Рисует CeTZ: в PDF и в HTML без
// скрипта граф — картинка; приложение оживляет его по тем же
// координатам (наведение, перетаскивание в рамке рисунка, переход к
// заметке по щелчку).
//
// Заметка с графом зависит от всего хранилища: она пересобирается, когда
// меняется любая заметка.

#import "@preview/cetz:0.4.2"
#import "theme.typ": current-theme
#import "web.typ": is-web, elem, frame

// Кегль подписи и её отступ от кружка — в единицах раскладки (как в ядре).
#let _label-size = 11
#let _label-gap = 2

// Путь в фильтре — строка JSON; `/` в имени файла экранируется (ядро разбирает обратно).
#let _escape(s) = s.replace("%", "%25").replace("/", "%2F").replace("\\", "%5C")

// Цвета групп (папок верхнего уровня) — те же, что у графа в приложении.
#let _group-colors(theme) = {
  let (c, b) = (theme.color, theme.color.boxes)
  (c.accent, b.example, c.secondary, b.idea, b.algorithm, b.pitfall, b.theorem)
}

/// Граф хранилища, весь или по фильтру.
/// - around, depth: только соседи заметки на depth шагов (она сама — в центре);
/// - folders: только эти папки верхнего уровня ("в корне" — заметки в корне);
/// - hidden: скрыть эти папки;
/// - tag: только заметки с тегом;
/// - missing: показывать ненаписанные (на них ссылаются, но их нет);
/// - orphans: показывать заметки без связей;
/// - width: наибольшая ширина рисунка.
#let vault-graph(
  around: none, depth: 1, folders: (), hidden: (), tag: none,
  missing: true, orphans: true, width: 15cm,
) = {
  assert(around == none or type(around) == str, message: "vault-graph: around — путь заметки, например \"Сеть/SSH\"")
  assert(type(depth) == int and depth >= 1, message: "vault-graph: depth — целое, не меньше 1")
  assert(type(folders) == array and type(hidden) == array, message: "vault-graph: folders и hidden — массивы строк, например (\"Сеть\",)")
  assert(tag == none or type(tag) == str, message: "vault-graph: tag — строка")
  let filter = (
    folders: folders, hidden: hidden, tag: tag, missing: missing, orphans: orphans,
    around: around, depth: depth,
  )
  let path = "/_vault/graph/" + _escape(json.encode(filter, pretty: false)) + ".json"
  let data = json(path)
  let (x0, y0, x1, y1) = data.bounds
  // Единица раскладки: подпись ~ 8,8 pt; большой граф ужимается до width.
  let unit = calc.min(0.8pt, width / calc.max(x1 - x0, 1))

  let drawing = context {
    let theme = current-theme()
    let colors = _group-colors(theme)
    let color-of(group) = colors.at(calc.rem(calc.max(data.groups.position(g => g == group), 0), colors.len()))
    let at(n) = (n.x, -n.y)
    let by-id = (:)
    for n in data.nodes { by-id.insert(n.id, n) }
    cetz.canvas(length: unit, {
      import cetz.draw: *
      for e in data.edges {
        let (a, b) = (by-id.at(e.from), by-id.at(e.to))
        let w = calc.min(1 + e.count * 0.4, 3)
        line(at(a), at(b), stroke: (paint: theme.color.muted.transparentize(35%), thickness: w * 0.45pt))
      }
      for n in data.nodes {
        let col = color-of(n.group)
        let book = n.kind == "book"
        let center = n.id == data.center
        circle(at(n), radius: n.r,
          fill: if n.kind == none { none } else { col },
          stroke: if center { 1.6pt + theme.color.text } else if n.kind == none { (paint: col, thickness: 0.8pt, dash: "dashed") } else { 0.6pt + col })
        let size = _label-size * unit * if book { 1.1 } else { 1 }
        content((n.x, -(n.y + n.r + _label-gap)), anchor: "north", padding: 0pt,
          text(size: size, fill: if n.kind == none { theme.color.muted } else { theme.color.text },
            weight: if book or center { "bold" } else { "regular" }, n.name))
      }
    })
  }
  context if is-web() {
    // Данные — клиенту: он оживляет граф по тем же координатам.
    elem("div", "k-graph", ..("data-k-graph": read(path)), frame(drawing))
  } else {
    align(center, drawing)
  }
}
