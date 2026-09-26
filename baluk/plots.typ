// Интерактивные рисунки: график с ползунками и поверхность с вращением.
//
//   #fig(interactive-plot("a * calc.sin(b * x)", -5, 5,
//     params: (a: (from: 0, to: 3, value: 1), b: (from: 0.5, to: 4, step: 0.1, value: 1)),
//     y: (-3, 3)), [Амплитуда и частота синусоиды])
//
// Формула — строка с кодом Typst из подмножества (см. `_parse`): её считает
// и Typst (кадр для PDF и для страницы без JS), и клиент приложения
// (`app/src/lib/plot/`) — разбор и вычисление там повторяют здешние
// ПОСТРОЧНО. Меняешь одно — меняй и другое; сверка — фикстура
// `tests/vault/Рисунки/Интерактив.typ` (её снимок читает Vitest).
//
// В HTML рисунок — `div.k-plot` с `data-k-plot` (JSON: формулы, диапазоны,
// параметры) и обычным кадром `canvas` внутри; клиент прячет кадр и рисует
// живой рисунок. В PDF — кадр при значениях по умолчанию и подпись с ними.
//
// Свой разбор, а не `eval`: `eval` падает с ошибкой сборки на делении на
// ноль, `calc.sqrt` от отрицательного и т. п. — а на графике это просто
// разрыв кривой.

#import "@preview/cetz:0.4.2"
#import "theme.typ": current-theme, fig-shades
#import "web.typ": is-web, elem
#import "figures.typ": canvas

#let _draw = cetz.draw

// ── Разбор формулы ───────────────────────────────────────────────────────

#let _functions = (
  sin: 1, cos: 1, tan: 1, asin: 1, acos: 1, atan: 1, exp: 1, ln: 1, log: 1,
  sqrt: 1, abs: 1, floor: 1, ceil: 1, pow: 2,
)
#let _constants = (pi: calc.pi, e: calc.e)

#let _token-re = regex("\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|[\p{L}_][\p{L}\p{N}_]*(?:\.[\p{L}_][\p{L}\p{N}_]*)?|[-+*/(),]|\S")

/// Лексемы формулы (пробелы пропускаются).
#let _tokens(src) = src.matches(_token-re).map(arrow-mark => arrow-mark.text)

#let _fail(src, txt) = panic("формула «" + src + "»: " + txt)

/// Разбор выражения уровня `level` с лексемы `i`: (узел, следующая лексема).
/// Уровни: 0 — сумма, 1 — произведение, 2 — унарный знак и операнд.
/// Приоритеты как в Typst: унарный минус сильнее `*` и `/`.
#let _parse(toks, i, level, names, src) = {
  let ntok = toks.len()
  if level < 2 {
    let ops = if level == 0 { ("+", "-") } else { ("*", "/") }
    let (cur, i) = _parse(toks, i, level + 1, names, src)
    while i < ntok and toks.at(i) in ops {
      let (prm, j) = _parse(toks, i + 1, level + 1, names, src)
      cur = (k: "op", o: toks.at(i), a: cur, b: prm)
      i = j
    }
    return (cur, i)
  }
  if i >= ntok { _fail(src, "формула оборвалась") }
  let cur = toks.at(i)
  if cur == "-" or cur == "+" {
    let (operand, j) = _parse(toks, i + 1, 2, names, src)
    return (if cur == "-" { (k: "neg", a: operand) } else { operand }, j)
  }
  if cur == "(" {
    let (operand, j) = _parse(toks, i + 1, 0, names, src)
    if j >= ntok or toks.at(j) != ")" { _fail(src, "не хватает «)»") }
    return (operand, j + 1)
  }
  if cur.match(regex("^\d")) != none { return ((k: "n", v: float(cur)), i + 1) }
  if cur.starts-with("calc.") {
    let name = cur.slice(5)
    if name in _constants { return ((k: "n", v: _constants.at(name)), i + 1) }
    if name not in _functions {
      _fail(src, "нет функции «" + cur + "»; есть calc." + _functions.keys().join(", calc.") + ", calc.pi, calc.e")
    }
    if i + 1 >= ntok or toks.at(i + 1) != "(" { _fail(src, "после «" + cur + "» нужны скобки") }
    let fargs = ()
    let j = i + 2
    let (operand, j) = _parse(toks, j, 0, names, src)
    fargs.push(operand)
    while j < ntok and toks.at(j) == "," {
      let (operand, k) = _parse(toks, j + 1, 0, names, src)
      fargs.push(operand)
      j = k
    }
    if j >= ntok or toks.at(j) != ")" { _fail(src, "не хватает «)» после аргументов «" + cur + "»") }
    if fargs.len() != _functions.at(name) {
      _fail(src, "«" + cur + "» принимает аргументов: " + str(_functions.at(name)))
    }
    return ((k: "f", n: name, args: fargs), j + 1)
  }
  if cur in names { return ((k: "v", n: cur), i + 1) }
  if cur == "^" { _fail(src, "степень пишется calc.pow(x, 2)") }
  if cur.match(regex("^[\p{L}_]")) != none {
    _fail(src, "неизвестное имя «" + cur + "»; можно: " + names.join(", ") + ", calc.…")
  }
  _fail(src, "непонятный знак «" + cur + "»")
}

/// Разбирает формулу с переменными `names` (x, y, параметры).
#let _formula(src, names) = {
  let toks = _tokens(src)
  if toks.len() == 0 { _fail(src, "пустая формула") }
  let (node, i) = _parse(toks, 0, 0, names, src)
  if i < toks.len() and toks.at(i) == "^" { _fail(src, "степень пишется calc.pow(x, 2)") }
  if i < toks.len() { _fail(src, "лишнее «" + toks.at(i) + "»") }
  node
}

// Число годно: не none, не NaN, не бесконечность.
#let _valid(v) = v != none and v == v and calc.abs(v) < 1e300

/// Значение узла при переменных `vars` (словарь). Вне области определения
/// (деление на ноль, корень из отрицательного…) — none.
#let _eval(nd, vars) = {
  let k = nd.k
  if k == "n" { return nd.v }
  if k == "v" { return vars.at(nd.n) }
  if k == "neg" {
    let a = _eval(nd.a, vars)
    return if a == none { none } else { -a }
  }
  if k == "op" {
    let a = _eval(nd.a, vars)
    if a == none { return none }
    let b = _eval(nd.b, vars)
    if b == none { return none }
    let r = if nd.o == "+" { a + b } else if nd.o == "-" { a - b } else if nd.o == "*" { a * b } else if b == 0 { none } else { a / b }
    return if _valid(r) { r } else { none }
  }
  // функция
  let a = _eval(nd.args.at(0), vars)
  if a == none { return none }
  let fname = nd.n
  let r = if fname == "sin" { calc.sin(a) } else if fname == "cos" { calc.cos(a) } else if fname == "tan" { calc.tan(a) }
  else if fname == "asin" { if calc.abs(a) > 1 { none } else { calc.asin(a).rad() } }
  else if fname == "acos" { if calc.abs(a) > 1 { none } else { calc.acos(a).rad() } }
  else if fname == "atan" { calc.atan(a).rad() }
  else if fname == "exp" { if a > 700 { none } else { calc.exp(a) } }
  else if fname == "ln" { if a <= 0 { none } else { calc.ln(a) } }
  else if fname == "log" { if a <= 0 { none } else { calc.log(a) } }
  else if fname == "sqrt" { if a < 0 { none } else { calc.sqrt(a) } }
  else if fname == "abs" { calc.abs(a) }
  else if fname == "floor" { float(calc.floor(a)) }
  else if fname == "ceil" { float(calc.ceil(a)) }
  else if fname == "pow" {
    let b = _eval(nd.args.at(1), vars)
    if b == none { none }
    else if a == 0 { if b > 0 { 0.0 } else if b == 0 { 1.0 } else { none } }
    else if a < 0 and calc.fract(b) != 0 { none }
    else if b * calc.ln(calc.abs(a)) > 700 { none }
    else { calc.pow(a, b) }
  }
  if _valid(r) { float(r) } else { none }
}

// ── Общее: параметры, деления осей, числа ────────────────────────────────

/// Параметры → массив словарей (name, min, max, step, value) для JSON и PDF.
#let _params(prm, forbidden) = prm.pairs().map(((name, opts)) => {
  if name in forbidden or name.match(regex("^[\p{L}_][\p{L}\p{N}_]*$")) == none {
    panic("параметр «" + name + "»: нужно имя вроде a, k, ω (не " + forbidden.join(", ") + ")")
  }
  let from = float(opts.from)
  let to = float(opts.to)
  if not (to > from) { panic("параметр «" + name + "»: нужно from < to") }
  let step = float(opts.at("step", default: (to - from) / 100))
  let value = float(opts.at("value", default: from))
  (name: name, min: from, max: to, step: step, value: value)
})

/// Шаг делений: 1, 2 или 5 × 10^k, около пяти делений на диапазон.
#let _tick-step(d) = {
  let s = d / 5
  let p = calc.pow(10.0, calc.floor(calc.log(s)))
  let m = s / p
  (if m < 1.5 { 1 } else if m < 3.5 { 2 } else if m < 7.5 { 5 } else { 10 }) * p
}

/// Число для подписи: знаков после запятой — как у шага, минус типографский.
/// trim: убрать хвостовые нули («1.0» → «1») — для подписи значений.
#let _num(v, step, trim: false) = {
  let digits = calc.max(0, -calc.floor(calc.log(step) + 1e-9))
  let r = calc.round(v, digits: digits)
  if calc.abs(r) < step / 1e6 { r = 0 }
  let src = str(calc.abs(r))
  if digits > 0 {
    let (col, item) = if src.contains(".") { src.split(".") } else { (src, "") }
    src = col + "." + item + "0" * (digits - item.len())
  }
  if trim and src.contains(".") { src = src.trim("0", at: end).trim(".", at: end) }
  (if r < 0 { "−" } else { "" }) + src
}

/// Деления в [a, b] с шагом `step`.
#let _divisions(a, b, step) = {
  let i0 = calc.ceil(a / step - 1e-9)
  let i1 = calc.floor(b / step + 1e-9)
  range(i0, i1 + 1).map(i => i * step)
}

/// Подпись значений параметров: «a = 1, b = 0.5».
#let _values-label(plist) = plist.map(prm => prm.name + " = " + _num(prm.value, prm.step, trim: true)).join(", ")

/// Словарь переменных для вычисления: параметры при значениях по умолчанию.
#let _env(plist) = plist.map(prm => (prm.name, prm.value)).to-dict()

#let _json(data) = json.encode(data, pretty: false)

/// Цвет рисунка по имени: линия, второй, третий, грань, акцент.
#let _fig-color(theme, name) = if name == "accent" { theme.color.accent } else { theme.color.fig.at(name, default: theme.color.fig.line) }

/// Имя цвета → CSS-переменная (в HTML цвета задаёт тема, не разметка).
#let _css-color(name) = if name == "accent" { "var(--k-accent)" } else {
  "var(--k-fig-" + (line: "line", second: "second", third: "third", face: "face").at(name, default: "line") + ")"
}

/// Обёртка: в HTML — div.k-plot с JSON и кадром внутри, в PDF — кадр и подпись.
/// легенда — кривые с подписями (под рисунком: на поле подпись наезжает на оси).
#let _figure(data, frame, plist, legend: ()) = context {
  let theme = current-theme()
  let label = if plist.len() > 0 { _values-label(plist) }
  let glyph(kk) = if kk.dashed { "- - " } else { "— " }
  if is-web() {
    elem("div", "k-plot", ..("data-k-plot": _json(data)), {
      frame
      if legend.len() > 0 {
        elem("div", "k-plot-legend", legend.map(crv => elem("span", "k-plot-key", ..(style: "color: " + _css-color(crv.color)), glyph(crv) + crv.label)).join())
      }
      if label != none { elem("div", "k-plot-values", label) }
    })
  } else {
    block(breakable: false, {
      frame
      set text(size: theme.size.small)
      if legend.len() > 0 {
        v(0.3em, weak: true)
        align(center, legend.map(crv => text(fill: _fig-color(theme, crv.color), glyph(crv) + emph(crv.label))).join(h(1.2em)))
      }
      if label != none {
        v(0.2em, weak: true)
        align(center, text(fill: theme.color.muted, label))
      }
    })
  }
}

// ── 2D ───────────────────────────────────────────────────────────────────

/// Участки кривой внутри полосы y ∈ [y0, y1]: точки — (x, y) или (x, none).
/// Отрезок, пересекающий полосу, обрезается по её краю; отрезок между
/// точками по разные стороны от полосы — разрыв (асимптота, как у tg x).
#let _segments(pts, y0, y1) = {
  let all = ()
  let cur-seg = ()
  for i in range(pts.len() - 1) {
    let (xa, ya) = pts.at(i)
    let (xb, yb) = pts.at(i + 1)
    if ya == none or yb == none or (ya > y1 and yb < y0) or (ya < y0 and yb > y1) {
      if cur-seg.len() > 1 { all.push(cur-seg) }
      cur-seg = ()
      continue
    }
    // обрезка отрезка по полосе: параметры входа и выхода t ∈ [0, 1]
    let (t0, t1) = (0.0, 1.0)
    for (bound, above) in ((y0, false), (y1, true)) {
      let out-a = if above { ya > bound } else { ya < bound }
      let out-b = if above { yb > bound } else { yb < bound }
      if out-a and out-b { t0 = 2.0 }
      else if out-a { t0 = calc.max(t0, (bound - ya) / (yb - ya)) }
      else if out-b { t1 = calc.min(t1, (bound - ya) / (yb - ya)) }
    }
    if t0 > t1 {
      if cur-seg.len() > 1 { all.push(cur-seg) }
      cur-seg = ()
      continue
    }
    let p(t) = (xa + (xb - xa) * t, ya + (yb - ya) * t)
    if cur-seg.len() == 0 or t0 > 0 {
      if cur-seg.len() > 1 { all.push(cur-seg) }
      cur-seg = (p(t0),)
    }
    cur-seg.push(p(t1))
    if t1 < 1 {
      if cur-seg.len() > 1 { all.push(cur-seg) }
      cur-seg = ()
    }
  }
  if cur-seg.len() > 1 { all.push(cur-seg) }
  all
}

/// Точки кривой: n + 1 точка на [a, b].
#let _sample(ff, a, b, n, vars) = range(n + 1).map(i => {
  let x = a + (b - a) * i / n
  (x, _eval(ff, vars + (x: x)))
})

/// Диапазон y по точкам: без выбросов (2 % с каждой стороны) и с полями.
#let _range-of(values) = {
  let zz = values.filter(v => v != none).sorted()
  if zz.len() == 0 { return (-1.0, 1.0) }
  let k = calc.floor(zz.len() * 0.02)
  let (lo, hi) = (zz.at(k), zz.at(zz.len() - 1 - k))
  if hi - lo < 1e-9 { return (lo - 1, hi + 1) }
  let slack = (hi - lo) * 0.08
  (lo - slack, hi + slack)
}

/// Кривые: строка, словарь (f:, цвет:, подпись:, пунктир:) или массив таких.
#let _curves(spec) = {
  let entries = if type(spec) == array { spec } else { (spec,) }
  let colors = ("line", "second", "third")
  entries.enumerate().map(((i, crv)) => {
    let crv = if type(crv) == str { (f: crv) } else { crv }
    (
      f: crv.f,
      color: crv.at("color", default: colors.at(calc.rem(i, 3))),
      label: crv.at("label", default: none),
      dashed: crv.at("dashed", default: false),
    )
  })
}

/// График y = f(x) с ползунками параметров.
/// formula: "a * calc.sin(x)" | (f: "…", color: "second", label: "sin", dashed: true) | массив.
/// params: (a: (from: 0, to: 2, step: 0.1, value: 1), …) — step по умолчанию 1/100 диапазона,
///   знач — от.
/// y: (y0, y1) или auto (по кадру при значениях по умолчанию; лучше задать).
/// ширина, высота — размер поля графика в см.
#let interactive-plot(formula, a, b, params: (:), y: auto, labels: ("x", "y"), width: 8, height: 5, n: 160) = {
  let plist = _params(params, ("x", "y", "calc"))
  let names = ("x",) + plist.map(prm => prm.name)
  let curves = _curves(formula)
  let trees = curves.map(crv => _formula(crv.f, names))
  let vars = _env(plist)
  let (a, b) = (float(a), float(b))
  if not (b > a) { panic("interactive-plot: нужно a < b") }
  let points = trees.map(item => _sample(item, a, b, n, vars))
  let (y0, y1) = if y == auto { _range-of(points.join().map(theme => theme.at(1))) } else { y.map(float) }
  if not (y1 > y0) { panic("interactive-plot: нужно y0 < y1") }
  let data = (
    kind: "2d", curves: curves, x: (a, b), y: (y0, y1), params: plist,
    labels: labels, width: width, height: height,
  )
  let (sx, sy) = (width / (b - a), height / (y1 - y0))
  let to-canvas(p) = ((p.at(0) - a) * sx, (p.at(1) - y0) * sy)
  let frame = canvas(theme => {
    import cetz.draw: *
    let fc = theme.color.fig
    // оси: через ноль, если он в диапазоне, иначе по краю
    let ox = if a <= 0 and 0 <= b { 0.0 } else { a }
    let oy = if y0 <= 0 and 0 <= y1 { 0.0 } else { y0 }
    let (hx, hy) = (_tick-step(b - a), _tick-step(y1 - y0))
    let arrow-mark = (end: "stealth", fill: fc.axis, stroke: 0pt, scale: 0.55)
    let axis-stroke = 0.6pt + fc.axis
    let matrix-cells = 0.3pt + fc.grid
    for x in _divisions(a, b, hx) {
      line(to-canvas((x, y0)), to-canvas((x, y1)), stroke: matrix-cells)
    }
    for yy in _divisions(y0, y1, hy) {
      line(to-canvas((a, yy)), to-canvas((b, yy)), stroke: matrix-cells)
    }
    line(to-canvas((a, oy)), to-canvas((b, oy)), stroke: axis-stroke, mark: arrow-mark)
    line(to-canvas((ox, y0)), to-canvas((ox, y1)), stroke: axis-stroke, mark: arrow-mark)
    let small-text(src) = text(size: 0.85em, fill: fc.axis, src)
    for x in _divisions(a, b, hx) {
      if calc.abs(x - ox) > hx / 2 and a + hx / 3 < x and x < b - hx / 3 {
        content(to-canvas((x, oy)), anchor: "north", padding: 2pt, small-text(_num(x, hx)))
      }
    }
    for yy in _divisions(y0, y1, hy) {
      if calc.abs(yy - oy) > hy / 2 and y0 + hy / 3 < yy and yy < y1 - hy / 3 {
        content(to-canvas((ox, yy)), anchor: "east", padding: 2pt, small-text(_num(yy, hy)))
      }
    }
    content(to-canvas((b, oy)), anchor: "north-east", padding: (top: 7pt), emph(labels.at(0)))
    content(to-canvas((ox, y1)), anchor: "north-west", padding: (left: 5pt), emph(labels.at(1)))
    for (crv, cpts) in curves.zip(points) {
      let col = _fig-color(theme, crv.color)
      let segs = _segments(cpts, y0, y1)
      for seg in segs {
        line(..seg.map(to-canvas), stroke: (paint: col, thickness: 1.1pt, join: "round", dash: if crv.dashed { "dashed" } else { none }))
      }
    }
  })
  _figure(data, frame, plist, legend: curves.filter(crv => crv.label != none))
}

// ── 3D ───────────────────────────────────────────────────────────────────
//
// Поверхность нормируется в коробку [-1, 1] × [-1, 1] × [-0.7, 0.7] и
// рисуется ортогонально: поворот вокруг вертикали на угол `rotation`
// (азимут), потом наклон на `tilt`. Клиент вращает с того же вида.

#let _Z = 0.7

/// Экранные координаты и глубина (больше — ближе к зрителю) точки коробки.
#let _project(p, θ, φ) = {
  let (x, y, z) = p
  let xr = x * calc.cos(θ) - y * calc.sin(θ)
  let yr = x * calc.sin(θ) + y * calc.cos(θ)
  (xr, z * calc.cos(φ) + yr * calc.sin(φ), -yr * calc.cos(φ) + z * calc.sin(φ))
}

/// Освещённость 0..1 грани с нормалью n (в осях экрана: u, v, глубина).
/// Свет — от зрителя слева сверху; обе стороны грани освещены одинаково.
#let _illum(n) = {
  let (a, b, c) = n
  let len = calc.sqrt(a * a + b * b + c * c)
  if len == 0 { return 0.5 }
  let (lu, lv, ld) = (-0.38, 0.62, 0.69)
  calc.abs(a * lu + b * lv + c * ld) / len
}

/// Цвет грани: смесь тени и света темы (как `--k-fig-*-dark/-light` в CSS).
#let _tone(theme, base, k) = {
  let (shadow, light) = fig-shades(theme, base)
  color.mix((shadow, 100% - k * 100%), (light, k * 100%), space: rgb)
}

/// Поверхность z = f(x, y) с вращением перетаскиванием и ползунками.
/// formula: "calc.sin(a * x) * calc.cos(y)"; xr, yr — диапазоны (x0, x1).
/// z: (z0, z1) или auto (по кадру при значениях по умолчанию).
/// style: "shaded" — грани со светотенью, "wire" — каркас.
/// color: "face" (по умолчанию), "line", "second", "third".
/// поворот, наклон — начальный вид (в градусах).
#let interactive-surface(formula, xr, yr, params: (:), z: auto, labels: ("x", "y", "z"), n: 24, style: "shaded", color: "face", rotation: -30, tilt: 28, size: 3) = {
  let plist = _params(params, ("x", "y", "calc"))
  let names = ("x", "y") + plist.map(prm => prm.name)
  let ast = _formula(formula, names)
  let vars = _env(plist)
  let (x0, x1) = xr.map(float)
  let (y0, y1) = yr.map(float)
  if not (x1 > x0 and y1 > y0) { panic("interactive-surface: нужно x0 < x1 и y0 < y1") }
  if color not in ("face", "line", "second", "third") { panic("interactive-surface: color — face, line, second или third") }
  // значения в узлах сетки (n + 1) × (n + 1)
  let grid-vals = range(n + 1).map(i => range(n + 1).map(j => {
    let x = x0 + (x1 - x0) * i / n
    let y = y0 + (y1 - y0) * j / n
    _eval(ast, vars + (x: x, y: y))
  }))
  let (z0, z1) = if z == auto {
    let zs = grid-vals.join().filter(v => v != none)
    if zs.len() == 0 { (-1.0, 1.0) } else {
      let (lo, hi) = (calc.min(..zs), calc.max(..zs))
      if hi - lo < 1e-9 { (lo - 1, hi + 1) } else { (lo, hi) }
    }
  } else { z.map(float) }
  if not (z1 > z0) { panic("interactive-surface: нужно z0 < z1") }
  let data = (
    kind: "3d", f: formula, x: (x0, x1), y: (y0, y1), z: (z0, z1), params: plist,
    labels: labels, n: n, style: style, color: color, view: (rotation, tilt), size: size,
  )
  let (θ, φ) = (rotation * 1deg, tilt * 1deg)
  let S = size / 2
  // узел → точка коробки
  let to-box(i, j, v) = (-1 + 2 * i / n, -1 + 2 * j / n, -_Z + 2 * _Z * (v - z0) / (z1 - z0))
  let to-screen(p) = { let (u, vv, _) = _project(p, θ, φ); (u * S, vv * S) }
  let frame = canvas(theme => {
    import cetz.draw: *
    let fc = theme.color.fig
    let base = fc.at(color)
    let axis = 0.5pt + fc.axis
    // пол коробки и вертикальное ребро в дальнем углу
    let corners = ((-1, -1), (1, -1), (1, 1), (-1, 1))
    let far = corners.sorted(key: ((x, y)) => _project((x, y, 0), θ, φ).at(2)).first()
    line(..corners.map(((x, y)) => to-screen((x, y, -_Z))), close: true, stroke: axis)
    line(to-screen((..far, -_Z)), to-screen((..far, _Z)), stroke: axis)
    // грани: дальние раньше (алгоритм художника); точки вне [z0, z1] — не рисуем
    let faces = ()
    for i in range(n) {
      for j in range(n) {
        let quad = (grid-vals.at(i).at(j), grid-vals.at(i + 1).at(j), grid-vals.at(i + 1).at(j + 1), grid-vals.at(i).at(j + 1))
        if quad.any(v => v == none or v < z0 or v > z1) { continue }
        let p = (to-box(i, j, quad.at(0)), to-box(i + 1, j, quad.at(1)), to-box(i + 1, j + 1, quad.at(2)), to-box(i, j + 1, quad.at(3)))
        let proj = p.map(q => _project(q, θ, φ))
        let dep = proj.map(q => q.at(2)).sum() / 4
        faces.push((dep, proj))
      }
    }
    let edge = 0.3pt + fc.line.transparentize(35%)
    for (_, proj) in faces.sorted(key: ax => ax.at(0)) {
      let pts = proj.map(q => (q.at(0) * S, q.at(1) * S))
      if style == "wire" {
        line(..pts, close: true, stroke: 0.5pt + base)
      } else {
        // нормаль по диагоналям грани (в осях экрана)
        let d1 = range(3).map(c => proj.at(2).at(c) - proj.at(0).at(c))
        let d2 = range(3).map(c => proj.at(3).at(c) - proj.at(1).at(c))
        let nrm = (
          d1.at(1) * d2.at(2) - d1.at(2) * d2.at(1),
          d1.at(2) * d2.at(0) - d1.at(0) * d2.at(2),
          d1.at(0) * d2.at(1) - d1.at(1) * d2.at(0),
        )
        line(..pts, close: true, fill: _tone(theme, base, _illum(nrm)), stroke: edge)
      }
    }
    // подписи осей — поверх граней
    let small-text(src) = text(fill: fc.axis, emph(src))
    // x — у ближнего к зрителю ребра вдоль x, y — у ближнего вдоль y
    let nearer(a, b) = if _project(a, θ, φ).at(2) > _project(b, θ, φ).at(2) { a } else { b }
    content(to-screen(nearer((0, -1.22, -_Z), (0, 1.22, -_Z))), small-text(labels.at(0)))
    content(to-screen(nearer((-1.22, 0, -_Z), (1.22, 0, -_Z))), small-text(labels.at(1)))
    content(to-screen((far.at(0), far.at(1), _Z + 0.14)), small-text(labels.at(2)))
  })
  _figure(data, frame, plist)
}
