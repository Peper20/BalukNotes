// Рисунки на cetz 0.4.2 в цветах текущей темы.
//
// Все помощники возвращают элементы cetz и берут цвета из темы сами
// (через get-ctx → state), поэтому работают в любом cetz.canvas. Удобнее
// всего — внутри #холст(т => { ... }), где т — текущая тема:
//
//   #рис(холст(т => {
//     import cetz.draw: *
//     оси(x: (-0.5, 4), y: (-0.5, 3))
//     график(x => x * x / 4, 0, 3.4, подпись: $y = x^2/4$)
//     line((1, 0), (1, 2), stroke: 1pt + т.цвет.рис.второй)
//   }), [Подпись: что именно показывает рисунок])
//
// СОГЛАШЕНИЕ ОБ ОСЯХ: во всех помощниках ось y смотрит ВВЕРХ (как в
// математике), КРОМЕ `сетка` и `массив`, где строки идут сверху вниз, как
// в матрице. В ICPC-конспекте путаница с направлением оси давала
// отражённые рисунки без единой ошибки сборки.
//
// ВАЖНО: внутри `import cetz.draw: *` имена line/rect/content/fill/stroke
// заняты функциями cetz — не называй так свои переменные.

#import "@preview/cetz:0.4.2"
#import "theme.typ": тема, тёмная
#import "web.typ": веб, эл, кадр

#let _draw = cetz.draw

// ── Цвет по имени: "линия", "второй", "третий", "акцент" или color ─────────
#let _цвет(т, ц, по-умолч: "линия") = {
  let ц = if ц == auto { по-умолч } else { ц }
  if type(ц) == color { return ц }
  if ц == "акцент" { return т.цвет.акцент }
  if ц in т.цвет.рис { return т.цвет.рис.at(ц) }
  if ц in т.цвет.врезки { return т.цвет.врезки.at(ц) }
  т.цвет.рис.линия
}

// ── Холст ─────────────────────────────────────────────────────────────────
/// cetz.canvas с настройками темы. Тело — блок { ... } или функция т => { ... }.
#let холст(тело, масштаб: 1cm) = context {
  let т = тема()
  let р = т.цвет.рис
  let элементы = if type(тело) == function { тело(т) } else { тело }
  let полотно = {
    set text(size: т.кегль.мелкий)
    cetz.canvas(length: масштаб, {
      _draw.set-style(stroke: 0.6pt + р.ось, content: (padding: 2pt))
      элементы
    })
  }
  // В HTML — SVG. Его размер задан в em основного текста, поэтому рисунок
  // масштабируется вместе с кеглем страницы.
  if веб() { кадр(полотно) } else { полотно }
}

/// Рисунок с подписью. Подпись — утверждение о том, что видно на рисунке,
/// а не название («Спица входит через φ₁ и выходит через φ₂», а не «Область»).
/// плавающий: true — typst сам поставит рисунок вверх или вниз страницы
/// (как float в LaTeX). Для крупных рисунков, на которые есть ссылка
/// «рис. N» в тексте: они перестают оставлять дыры внизу страниц.
#let рис(тело, подпись, метка: none, плавающий: false) = context {
  // В HTML плавающий рисунок пропал бы целиком: у страницы нет верха и низа.
  let ф = figure(тело, caption: подпись, kind: image, supplement: [Рис.],
    placement: if плавающий and not веб() { auto } else { none })
  if метка != none [#ф#label(метка)] else { ф }
}

/// Несколько рисунков в ряд с общей подписью (small multiples).
/// разделители — содержимое между рисунками, например ($+$, $-$, $-$).
#let в-ряд(..элементы, разделители: none, зазор: 0.8em) = context {
  let э = элементы.pos()
  if веб() {
    return эл("div", "k-row", э.enumerate().map(((i, x)) => {
      x
      if i < э.len() - 1 and разделители != none {
        эл("span", "k-row-sep", разделители.at(i, default: []))
      }
    }).join())
  }
  let ячейки = ()
  for (i, x) in э.enumerate() {
    ячейки.push(x)
    if i < э.len() - 1 and разделители != none {
      ячейки.push(text(size: 1.4em, разделители.at(i, default: [])))
    }
  }
  // без align(center): рисунок и так центрирует тело, а естественная
  // ширина нужна шаблону, чтобы узнать «широкий» рисунок
  stack(dir: ltr, spacing: зазор, ..ячейки.map(x => box(x, baseline: 50%)))
}

// ═════════════════════════════════════════════════════════════════════════
// 2D
// ═════════════════════════════════════════════════════════════════════════

/// Оси координат со стрелками и подписями.
// Сколько единиц холста в 1pt: размеры засечек, точек и отступов подписей
// задаём в pt, чтобы они не росли вместе с масштабом холста.
#let _pt(ctx) = 1pt / ctx.length

#let оси(x: (-0.5, 4), y: (-0.5, 3), подписи: ($x$, $y$), начало: $O$) = _draw.get-ctx(ctx => {
  let р = тема().цвет.рис
  let u = _pt(ctx)
  import cetz.draw: *
  let штрих = 0.6pt + р.ось
  let м = (end: "stealth", fill: р.ось, stroke: 0pt, scale: 0.55)
  line((x.at(0), 0), (x.at(1), 0), stroke: штрих, mark: м)
  line((0, y.at(0)), (0, y.at(1)), stroke: штрих, mark: м)
  content((x.at(1), -2 * u), anchor: "north-east", подписи.at(0))
  content((-3 * u, y.at(1)), anchor: "north-east", подписи.at(1))
  if начало != none { content((-1 * u, -1 * u), anchor: "north-east", text(size: 0.85em, начало)) }
})

/// Засечка на оси x с подписью и (необязательно) пунктиром вверх до высоты `до`.
#let засечка(x, подпись, до: none) = _draw.get-ctx(ctx => {
  let р = тема().цвет.рис
  let u = _pt(ctx)
  import cetz.draw: *
  if до != none {
    line((x, 0), (x, до), stroke: (paint: р.ось.transparentize(40%), thickness: 0.45pt, dash: "dashed"))
  }
  line((x, -2.2 * u), (x, 2.2 * u), stroke: 0.6pt + р.ось)
  content((x, -3 * u), anchor: "north", подпись)
})

/// График y = f(x) на [a, b]. цвет: "линия" | "второй" | "третий" | color.
/// Подпись ставится у правого конца кривой или, если задан `подпись-x`,
/// у точки (подпись-x, f(подпись-x)) — так разводят подписи кривых,
/// сходящихся в одной точке.
#let график(f, a, b, n: 80, цвет: auto, толщина: 1.1pt, пунктир: false, подпись: none, якорь: "south-west", подпись-x: none) = _draw.get-ctx(ctx => {
  let т = тема()
  let ц = _цвет(т, цвет)
  import cetz.draw: *
  let pts = range(n + 1).map(i => { let x = a + (b - a) * i / n; (x, f(x)) })
  line(..pts, stroke: (paint: ц, thickness: толщина, dash: if пунктир { "dashed" } else { none }, join: "round"))
  if подпись != none {
    let p = if подпись-x == none { pts.last() } else { (подпись-x, f(подпись-x)) }
    content(p, anchor: якорь, padding: 3pt, text(fill: ц, подпись))
  }
})

/// Параметрическая кривая t ↦ (x(t), y(t)).
#let кривая(fx, fy, t0, t1, n: 90, цвет: auto, толщина: 1.1pt, замкнуть: false, заливка: none) = _draw.get-ctx(ctx => {
  let т = тема()
  import cetz.draw: *
  let pts = range(n + 1).map(i => { let t = t0 + (t1 - t0) * i / n; (fx(t), fy(t)) })
  line(..pts, close: замкнуть, fill: if заливка == auto { т.цвет.рис.заливка } else { заливка },
    stroke: (paint: _цвет(т, цвет), thickness: толщина, join: "round"))
})

/// Заливка области a ≤ x ≤ b, низ(x) ≤ y ≤ верх(x).
#let область(низ, верх, a, b, n: 60, цвет: auto) = _draw.get-ctx(ctx => {
  let т = тема()
  import cetz.draw: *
  let top = range(n + 1).map(i => { let x = a + (b - a) * i / n; (x, верх(x)) })
  let bot = range(n + 1).map(i => { let x = b - (b - a) * i / n; (x, низ(x)) })
  line(..top, ..bot, close: true, stroke: none,
    fill: if цвет == auto { т.цвет.рис.заливка } else { _цвет(т, цвет) })
})

/// «Спица» — двусторонняя стрелка: вертикальная (x, y0 → y1) или,
/// с гориз: true, горизонтальная (y, x0 → x1).
#let спица(c, от, до, гориз: false, цвет: "второй") = _draw.get-ctx(ctx => {
  let т = тема()
  let ц = _цвет(т, цвет)
  import cetz.draw: *
  let (p, q) = if гориз { ((от, c), (до, c)) } else { ((c, от), (c, до)) }
  line(p, q, stroke: 1.1pt + ц, mark: (start: "stealth", end: "stealth", fill: ц, stroke: 0pt, scale: 0.5))
})

/// Точка с подписью.
#let точка(p, подпись: none, якорь: "south-west", цвет: "линия", радиус: 1.9pt) = _draw.get-ctx(ctx => {
  let т = тема()
  let ц = _цвет(т, цвет)
  import cetz.draw: *
  circle(p, radius: радиус / ctx.length, fill: ц, stroke: 0.5pt + т.цвет.фон)
  if подпись != none { content(p, anchor: якорь, подпись, padding: 3pt) }
})

// ═════════════════════════════════════════════════════════════════════════
// Псевдо-3D (косая проекция). Ось x вправо, z вверх, y уходит ВЛЕВО-вверх
// («от нас»). Зритель смотрит слева-спереди-сверху, поэтому видны грани
// x = min, y = min и верх. Так все три оси идут по видимым рёбрам тела в
// первом октанте или выходят за его контур — подписи осей не прячутся за
// телом (при y вправо-вверх ось y уходила за столбики).
// Глубина для алгоритма художника: −0.45x − y + 0.32z (больше — ближе).
// Меняешь проекцию — меняй и `_глубина`, и видимые грани в `_столбик`.
// ═════════════════════════════════════════════════════════════════════════

#let п3(x, y, z) = (x - 0.45 * y, z + 0.32 * y)
#let _глубина(x, y, z) = -0.45 * x - y + 0.32 * z

/// Оси x, y, z. Подписи ставятся за концами стрелок.
/// скрыто: (x: a, y: b, z: c) — длина участка оси, спрятанного за телом.
/// Такие оси рисуют ПОСЛЕ тела: скрытый участок — тонким пунктиром поверх
/// (как невидимые рёбра в учебнике), остальное — сплошной линией.
#let оси3d(x: 3, y: 2.6, z: 2.4, подписи: ($x$, $y$, $z$), скрыто: (:)) = _draw.get-ctx(ctx => {
  let р = тема().цвет.рис
  import cetz.draw: *
  let штрих = 0.6pt + р.ось
  let пунктир = (paint: р.ось.transparentize(35%), thickness: 0.5pt, dash: "dashed")
  let м = (end: "stealth", fill: р.ось, stroke: 0pt, scale: 0.55)
  for (ось, конец, e) in (("x", x, (1, 0, 0)), ("y", y, (0, 1, 0)), ("z", z, (0, 0, 1))) {
    let h = скрыто.at(ось, default: 0)
    let т(s) = п3(e.at(0) * s, e.at(1) * s, e.at(2) * s)
    if h > 0 { line(т(0), т(h), stroke: пунктир) }
    line(т(h), т(конец), stroke: штрих, mark: м)
  }
  content(п3(x, 0, 0), anchor: "west", подписи.at(0), padding: 4pt)
  content(п3(0, y, 0), anchor: "south-east", подписи.at(1), padding: 3pt)
  content(п3(0, 0, z), anchor: "south", подписи.at(2), padding: 3pt)
})

// Освещённость грани с нормалью n (не обязательно единичной): 0..1.
#let _свет(n) = {
  let (a, b, c) = n
  let len = calc.sqrt(a * a + b * b + c * c)
  let (lx, ly, lz) = (-0.35, -0.55, 0.76) // свет спереди-слева-сверху
  calc.max(0, (a * lx + b * ly + c * lz) / len)
}

#let _оттенок(т, база, k) = {
  // k ∈ [0,1]: 0 — тень, 1 — полностью освещено
  let тень = if тёмная(т) { база.darken(35%) } else { база.darken(28%) }
  let свет = if тёмная(т) { база.lighten(18%) } else { база.lighten(45%) }
  color.mix((тень, 100% - k * 100%), (свет, k * 100%))
}

/// Поверхность z = f(x, y) над прямоугольником.
/// вид: "свет" — непрозрачные грани со светотенью (основной вариант),
///      "грани" — непрозрачные грани одного цвета,
///      "сетка" — прозрачный каркас (видно, что под поверхностью).
#let поверхность(f, xr, yr, n: 14, m: auto, вид: "свет", цвет: auto, край: auto) = _draw.get-ctx(ctx => {
  let т = тема()
  let р = т.цвет.рис
  import cetz.draw: *
  let m = if m == auto { n } else { m }
  let (x0, x1) = xr
  let (y0, y1) = yr
  let hx = (x1 - x0) / n
  let hy = (y1 - y0) / m
  let база = if цвет == auto { р.грань } else { _цвет(т, цвет) }
  let ребро = if край == auto { 0.3pt + р.линия.transparentize(35%) } else { край }
  if вид == "сетка" {
    let st = 0.5pt + р.линия.transparentize(30%)
    for j in range(m + 1) {
      let y = y0 + hy * j
      line(..range(n * 3 + 1).map(i => { let x = x0 + (x1 - x0) * i / (n * 3); п3(x, y, f(x, y)) }), stroke: st)
    }
    for i in range(n + 1) {
      let x = x0 + hx * i
      line(..range(m * 3 + 1).map(j => { let y = y0 + (y1 - y0) * j / (m * 3); п3(x, y, f(x, y)) }), stroke: st)
    }
    return
  }
  let квадраты = ()
  for i in range(n) {
    for j in range(m) {
      let (xa, xb) = (x0 + hx * i, x0 + hx * (i + 1))
      let (ya, yb) = (y0 + hy * j, y0 + hy * (j + 1))
      let (xc, yc) = ((xa + xb) / 2, (ya + yb) / 2)
      квадраты.push((_глубина(xc, yc, f(xc, yc)), xa, xb, ya, yb))
    }
  }
  for (_, xa, xb, ya, yb) in квадраты.sorted(key: q => q.at(0)) {
    let (za, zb, zc, zd) = (f(xa, ya), f(xb, ya), f(xb, yb), f(xa, yb))
    let грань = if вид == "свет" {
      // нормаль по двум диагоналям ячейки
      let u = (xb - xa, yb - ya, zc - za)
      let v = (xa - xb, yb - ya, zd - zb)
      let nrm = (u.at(1) * v.at(2) - u.at(2) * v.at(1), u.at(2) * v.at(0) - u.at(0) * v.at(2), u.at(0) * v.at(1) - u.at(1) * v.at(0))
      _оттенок(т, база, _свет(nrm))
    } else { база }
    line(п3(xa, ya, za), п3(xb, ya, zb), п3(xb, yb, zc), п3(xa, yb, zd), close: true, fill: грань, stroke: ребро)
  }
})

/// Прямоугольный параллелепипед [xa,xb]×[ya,yb]×[0,h] с видимыми гранями
/// (верх, перед, левый бок). Годится для столбиков интегральной суммы.
#let _столбик(т, xa, xb, ya, yb, h, база, ребро) = {
  import cetz.draw: *
  // левый бок (x = xa), перед (y = ya), верх (z = h)
  line(п3(xa, ya, 0), п3(xa, yb, 0), п3(xa, yb, h), п3(xa, ya, h), close: true, fill: _оттенок(т, база, 0.4), stroke: ребро)
  line(п3(xa, ya, 0), п3(xb, ya, 0), п3(xb, ya, h), п3(xa, ya, h), close: true, fill: _оттенок(т, база, 0.68), stroke: ребро)
  line(п3(xa, ya, h), п3(xb, ya, h), п3(xb, yb, h), п3(xa, yb, h), close: true, fill: _оттенок(т, база, 0.95), stroke: ребро)
}

/// Столбики интегральной суммы: разбиение n × m, высота — значение f
/// в центре ячейки. выделить: ((i, j), ...) — ячейки другим цветом.
#let столбики(f, xr, yr, n: 5, m: 4, зазор: 0.0, выделить: (), цвет: auto, цвет-выделения: "второй") = _draw.get-ctx(ctx => {
  let т = тема()
  let р = т.цвет.рис
  let (x0, x1) = xr
  let (y0, y1) = yr
  let hx = (x1 - x0) / n
  let hy = (y1 - y0) / m
  let база = if цвет == auto { р.грань } else { _цвет(т, цвет) }
  let яркий = _цвет(т, цвет-выделения)
  let ребро = 0.35pt + р.линия.transparentize(20%)
  let все = ()
  for i in range(n) {
    for j in range(m) {
      let xc = x0 + hx * (i + 0.5)
      let yc = y0 + hy * (j + 0.5)
      все.push((_глубина(xc, yc, 0), i, j, xc, yc))
    }
  }
  for (_, i, j, xc, yc) in все.sorted(key: q => q.at(0)) {
    let з = зазор / 2
    _столбик(т, x0 + hx * i + з, x0 + hx * (i + 1) - з, y0 + hy * j + з, y0 + hy * (j + 1) - з, f(xc, yc),
      if (i, j) in выделить { яркий.lighten(if тёмная(т) { 0% } else { 25% }) } else { база }, ребро)
  }
})

/// Плоская фигура в плоскости z = 0 по вершинам (x, y).
#let основание(..точки, цвет: auto, подпись: none) = _draw.get-ctx(ctx => {
  let т = тема()
  import cetz.draw: *
  let pts = точки.pos()
  line(..pts.map(p => п3(p.at(0), p.at(1), 0)), close: true,
    fill: if цвет == auto { т.цвет.рис.заливка } else { _цвет(т, цвет) },
    stroke: 0.7pt + т.цвет.рис.линия)
  if подпись != none {
    let cx = pts.map(p => p.at(0)).sum() / pts.len()
    let cy = pts.map(p => p.at(1)).sum() / pts.len()
    content(п3(cx, cy, 0), подпись)
  }
})

// ═════════════════════════════════════════════════════════════════════════
// Массивы и таблицы значений (ось вниз: строка 0 — верхняя)
// ═════════════════════════════════════════════════════════════════════════

/// Одномерный массив.
/// - выделить: словарь «индекс → цвет» ("акцент", "второй", ... или color);
/// - отрезки: ((l, r, цвет, подпись), ...) — скобка под ячейками l..r включительно;
/// - указатели: ((i, подпись), ...) — стрелка сверху к ячейке i;
/// - начало: номер первой ячейки (0 или 1);
/// - блок: размер блока — границы блоков рисуются жирнее (sqrt-декомпозиция);
/// - своды: значения по блокам — коробки над массивом во всю ширину блока
///   (суммы блоков, максимумы, «ленивые» пометки);
/// - дуги: ((i, j, подпись, цвет), ...) — дуга над массивом между ячейками
///   (обмен, ссылка, пара двух указателей).
#let массив(
  значения, клетка: 0.62, выделить: (:), отрезки: (), указатели: (),
  индексы: true, начало: 0, блок: none, своды: (), дуги: (),
) = _draw.get-ctx(ctx => {
  let т = тема()
  let р = т.цвет.рис
  import cetz.draw: *
  let k = клетка
  for (i, v) in значения.enumerate() {
    let ц = выделить.at(str(i + начало), default: none)
    let заливка = if ц == none { т.цвет.фон } else {
      let c = _цвет(т, ц)
      if тёмная(т) { c.transparentize(65%) } else { c.lighten(72%) }
    }
    rect((i * k, 0), ((i + 1) * k, k), fill: заливка, stroke: 0.6pt + р.линия.transparentize(10%))
    content(((i + 0.5) * k, k / 2), [#v])
    if индексы {
      content(((i + 0.5) * k, -0.02), anchor: "north", text(size: 0.72em, fill: т.цвет.приглушённый, str(i + начало)))
    }
  }
  for (l, r, ц, подпись) in отрезки {
    let c = _цвет(т, ц)
    let y = if индексы { -0.42 } else { -0.12 }
    let (a, b) = ((l - начало) * k + 0.05, (r - начало + 1) * k - 0.05)
    line((a, y + 0.1), (a, y), (b, y), (b, y + 0.1), stroke: 0.9pt + c)
    content(((a + b) / 2, y - 0.04), anchor: "north", text(fill: c, size: 0.85em, подпись))
  }
  for (i, подпись) in указатели {
    let x = (i - начало + 0.5) * k
    line((x, k + 0.45), (x, k + 0.06), stroke: 0.9pt + р.второй, mark: (end: "stealth", fill: р.второй, stroke: 0pt, scale: 0.45))
    content((x, k + 0.45), anchor: "south", text(fill: р.второй, size: 0.85em, подпись))
  }
  // Границы блоков поверх клеток.
  if блок != none {
    let n = значения.len()
    let б = calc.ceil(n / блок)
    for j in range(б + 1) {
      let x = calc.min(j * блок, n) * k
      line((x, 0), (x, k), stroke: 1.4pt + р.линия)
    }
  }
  // Своды блоков: коробка над каждым блоком.
  if своды.len() > 0 {
    let n = значения.len()
    let ширина-блока = if блок != none { блок } else { calc.ceil(n / своды.len()) }
    for (j, v) in своды.enumerate() {
      let a = j * ширина-блока * k
      let b = calc.min((j + 1) * ширина-блока, n) * k
      if b <= a { continue }
      rect((a, k + 0.18), (b, k + 0.18 + k * 0.85),
        fill: if тёмная(т) { р.линия.transparentize(78%) } else { р.линия.lighten(85%) },
        stroke: 0.6pt + р.линия)
      content(((a + b) / 2, k + 0.18 + k * 0.425), text(size: 0.85em, [#v]))
    }
  }
  for д in дуги {
    let (i, j) = (д.at(0), д.at(1))
    let подпись = д.at(2, default: none)
    let c = _цвет(т, д.at(3, default: "второй"))
    let (xi, xj) = ((i - начало + 0.5) * k, (j - начало + 0.5) * k)
    let верх = k + 0.12 + calc.abs(xj - xi) * 0.32
    bezier((xi, k + 0.04), (xj, k + 0.04), ((xi + xj) / 2, верх),
      stroke: 0.9pt + c, mark: (end: "stealth", fill: c, stroke: 0pt, scale: 0.45))
    if подпись != none {
      content(((xi + xj) / 2, верх * 0.88), anchor: "south", text(fill: c, size: 0.8em, подпись))
    }
  }
})

/// Двумерная таблица значений (матрица). Строки — сверху вниз.
/// - прямоугольники: ((r1, c1, r2, c2, цвет), ...) — включительно, в
///   нумерации `начало`; рисуются полупрозрачной заливкой с рамкой;
/// - клетки: словарь "r,c" → цвет — точечная подсветка;
/// - индексы: подписи номеров строк и столбцов;
/// - значения: false — только клетки, без чисел (для мелких схем);
/// - стрелки: ((r1, c1, r2, c2, цвет), ...) — переход из клетки в клетку
///   (ровно то, что нужно для таблиц ДП).
#let сетка(
  м, клетка: 0.55, прямоугольники: (), клетки: (:), индексы: true,
  начало: 1, мелко: false, значения: true, стрелки: (),
) = _draw.get-ctx(ctx => {
  let т = тема()
  let р = т.цвет.рис
  import cetz.draw: *
  let k = клетка
  let n = м.len()
  let w = м.at(0).len()
  let поз(r, c) = (c * k, -r * k) // левый верхний угол клетки (r, c), от 0
  for r in range(n) {
    for c in range(w) {
      let key = str(r + начало) + "," + str(c + начало)
      let ц = клетки.at(key, default: none)
      let (x, y) = поз(r, c)
      rect((x, y), (x + k, y - k), stroke: 0.45pt + р.сетка,
        fill: if ц == none { т.цвет.фон } else { let cc = _цвет(т, ц); if тёмная(т) { cc.transparentize(60%) } else { cc.lighten(70%) } })
    }
  }
  for (r1, c1, r2, c2, ц) in прямоугольники {
    let c = _цвет(т, ц)
    let (xa, ya) = поз(r1 - начало, c1 - начало)
    let (xb, yb) = поз(r2 - начало + 1, c2 - начало + 1)
    rect((xa, ya), (xb, yb), fill: c.transparentize(if тёмная(т) { 70% } else { 78% }), stroke: 1.1pt + c)
  }
  if значения {
    for r in range(n) {
      for c in range(w) {
        let (x, y) = поз(r, c)
        content((x + k / 2, y - k / 2), text(size: if мелко { 0.78em } else { 0.9em }, [#м.at(r).at(c)]))
      }
    }
  }
  for ст in стрелки {
    let (r1, c1, r2, c2) = (ст.at(0), ст.at(1), ст.at(2), ст.at(3))
    let c = _цвет(т, ст.at(4, default: "второй"))
    let (x1, y1) = поз(r1 - начало, c1 - начало)
    let (x2, y2) = поз(r2 - начало, c2 - начало)
    let (a, b) = ((x1 + k / 2, y1 - k / 2), (x2 + k / 2, y2 - k / 2))
    let (dx, dy) = (b.at(0) - a.at(0), b.at(1) - a.at(1))
    let d = calc.sqrt(dx * dx + dy * dy)
    let отступ = k * 0.34
    line(
      (a.at(0) + dx / d * отступ, a.at(1) + dy / d * отступ),
      (b.at(0) - dx / d * отступ, b.at(1) - dy / d * отступ),
      stroke: 1pt + c, mark: (end: "stealth", fill: c, stroke: 0pt, scale: 0.4),
    )
  }
  if индексы {
    for c in range(w) {
      let (x, y) = поз(0, c)
      content((x + k / 2, 0.04), anchor: "south", text(size: 0.68em, fill: т.цвет.приглушённый, str(c + начало)))
    }
    for r in range(n) {
      let (x, y) = поз(r, 0)
      content((-0.06, y - k / 2), anchor: "east", text(size: 0.68em, fill: т.цвет.приглушённый, str(r + начало)))
    }
  }
})

// ═════════════════════════════════════════════════════════════════════════
// Графы (ось y вверх)
// ═════════════════════════════════════════════════════════════════════════

/// Граф.
/// - вершины: словарь имя → (x, y);
/// - рёбра: массив (u, v) | (u, v, стиль) | (u, v, стиль, подпись);
///   стили: "обычное", "выделено", "второе", "тусклое", "пунктир";
/// - выделить: имена вершин с заливкой акцентом; тусклые: приглушённые;
/// - подписи: словарь имя → содержимое (по умолчанию — само имя);
/// - метки: словарь имя → (содержимое, якорь) — пометка рядом с вершиной;
/// - форма: "круг" или "прямоугольник" (для узлов с двумя строками —
///   ключ и приоритет декартова дерева, состояние автомата и т. п.);
///   размер прямоугольника — `размер: (ширина, высота)`.
/// Встречные рёбра u→v и v→u в ориентированном графе разводятся сами.
#let граф(
  вершины, рёбра, ориентированный: false, радиус: 0.27, выделить: (),
  тусклые: (), подписи: (:), метки: (:), форма: "круг", размер: (0.72, 0.46),
) = _draw.get-ctx(ctx => {
  let т = тема()
  let р = т.цвет.рис
  import cetz.draw: *
  let пары = рёбра.map(e => (e.at(0), e.at(1)))
  for e in рёбра {
    let (u, v) = (e.at(0), e.at(1))
    let стиль = e.at(2, default: "обычное")
    let подпись = e.at(3, default: none)
    let (x1, y1) = вершины.at(u)
    let (x2, y2) = вершины.at(v)
    let (dx, dy) = (x2 - x1, y2 - y1)
    let d = calc.sqrt(dx * dx + dy * dy)
    let (ux, uy) = (dx / d, dy / d)
    // сдвиг встречных рёбер
    let s = if ориентированный and (v, u) in пары { 0.09 } else { 0 }
    let (ox, oy) = (-uy * s, ux * s)
    // до края узла: у круга — радиус, у прямоугольника — пересечение со стороной
    let край = if форма == "прямоугольник" {
      let (a, b) = (размер.at(0) / 2, размер.at(1) / 2)
      calc.min(
        if calc.abs(ux) < 0.001 { 1e9 } else { a / calc.abs(ux) },
        if calc.abs(uy) < 0.001 { 1e9 } else { b / calc.abs(uy) },
      )
    } else { радиус }
    let p = (x1 + ux * край + ox, y1 + uy * край + oy)
    let q = (x2 - ux * край + ox, y2 - uy * край + oy)
    let (ц, толщина, штрих) = if стиль == "выделено" { (р.линия, 1.5pt, none) } else if стиль == "второе" { (р.второй, 1.5pt, none) } else if стиль == "тусклое" { (т.цвет.приглушённый.transparentize(45%), 0.6pt, none) } else if стиль == "пунктир" { (т.цвет.приглушённый, 0.7pt, "dashed") } else { (р.ось.transparentize(15%), 0.8pt, none) }
    line(p, q, stroke: (paint: ц, thickness: толщина, dash: штрих),
      mark: if ориентированный { (end: "stealth", fill: ц, stroke: 0pt, scale: 0.55) } else { none })
    if подпись != none {
      let m = ((p.at(0) + q.at(0)) / 2 - uy * 0.16, (p.at(1) + q.at(1)) / 2 + ux * 0.16)
      content(m, box(fill: т.цвет.фон, inset: 1.2pt, radius: 1.5pt, text(size: 0.78em, fill: if стиль in ("выделено", "второе") { ц } else { т.цвет.текст }, подпись)))
    }
  }
  for (имя, pos) in вершины {
    let выд = имя in выделить
    let тус = имя in тусклые
    let заливка = if выд { р.линия } else { т.цвет.фон }
    let обводка = (if тус { 0.6pt } else { 0.9pt }) + (if тус { т.цвет.приглушённый.transparentize(40%) } else { р.линия })
    if форма == "прямоугольник" {
      let (a, b) = (размер.at(0) / 2, размер.at(1) / 2)
      rect((pos.at(0) - a, pos.at(1) - b), (pos.at(0) + a, pos.at(1) + b),
        fill: заливка, stroke: обводка, radius: 0.06)
    } else {
      circle(pos, radius: радиус, fill: заливка, stroke: обводка)
    }
    content(pos, text(size: 0.85em, weight: if выд { "bold" } else { "regular" },
      fill: if выд { т.цвет.фон } else if тус { т.цвет.приглушённый } else { т.цвет.текст },
      подписи.at(имя, default: имя)))
    if имя in метки {
      let (м, якорь) = метки.at(имя)
      content(pos, anchor: якорь, text(size: 0.75em, fill: р.второй, м),
        padding: (if форма == "прямоугольник" { размер.at(0) / 2 } else { радиус }) * 1cm + 2pt)
    }
  }
})

/// Линии уровня f(x, y) = c методом marching squares: та самая картинка,
/// по которой студент понимает форму поверхности, глядя сверху.
#let линии-уровня(f, уровни, xr: (-2, 2), yr: (-2, 2), n: 40, цвет: auto) = _draw.get-ctx(ctx => {
  let т = тема()
  let ц = _цвет(т, цвет)
  import cetz.draw: *
  let (x0, x1) = xr
  let (y0, y1) = yr
  let dx = (x1 - x0) / n
  let dy = (y1 - y0) / n
  for c in уровни {
    for i in range(n) {
      for j in range(n) {
        let (xa, ya) = (x0 + dx * i, y0 + dy * j)
        let угол = ((xa, ya), (xa + dx, ya), (xa + dx, ya + dy), (xa, ya + dy))
        let зн = угол.map(p => f(p.at(0), p.at(1)) - c)
        let точки = ()
        for k in range(4) {
          let (a, b) = (зн.at(k), зн.at(calc.rem(k + 1, 4)))
          if (a < 0) != (b < 0) {
            let t = a / (a - b)
            let (pa, pb) = (угол.at(k), угол.at(calc.rem(k + 1, 4)))
            точки.push((pa.at(0) + (pb.at(0) - pa.at(0)) * t, pa.at(1) + (pb.at(1) - pa.at(1)) * t))
          }
        }
        if точки.len() == 2 { line(точки.at(0), точки.at(1), stroke: 0.7pt + ц) }
      }
    }
  }
})

/// Тело вращения: горизонтальные «обручи» радиуса r(z) и силуэт. Обручи —
/// это буквально линии r = const, то есть идея цилиндрических координат.
#let вращение(r, zmin: 0, zmax: 2, обручей: 8, n: 40, цвет: auto) = _draw.get-ctx(ctx => {
  let т = тема()
  let ц = _цвет(т, цвет)
  import cetz.draw: *
  for k in range(обручей + 1) {
    let z = zmin + (zmax - zmin) * k / обручей
    let rr = r(z)
    if rr > 0.001 {
      line(..range(n + 1).map(i => {
        let t = 2 * calc.pi * i / n
        п3(rr * calc.cos(t), rr * calc.sin(t), z)
      }), close: true, stroke: 0.5pt + ц.transparentize(35%))
    }
  }
  for s in (1, -1) {
    line(..range(n + 1).map(i => {
      let z = zmin + (zmax - zmin) * i / n
      п3(s * r(z), 0, z)
    }), stroke: 1pt + ц)
  }
})

/// Сечение поверхности z = f(x, y) плоскостью y = y0 — «слой», из которых
/// набирается объём.
#let сечение(f, y0, xr: (0, 3), n: 40, цвет: "второй", заливка: true) = _draw.get-ctx(ctx => {
  let т = тема()
  let ц = _цвет(т, цвет)
  import cetz.draw: *
  let (a, b) = xr
  let верх = range(n + 1).map(i => { let x = a + (b - a) * i / n; п3(x, y0, f(x, y0)) })
  if заливка {
    line(..верх, п3(b, y0, 0), п3(a, y0, 0), close: true,
      fill: ц.transparentize(if тёмная(т) { 75% } else { 85% }), stroke: none)
  }
  line(..верх, stroke: 1.1pt + ц)
})

/// Раскладка дерева по уровням: корень сверху, дети — ниже. Каждому
/// поддереву выделяется полоса по его ширине — соседи не наезжают.
#let раскладка-дерево(рёбра, корень, dx: 0.9, dy: 0.95) = {
  let дети = (:)
  for e in рёбра {
    let (u, v) = (e.at(0), e.at(1))
    дети.insert(u, дети.at(u, default: ()) + (v,))
  }
  let ширина(v) = {
    let д = дети.at(v, default: ())
    if д.len() == 0 { 1 } else { д.map(ширина).sum() }
  }
  let поз = (:)
  let разложить(v, x0, глуб) = {
    let д = дети.at(v, default: ())
    let w = ширина(v)
    let итог = ((v, ((x0 + w / 2 - 0.5) * dx, -глуб * dy)),)
    let x = x0
    for c in д {
      итог += разложить(c, x, глуб + 1)
      x += ширина(c)
    }
    итог
  }
  for (v, p) in разложить(корень, 0, 0) { поз.insert(v, p) }
  поз
}

/// Раскладка бинарного дерева: x — порядок обхода «слева направо»
/// (in-order), y — глубина. Именно так рисуют деревья поиска, декартовы
/// деревья (treap) и деревья отрезков: ключи идут слева направо.
/// дети: словарь имя → (левый, right) — `none` там, где ребёнка нет.
#let раскладка-бинарное(дети, корень, dx: 0.85, dy: 0.95) = {
  // in-order: сначала левое поддерево, потом сам, потом правое.
  // Счётчик передаётся и возвращается — замыкания в typst не меняют
  // переменные внешней области.
  let обойти(v, глубина, i) = {
    if v == none { return ((), i) }
    let (л, п) = дети.at(v, default: (none, none))
    let (слева, i2) = обойти(л, глубина + 1, i)
    let сам = ((v, (i2 * dx, -глубина * dy)),)
    let (справа, i3) = обойти(п, глубина + 1, i2 + 1)
    (слева + сам + справа, i3)
  }
  let (пары, _) = обойти(корень, 0, 0)
  let поз = (:)
  for (v, p) in пары { поз.insert(v, p) }
  поз
}

/// Рёбра бинарного дерева по словарю детей — в том виде, который ждёт `граф`.
#let рёбра-бинарного(дети, стиль: "обычное") = {
  let рёбра = ()
  for (v, (л, п)) in дети {
    if л != none { рёбра.push((v, л, стиль)) }
    if п != none { рёбра.push((v, п, стиль)) }
  }
  рёбра
}

/// Раскладка по окружности (первая вершина сверху, дальше по часовой).
#let раскладка-круг(имена, радиус: 1.3, начало: 90deg) = {
  let n = имена.len()
  let поз = (:)
  for (i, v) in имена.enumerate() {
    let a = начало - 360deg * i / n
    поз.insert(v, (радиус * calc.cos(a), радиус * calc.sin(a)))
  }
  поз
}
