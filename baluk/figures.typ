// Рисунки на cetz 0.4.2 в цветах текущей темы.
//
// Все помощники возвращают элементы cetz и берут цвета из темы сами
// (через get-ctx → state), поэтому работают в любом cetz.canvas. Удобнее
// всего — внутри #canvas(theme => { ... }), где theme — текущая тема:
//
//   #fig(canvas(theme => {
//     import cetz.draw: *
//     axes(x: (-0.5, 4), y: (-0.5, 3))
//     plot(x => x * x / 4, 0, 3.4, label: $y = x^2/4$)
//     line((1, 0), (1, 2), stroke: 1pt + theme.color.fig.second)
//   }), [Подпись: что именно показывает рисунок])
//
// СОГЛАШЕНИЕ ОБ ОСЯХ: во всех помощниках ось y смотрит ВВЕРХ (как в
// математике), КРОМЕ `matrix-cells` и `array-cells`, где строки идут сверху вниз, как
// в матрице. В ICPC-конспекте путаница с направлением оси давала
// отражённые рисунки без единой ошибки сборки.
//
// ВАЖНО: внутри `import cetz.draw: *` имена line/rect/content/fill/stroke
// заняты функциями cetz — не называй так свои переменные.

#import "@preview/cetz:0.4.2"
#import "theme.typ": current-theme, is-dark
#import "web.typ": is-web, elem, frame

#let _draw = cetz.draw

// ── Цвет по имени: "line", "second", "third", "accent" или color ─────────
#let _color(theme, col, fallback: "line") = {
  let col = if col == auto { fallback } else { col }
  if type(col) == color { return col }
  if col == "accent" { return theme.color.accent }
  if col in theme.color.fig { return theme.color.fig.at(col) }
  if col in theme.color.boxes { return theme.color.boxes.at(col) }
  theme.color.fig.line
}

// ── Холст ─────────────────────────────────────────────────────────────────
/// cetz.canvas с настройками темы. Тело — блок { ... } или функция theme => { ... }.
#let canvas(body, unit: 1cm) = context {
  let theme = current-theme()
  let fc = theme.color.fig
  let elements = if type(body) == function { body(theme) } else { body }
  let drawing = {
    set text(size: theme.size.small)
    cetz.canvas(length: unit, {
      _draw.set-style(stroke: 0.6pt + fc.axis, content: (padding: 2pt))
      elements
    })
  }
  // В HTML — SVG. Его размер задан в em основного текста, поэтому рисунок
  // масштабируется вместе с кеглем страницы.
  if is-web() { frame(drawing) } else { drawing }
}

/// Рисунок с подписью. Подпись — утверждение о том, что видно на рисунке,
/// а не название («Спица входит через φ₁ и выходит через φ₂», а не «Область»).
/// floating: true — typst сам поставит рисунок вверх или вниз страницы
/// (как float в LaTeX). Для крупных рисунков, на которые есть ссылка
/// «рис. N» в тексте: они перестают оставлять дыры внизу страниц.
#let fig(body, caption, label: none, floating: false) = context {
  // В HTML плавающий рисунок пропал бы целиком: у страницы нет верха и низа.
  let fig-el = figure(body, caption: caption, kind: image, supplement: [Рис.],
    placement: if floating and not is-web() { auto } else { none })
  if label != none [#fig-el#std.label(label)] else { fig-el }
}

/// Несколько рисунков в ряд с общей подписью (small multiples).
/// разделители — содержимое между рисунками, например ($+$, $-$, $-$).
#let in-row(..elements, separators: none, gap: 0.8em) = context {
  let parts = elements.pos()
  if is-web() {
    return elem("div", "k-row", parts.enumerate().map(((i, x)) => {
      x
      if i < parts.len() - 1 and separators != none {
        elem("span", "k-row-sep", separators.at(i, default: []))
      }
    }).join())
  }
  let cells = ()
  for (i, x) in parts.enumerate() {
    cells.push(x)
    if i < parts.len() - 1 and separators != none {
      cells.push(text(size: 1.4em, separators.at(i, default: [])))
    }
  }
  // без align(center): рисунок и так центрирует тело, а естественная
  // ширина нужна шаблону, чтобы узнать «широкий» рисунок
  stack(dir: ltr, spacing: gap, ..cells.map(x => box(x, baseline: 50%)))
}

// ═════════════════════════════════════════════════════════════════════════
// 2D
// ═════════════════════════════════════════════════════════════════════════

/// Оси координат со стрелками и подписями.
// Сколько единиц холста в 1pt: размеры засечек, точек и отступов подписей
// задаём в pt, чтобы они не росли вместе с масштабом холста.
#let _pt(ctx) = 1pt / ctx.length

#let axes(x: (-0.5, 4), y: (-0.5, 3), labels: ($x$, $y$), origin: $O$) = _draw.get-ctx(ctx => {
  let fc = current-theme().color.fig
  let u = _pt(ctx)
  import cetz.draw: *
  let axis-stroke = 0.6pt + fc.axis
  let arrow-mark = (end: "stealth", fill: fc.axis, stroke: 0pt, scale: 0.55)
  line((x.at(0), 0), (x.at(1), 0), stroke: axis-stroke, mark: arrow-mark)
  line((0, y.at(0)), (0, y.at(1)), stroke: axis-stroke, mark: arrow-mark)
  content((x.at(1), -2 * u), anchor: "north-east", labels.at(0))
  content((-3 * u, y.at(1)), anchor: "north-east", labels.at(1))
  if origin != none { content((-1 * u, -1 * u), anchor: "north-east", text(size: 0.85em, origin)) }
})

/// Засечка на оси x с подписью и (необязательно) пунктиром вверх до высоты `to`.
#let tick(x, label, up-to: none) = _draw.get-ctx(ctx => {
  let fc = current-theme().color.fig
  let u = _pt(ctx)
  import cetz.draw: *
  if up-to != none {
    line((x, 0), (x, up-to), stroke: (paint: fc.axis.transparentize(40%), thickness: 0.45pt, dash: "dashed"))
  }
  line((x, -2.2 * u), (x, 2.2 * u), stroke: 0.6pt + fc.axis)
  content((x, -3 * u), anchor: "north", label)
})

/// График y = f(x) на [a, b]. цвет: "line" | "second" | "third" | color.
/// Подпись ставится у правого конца кривой или, если задан `label-x`,
/// у точки (подпись-x, f(подпись-x)) — так разводят подписи кривых,
/// сходящихся в одной точке.
#let plot(f, a, b, n: 80, color: auto, thickness: 1.1pt, dashed: false, label: none, label-anchor: "south-west", label-x: none) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  let pts = range(n + 1).map(i => { let x = a + (b - a) * i / n; (x, f(x)) })
  line(..pts, stroke: (paint: col, thickness: thickness, dash: if dashed { "dashed" } else { none }, join: "round"))
  if label != none {
    let p = if label-x == none { pts.last() } else { (label-x, f(label-x)) }
    content(p, anchor: label-anchor, padding: 3pt, text(fill: col, label))
  }
})

/// Параметрическая кривая t ↦ (x(t), y(t)).
#let parametric(fx, fy, t0, t1, n: 90, color: auto, thickness: 1.1pt, closed: false, fill-color: none) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  import cetz.draw: *
  let pts = range(n + 1).map(i => { let t = t0 + (t1 - t0) * i / n; (fx(t), fy(t)) })
  line(..pts, close: closed, fill: if fill-color == auto { theme.color.fig.fill } else { fill-color },
    stroke: (paint: _color(theme, color), thickness: thickness, join: "round"))
})

/// Заливка области a ≤ x ≤ b, низ(x) ≤ y ≤ верх(x).
#let fill-between(lo, hi, a, b, n: 60, color: auto) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  import cetz.draw: *
  let top = range(n + 1).map(i => { let x = a + (b - a) * i / n; (x, hi(x)) })
  let bot = range(n + 1).map(i => { let x = b - (b - a) * i / n; (x, lo(x)) })
  line(..top, ..bot, close: true, stroke: none,
    fill: if color == auto { theme.color.fig.fill } else { _color(theme, color) })
})

/// «Спица» — двусторонняя стрелка: вертикальная (x, y0 → y1) или,
/// с гориз: true, горизонтальная (y, x0 → x1).
#let spoke(c, from, to, horizontal: false, color: "second") = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  let (p, q) = if horizontal { ((from, c), (to, c)) } else { ((c, from), (c, to)) }
  line(p, q, stroke: 1.1pt + col, mark: (start: "stealth", end: "stealth", fill: col, stroke: 0pt, scale: 0.5))
})

/// Точка с подписью.
#let point(p, label: none, label-anchor: "south-west", color: "line", radius: 1.9pt) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  circle(p, radius: radius / ctx.length, fill: col, stroke: 0.5pt + theme.color.bg)
  if label != none { content(p, anchor: label-anchor, label, padding: 3pt) }
})

// ═════════════════════════════════════════════════════════════════════════
// Псевдо-3D (косая проекция). Ось x вправо, z вверх, y уходит ВЛЕВО-вверх
// («от нас»). Зритель смотрит слева-спереди-сверху, поэтому видны грани
// x = min, y = min и верх. Так все три оси идут по видимым рёбрам тела в
// первом октанте или выходят за его контур — подписи осей не прячутся за
// телом (при y вправо-вверх ось y уходила за столбики).
// Глубина для алгоритма художника: −0.45x − y + 0.32z (больше — ближе).
// Меняешь проекцию — меняй и `_depth`, и видимые грани в `_prism`.
// ═════════════════════════════════════════════════════════════════════════

#let p3(x, y, z) = (x - 0.45 * y, z + 0.32 * y)
#let _depth(x, y, z) = -0.45 * x - y + 0.32 * z

/// Оси x, y, z. Подписи ставятся за концами стрелок.
/// hidden: (x: a, y: b, z: c) — длина участка оси, спрятанного за телом.
/// Такие оси рисуют ПОСЛЕ тела: скрытый участок — тонким пунктиром поверх
/// (как невидимые рёбра в учебнике), остальное — сплошной линией.
#let axes3d(x: 3, y: 2.6, z: 2.4, labels: ($x$, $y$, $z$), hidden: (:)) = _draw.get-ctx(ctx => {
  let fc = current-theme().color.fig
  import cetz.draw: *
  let axis-stroke = 0.6pt + fc.axis
  let dashed = (paint: fc.axis.transparentize(35%), thickness: 0.5pt, dash: "dashed")
  let arrow-mark = (end: "stealth", fill: fc.axis, stroke: 0pt, scale: 0.55)
  for (axis, len, e) in (("x", x, (1, 0, 0)), ("y", y, (0, 1, 0)), ("z", z, (0, 0, 1))) {
    let h = hidden.at(axis, default: 0)
    let along(s) = p3(e.at(0) * s, e.at(1) * s, e.at(2) * s)
    if h > 0 { line(along(0), along(h), stroke: dashed) }
    line(along(h), along(len), stroke: axis-stroke, mark: arrow-mark)
  }
  content(p3(x, 0, 0), anchor: "west", labels.at(0), padding: 4pt)
  content(p3(0, y, 0), anchor: "south-east", labels.at(1), padding: 3pt)
  content(p3(0, 0, z), anchor: "south", labels.at(2), padding: 3pt)
})

// Освещённость грани с нормалью n (не обязательно единичной): 0..1.
#let _lighting(n) = {
  let (a, b, c) = n
  let len = calc.sqrt(a * a + b * b + c * c)
  let (lx, ly, lz) = (-0.35, -0.55, 0.76) // свет спереди-слева-сверху
  calc.max(0, (a * lx + b * ly + c * lz) / len)
}

#let _shade(theme, base, k) = {
  // k ∈ [0,1]: 0 — тень, 1 — полностью освещено
  let shadow = if is-dark(theme) { base.darken(35%) } else { base.darken(28%) }
  let light = if is-dark(theme) { base.lighten(18%) } else { base.lighten(45%) }
  color.mix((shadow, 100% - k * 100%), (light, k * 100%))
}

/// Поверхность z = f(x, y) над прямоугольником.
/// style: "shaded" — непрозрачные грани со светотенью (основной вариант),
///      "flat" — непрозрачные грани одного цвета,
///      "wire" — прозрачный каркас (видно, что под поверхностью).
#let surface(f, xr, yr, n: 14, m: auto, style: "shaded", color: auto, edge: auto) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  import cetz.draw: *
  let m = if m == auto { n } else { m }
  let (x0, x1) = xr
  let (y0, y1) = yr
  let hx = (x1 - x0) / n
  let hy = (y1 - y0) / m
  let base = if color == auto { fc.face } else { _color(theme, color) }
  let edge-st = if edge == auto { 0.3pt + fc.line.transparentize(35%) } else { edge }
  if style == "wire" {
    let st = 0.5pt + fc.line.transparentize(30%)
    for j in range(m + 1) {
      let y = y0 + hy * j
      line(..range(n * 3 + 1).map(i => { let x = x0 + (x1 - x0) * i / (n * 3); p3(x, y, f(x, y)) }), stroke: st)
    }
    for i in range(n + 1) {
      let x = x0 + hx * i
      line(..range(m * 3 + 1).map(j => { let y = y0 + (y1 - y0) * j / (m * 3); p3(x, y, f(x, y)) }), stroke: st)
    }
    return
  }
  let cells = ()
  for i in range(n) {
    for j in range(m) {
      let (xa, xb) = (x0 + hx * i, x0 + hx * (i + 1))
      let (ya, yb) = (y0 + hy * j, y0 + hy * (j + 1))
      let (xc, yc) = ((xa + xb) / 2, (ya + yb) / 2)
      cells.push((_depth(xc, yc, f(xc, yc)), xa, xb, ya, yb))
    }
  }
  for (_, xa, xb, ya, yb) in cells.sorted(key: q => q.at(0)) {
    let (za, zb, zc, zd) = (f(xa, ya), f(xb, ya), f(xb, yb), f(xa, yb))
    let face = if style == "shaded" {
      // нормаль по двум диагоналям ячейки
      let u = (xb - xa, yb - ya, zc - za)
      let v = (xa - xb, yb - ya, zd - zb)
      let nrm = (u.at(1) * v.at(2) - u.at(2) * v.at(1), u.at(2) * v.at(0) - u.at(0) * v.at(2), u.at(0) * v.at(1) - u.at(1) * v.at(0))
      _shade(theme, base, _lighting(nrm))
    } else { base }
    line(p3(xa, ya, za), p3(xb, ya, zb), p3(xb, yb, zc), p3(xa, yb, zd), close: true, fill: face, stroke: edge-st)
  }
})

/// Прямоугольный параллелепипед [xa,xb]×[ya,yb]×[0,h] с видимыми гранями
/// (верх, перед, левый бок). Годится для столбиков интегральной суммы.
#let _prism(theme, xa, xb, ya, yb, h, base, edge-st) = {
  import cetz.draw: *
  // левый бок (x = xa), перед (y = ya), верх (z = h)
  line(p3(xa, ya, 0), p3(xa, yb, 0), p3(xa, yb, h), p3(xa, ya, h), close: true, fill: _shade(theme, base, 0.4), stroke: edge-st)
  line(p3(xa, ya, 0), p3(xb, ya, 0), p3(xb, ya, h), p3(xa, ya, h), close: true, fill: _shade(theme, base, 0.68), stroke: edge-st)
  line(p3(xa, ya, h), p3(xb, ya, h), p3(xb, yb, h), p3(xa, yb, h), close: true, fill: _shade(theme, base, 0.95), stroke: edge-st)
}

/// Столбики интегральной суммы: разбиение n × m, высота — значение f
/// в центре ячейки. выделить: ((i, j), ...) — ячейки другим цветом.
#let prisms(f, xr, yr, n: 5, m: 4, gap: 0.0, highlight: (), color: auto, highlight-color: "second") = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  let (x0, x1) = xr
  let (y0, y1) = yr
  let hx = (x1 - x0) / n
  let hy = (y1 - y0) / m
  let base = if color == auto { fc.face } else { _color(theme, color) }
  let bright = _color(theme, highlight-color)
  let edge-st = 0.35pt + fc.line.transparentize(20%)
  let all = ()
  for i in range(n) {
    for j in range(m) {
      let xc = x0 + hx * (i + 0.5)
      let yc = y0 + hy * (j + 0.5)
      all.push((_depth(xc, yc, 0), i, j, xc, yc))
    }
  }
  for (_, i, j, xc, yc) in all.sorted(key: q => q.at(0)) {
    let half-gap = gap / 2
    _prism(theme, x0 + hx * i + half-gap, x0 + hx * (i + 1) - half-gap, y0 + hy * j + half-gap, y0 + hy * (j + 1) - half-gap, f(xc, yc),
      if (i, j) in highlight { bright.lighten(if is-dark(theme) { 0% } else { 25% }) } else { base }, edge-st)
  }
})

/// Плоская фигура в плоскости z = 0 по вершинам (x, y).
#let base-shape(..points, color: auto, label: none) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  import cetz.draw: *
  let pts = points.pos()
  line(..pts.map(p => p3(p.at(0), p.at(1), 0)), close: true,
    fill: if color == auto { theme.color.fig.fill } else { _color(theme, color) },
    stroke: 0.7pt + theme.color.fig.line)
  if label != none {
    let cx = pts.map(p => p.at(0)).sum() / pts.len()
    let cy = pts.map(p => p.at(1)).sum() / pts.len()
    content(p3(cx, cy, 0), label)
  }
})

// ═════════════════════════════════════════════════════════════════════════
// Массивы и таблицы значений (ось вниз: строка 0 — верхняя)
// ═════════════════════════════════════════════════════════════════════════

/// Одномерный массив.
/// - highlight: словарь «индекс → цвет» ("accent", "second", ... или color);
/// - spans: ((l, r, цвет, подпись), ...) — скобка под ячейками l..r включительно;
/// - pointers: ((i, подпись), ...) — стрелка сверху к ячейке i;
/// - index-from: номер первой ячейки (0 или 1);
/// - block-size: размер блока — границы блоков рисуются жирнее (sqrt-декомпозиция);
/// - block-values: значения по блокам — коробки над массивом во всю ширину блока
///   (суммы блоков, максимумы, «ленивые» пометки);
/// - arcs: ((i, j, подпись, цвет), ...) — дуга над массивом между ячейками
///   (обмен, ссылка, пара двух указателей).
#let array-cells(
  values, cell: 0.62, highlight: (:), spans: (), pointers: (),
  indices: true, index-from: 0, block-size: none, block-values: (), arcs: (),
) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  import cetz.draw: *
  let k = cell
  for (i, v) in values.enumerate() {
    let col = highlight.at(str(i + index-from), default: none)
    let fill-color = if col == none { theme.color.bg } else {
      let c = _color(theme, col)
      if is-dark(theme) { c.transparentize(65%) } else { c.lighten(72%) }
    }
    rect((i * k, 0), ((i + 1) * k, k), fill: fill-color, stroke: 0.6pt + fc.line.transparentize(10%))
    content(((i + 0.5) * k, k / 2), [#v])
    if indices {
      content(((i + 0.5) * k, -0.02), anchor: "north", text(size: 0.72em, fill: theme.color.muted, str(i + index-from)))
    }
  }
  for (l, r, col, label) in spans {
    let c = _color(theme, col)
    let y = if indices { -0.42 } else { -0.12 }
    let (a, b) = ((l - index-from) * k + 0.05, (r - index-from + 1) * k - 0.05)
    line((a, y + 0.1), (a, y), (b, y), (b, y + 0.1), stroke: 0.9pt + c)
    content(((a + b) / 2, y - 0.04), anchor: "north", text(fill: c, size: 0.85em, label))
  }
  for (i, label) in pointers {
    let x = (i - index-from + 0.5) * k
    line((x, k + 0.45), (x, k + 0.06), stroke: 0.9pt + fc.second, mark: (end: "stealth", fill: fc.second, stroke: 0pt, scale: 0.45))
    content((x, k + 0.45), anchor: "south", text(fill: fc.second, size: 0.85em, label))
  }
  // Границы блоков поверх клеток.
  if block-size != none {
    let n = values.len()
    let nblocks = calc.ceil(n / block-size)
    for j in range(nblocks + 1) {
      let x = calc.min(j * block-size, n) * k
      line((x, 0), (x, k), stroke: 1.4pt + fc.line)
    }
  }
  // Своды блоков: коробка над каждым блоком.
  if block-values.len() > 0 {
    let n = values.len()
    let block-w = if block-size != none { block-size } else { calc.ceil(n / block-values.len()) }
    for (j, v) in block-values.enumerate() {
      let a = j * block-w * k
      let b = calc.min((j + 1) * block-w, n) * k
      if b <= a { continue }
      rect((a, k + 0.18), (b, k + 0.18 + k * 0.85),
        fill: if is-dark(theme) { fc.line.transparentize(78%) } else { fc.line.lighten(85%) },
        stroke: 0.6pt + fc.line)
      content(((a + b) / 2, k + 0.18 + k * 0.425), text(size: 0.85em, [#v]))
    }
  }
  for bow in arcs {
    let (i, j) = (bow.at(0), bow.at(1))
    let label = bow.at(2, default: none)
    let c = _color(theme, bow.at(3, default: "second"))
    let (xi, xj) = ((i - index-from + 0.5) * k, (j - index-from + 0.5) * k)
    let peak = k + 0.12 + calc.abs(xj - xi) * 0.32
    bezier((xi, k + 0.04), (xj, k + 0.04), ((xi + xj) / 2, peak),
      stroke: 0.9pt + c, mark: (end: "stealth", fill: c, stroke: 0pt, scale: 0.45))
    if label != none {
      content(((xi + xj) / 2, peak * 0.88), anchor: "south", text(fill: c, size: 0.8em, label))
    }
  }
})

/// Двумерная таблица значений (матрица). Строки — сверху вниз.
/// - rects: ((r1, c1, r2, c2, цвет), ...) — включительно, в
///   нумерации `index-from`; рисуются полупрозрачной заливкой с рамкой;
/// - cells: словарь "r,c" → цвет — точечная подсветка;
/// - indices: подписи номеров строк и столбцов;
/// - values: false — только клетки, без чисел (для мелких схем);
/// - arrows: ((r1, c1, r2, c2, цвет), ...) — переход из клетки в клетку
///   (ровно то, что нужно для таблиц ДП).
#let matrix-cells(
  mat, cell: 0.55, rects: (), cells: (:), indices: true,
  index-from: 1, compact: false, values: true, arrows: (),
) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  import cetz.draw: *
  let k = cell
  let n = mat.len()
  let w = mat.at(0).len()
  let cell-pos(r, c) = (c * k, -r * k) // левый верхний угол клетки (r, c), от 0
  for r in range(n) {
    for c in range(w) {
      let key = str(r + index-from) + "," + str(c + index-from)
      let col = cells.at(key, default: none)
      let (x, y) = cell-pos(r, c)
      rect((x, y), (x + k, y - k), stroke: 0.45pt + fc.grid,
        fill: if col == none { theme.color.bg } else { let cc = _color(theme, col); if is-dark(theme) { cc.transparentize(60%) } else { cc.lighten(70%) } })
    }
  }
  for (r1, c1, r2, c2, col) in rects {
    let c = _color(theme, col)
    let (xa, ya) = cell-pos(r1 - index-from, c1 - index-from)
    let (xb, yb) = cell-pos(r2 - index-from + 1, c2 - index-from + 1)
    rect((xa, ya), (xb, yb), fill: c.transparentize(if is-dark(theme) { 70% } else { 78% }), stroke: 1.1pt + c)
  }
  if values {
    for r in range(n) {
      for c in range(w) {
        let (x, y) = cell-pos(r, c)
        content((x + k / 2, y - k / 2), text(size: if compact { 0.78em } else { 0.9em }, [#mat.at(r).at(c)]))
      }
    }
  }
  for ar in arrows {
    let (r1, c1, r2, c2) = (ar.at(0), ar.at(1), ar.at(2), ar.at(3))
    let c = _color(theme, ar.at(4, default: "second"))
    let (x1, y1) = cell-pos(r1 - index-from, c1 - index-from)
    let (x2, y2) = cell-pos(r2 - index-from, c2 - index-from)
    let (a, b) = ((x1 + k / 2, y1 - k / 2), (x2 + k / 2, y2 - k / 2))
    let (dx, dy) = (b.at(0) - a.at(0), b.at(1) - a.at(1))
    let d = calc.sqrt(dx * dx + dy * dy)
    let pad-len = k * 0.34
    line(
      (a.at(0) + dx / d * pad-len, a.at(1) + dy / d * pad-len),
      (b.at(0) - dx / d * pad-len, b.at(1) - dy / d * pad-len),
      stroke: 1pt + c, mark: (end: "stealth", fill: c, stroke: 0pt, scale: 0.4),
    )
  }
  if indices {
    for c in range(w) {
      let (x, y) = cell-pos(0, c)
      content((x + k / 2, 0.04), anchor: "south", text(size: 0.68em, fill: theme.color.muted, str(c + index-from)))
    }
    for r in range(n) {
      let (x, y) = cell-pos(r, 0)
      content((-0.06, y - k / 2), anchor: "east", text(size: 0.68em, fill: theme.color.muted, str(r + index-from)))
    }
  }
})

// ═════════════════════════════════════════════════════════════════════════
// Графы (ось y вверх)
// ═════════════════════════════════════════════════════════════════════════

/// Граф.
/// - vertices: словарь имя → (x, y);
/// - edges: массив (u, v) | (u, v, стиль) | (u, v, стиль, подпись);
///   стили: "normal", "bold", "second", "dim", "dashed";
/// - highlight: имена вершин с заливкой акцентом; тусклые: приглушённые;
/// - labels: словарь имя → содержимое (по умолчанию — само имя);
/// - marks: словарь имя → (содержимое, якорь) — пометка рядом с вершиной;
/// - shape: "circle" или "rect" (для узлов с двумя строками —
///   ключ и приоритет декартова дерева, состояние автомата и т. п.);
///   размер прямоугольника — `size: (width, height)`.
/// Встречные рёбра u→v и v→u в ориентированном графе разводятся сами.
#let graph(
  vertices, edges, directed: false, radius: 0.27, highlight: (),
  dimmed: (), labels: (:), marks: (:), shape: "circle", size: (0.72, 0.46),
) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let fc = theme.color.fig
  import cetz.draw: *
  let pairs = edges.map(e => (e.at(0), e.at(1)))
  for e in edges {
    let (u, v) = (e.at(0), e.at(1))
    let style = e.at(2, default: "normal")
    let label = e.at(3, default: none)
    let (x1, y1) = vertices.at(u)
    let (x2, y2) = vertices.at(v)
    let (dx, dy) = (x2 - x1, y2 - y1)
    let d = calc.sqrt(dx * dx + dy * dy)
    let (ux, uy) = (dx / d, dy / d)
    // сдвиг встречных рёбер
    let s = if directed and (v, u) in pairs { 0.09 } else { 0 }
    let (ox, oy) = (-uy * s, ux * s)
    // до края узла: у круга — радиус, у прямоугольника — пересечение со стороной
    let edge = if shape == "rect" {
      let (a, b) = (size.at(0) / 2, size.at(1) / 2)
      calc.min(
        if calc.abs(ux) < 0.001 { 1e9 } else { a / calc.abs(ux) },
        if calc.abs(uy) < 0.001 { 1e9 } else { b / calc.abs(uy) },
      )
    } else { radius }
    let p = (x1 + ux * edge + ox, y1 + uy * edge + oy)
    let q = (x2 - ux * edge + ox, y2 - uy * edge + oy)
    let (col, thickness, dash) = if style == "bold" { (fc.line, 1.5pt, none) } else if style == "second" { (fc.second, 1.5pt, none) } else if style == "dim" { (theme.color.muted.transparentize(45%), 0.6pt, none) } else if style == "dashed" { (theme.color.muted, 0.7pt, "dashed") } else { (fc.axis.transparentize(15%), 0.8pt, none) }
    line(p, q, stroke: (paint: col, thickness: thickness, dash: dash),
      mark: if directed { (end: "stealth", fill: col, stroke: 0pt, scale: 0.55) } else { none })
    if label != none {
      let m = ((p.at(0) + q.at(0)) / 2 - uy * 0.16, (p.at(1) + q.at(1)) / 2 + ux * 0.16)
      content(m, box(fill: theme.color.bg, inset: 1.2pt, radius: 1.5pt, text(size: 0.78em, fill: if style in ("bold", "second") { col } else { theme.color.text }, label)))
    }
  }
  for (name, pos) in vertices {
    let sel = name in highlight
    let dim = name in dimmed
    let fill-color = if sel { fc.line } else { theme.color.bg }
    let border = (if dim { 0.6pt } else { 0.9pt }) + (if dim { theme.color.muted.transparentize(40%) } else { fc.line })
    if shape == "rect" {
      let (a, b) = (size.at(0) / 2, size.at(1) / 2)
      rect((pos.at(0) - a, pos.at(1) - b), (pos.at(0) + a, pos.at(1) + b),
        fill: fill-color, stroke: border, radius: 0.06)
    } else {
      circle(pos, radius: radius, fill: fill-color, stroke: border)
    }
    content(pos, text(size: 0.85em, weight: if sel { "bold" } else { "regular" },
      fill: if sel { theme.color.bg } else if dim { theme.color.muted } else { theme.color.text },
      labels.at(name, default: name)))
    if name in marks {
      let (mk, label-anchor) = marks.at(name)
      content(pos, anchor: label-anchor, text(size: 0.75em, fill: fc.second, mk),
        padding: (if shape == "rect" { size.at(0) / 2 } else { radius }) * 1cm + 2pt)
    }
  }
})

/// Линии уровня f(x, y) = c методом marching squares: та самая картинка,
/// по которой студент понимает форму поверхности, глядя сверху.
#let contours(f, levels, xr: (-2, 2), yr: (-2, 2), n: 40, color: auto) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  let (x0, x1) = xr
  let (y0, y1) = yr
  let dx = (x1 - x0) / n
  let dy = (y1 - y0) / n
  for c in levels {
    for i in range(n) {
      for j in range(n) {
        let (xa, ya) = (x0 + dx * i, y0 + dy * j)
        let ang = ((xa, ya), (xa + dx, ya), (xa + dx, ya + dy), (xa, ya + dy))
        let corner-vals = ang.map(p => f(p.at(0), p.at(1)) - c)
        let points = ()
        for k in range(4) {
          let (a, b) = (corner-vals.at(k), corner-vals.at(calc.rem(k + 1, 4)))
          if (a < 0) != (b < 0) {
            let t = a / (a - b)
            let (pa, pb) = (ang.at(k), ang.at(calc.rem(k + 1, 4)))
            points.push((pa.at(0) + (pb.at(0) - pa.at(0)) * t, pa.at(1) + (pb.at(1) - pa.at(1)) * t))
          }
        }
        if points.len() == 2 { line(points.at(0), points.at(1), stroke: 0.7pt + col) }
      }
    }
  }
})

/// Тело вращения: горизонтальные «обручи» радиуса r(z) и силуэт. Обручи —
/// это буквально линии r = const, то есть идея цилиндрических координат.
#let revolution(r, zmin: 0, zmax: 2, rings: 8, n: 40, color: auto) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  for k in range(rings + 1) {
    let z = zmin + (zmax - zmin) * k / rings
    let rr = r(z)
    if rr > 0.001 {
      line(..range(n + 1).map(i => {
        let t = 2 * calc.pi * i / n
        p3(rr * calc.cos(t), rr * calc.sin(t), z)
      }), close: true, stroke: 0.5pt + col.transparentize(35%))
    }
  }
  for s in (1, -1) {
    line(..range(n + 1).map(i => {
      let z = zmin + (zmax - zmin) * i / n
      p3(s * r(z), 0, z)
    }), stroke: 1pt + col)
  }
})

/// Сечение поверхности z = f(x, y) плоскостью y = y0 — «слой», из которых
/// набирается объём.
#let cross-section(f, y0, xr: (0, 3), n: 40, color: "second", fill-color: true) = _draw.get-ctx(ctx => {
  let theme = current-theme()
  let col = _color(theme, color)
  import cetz.draw: *
  let (a, b) = xr
  let hi = range(n + 1).map(i => { let x = a + (b - a) * i / n; p3(x, y0, f(x, y0)) })
  if fill-color {
    line(..hi, p3(b, y0, 0), p3(a, y0, 0), close: true,
      fill: col.transparentize(if is-dark(theme) { 75% } else { 85% }), stroke: none)
  }
  line(..hi, stroke: 1.1pt + col)
})

/// Раскладка дерева по уровням: корень сверху, дети — ниже. Каждому
/// поддереву выделяется полоса по его ширине — соседи не наезжают.
#let tree-layout(edges, root, dx: 0.9, dy: 0.95) = {
  let children = (:)
  for e in edges {
    let (u, v) = (e.at(0), e.at(1))
    children.insert(u, children.at(u, default: ()) + (v,))
  }
  let width(v) = {
    let item = children.at(v, default: ())
    if item.len() == 0 { 1 } else { item.map(width).sum() }
  }
  let cell-pos = (:)
  let lay-out(v, x0, dep) = {
    let item = children.at(v, default: ())
    let w = width(v)
    let summary = ((v, ((x0 + w / 2 - 0.5) * dx, -dep * dy)),)
    let x = x0
    for c in item {
      summary += lay-out(c, x, dep + 1)
      x += width(c)
    }
    summary
  }
  for (v, p) in lay-out(root, 0, 0) { cell-pos.insert(v, p) }
  cell-pos
}

/// Раскладка бинарного дерева: x — порядок обхода «слева направо»
/// (in-order), y — глубина. Именно так рисуют деревья поиска, декартовы
/// деревья (treap) и деревья отрезков: ключи идут слева направо.
/// children: словарь имя → (левый, right) — `none` там, где ребёнка нет.
#let binary-layout(children, root, dx: 0.85, dy: 0.95) = {
  // in-order: сначала левое поддерево, потом сам, потом правое.
  // Счётчик передаётся и возвращается — замыкания в typst не меняют
  // переменные внешней области.
  let visit(v, depth, i) = {
    if v == none { return ((), i) }
    let (cur, prm) = children.at(v, default: (none, none))
    let (on-left, i2) = visit(cur, depth + 1, i)
    let me = ((v, (i2 * dx, -depth * dy)),)
    let (on-right, i3) = visit(prm, depth + 1, i2 + 1)
    (on-left + me + on-right, i3)
  }
  let (pairs, _) = visit(root, 0, 0)
  let cell-pos = (:)
  for (v, p) in pairs { cell-pos.insert(v, p) }
  cell-pos
}

/// Рёбра бинарного дерева по словарю детей — в том виде, который ждёт `graph`.
#let binary-edges(children, style: "normal") = {
  let edges = ()
  for (v, (cur, prm)) in children {
    if cur != none { edges.push((v, cur, style)) }
    if prm != none { edges.push((v, prm, style)) }
  }
  edges
}

/// Раскладка по окружности (первая вершина сверху, дальше по часовой).
#let circle-layout(names, radius: 1.3, start-angle: 90deg) = {
  let n = names.len()
  let cell-pos = (:)
  for (i, v) in names.enumerate() {
    let a = start-angle - 360deg * i / n
    cell-pos.insert(v, (radius * calc.cos(a), radius * calc.sin(a)))
  }
  cell-pos
}
