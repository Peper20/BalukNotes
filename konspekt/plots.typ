// Интерактивные рисунки: график с ползунками и поверхность с вращением.
//
//   #рис(график-интерактив("a * calc.sin(b * x)", -5, 5,
//     параметры: (a: (от: 0, до: 3, знач: 1), b: (от: 0.5, до: 4, шаг: 0.1, знач: 1)),
//     y: (-3, 3)), [Амплитуда и частота синусоиды])
//
// Формула — строка с кодом Typst из подмножества (см. `_разбор`): её считает
// и Typst (кадр для PDF и для страницы без JS), и клиент приложения
// (`app/src/lib/plot/`) — разбор и вычисление там повторяют здешние
// ПОСТРОЧНО. Меняешь одно — меняй и другое; сверка — фикстура
// `tests/vault/Рисунки/Интерактив.typ` (её снимок читает Vitest).
//
// В HTML рисунок — `div.k-plot` с `data-k-plot` (JSON: формулы, диапазоны,
// параметры) и обычным кадром `холст` внутри; клиент прячет кадр и рисует
// живой рисунок. В PDF — кадр при значениях по умолчанию и подпись с ними.
//
// Свой разбор, а не `eval`: `eval` падает с ошибкой сборки на делении на
// ноль, `calc.sqrt` от отрицательного и т. п. — а на графике это просто
// разрыв кривой.

#import "@preview/cetz:0.4.2"
#import "theme.typ": тема, тени-рисунка
#import "web.typ": веб, эл
#import "figures.typ": холст

#let _draw = cetz.draw

// ── Разбор формулы ───────────────────────────────────────────────────────

#let _функции = (
  sin: 1, cos: 1, tan: 1, asin: 1, acos: 1, atan: 1, exp: 1, ln: 1, log: 1,
  sqrt: 1, abs: 1, floor: 1, ceil: 1, pow: 2,
)
#let _константы = (pi: calc.pi, e: calc.e)

#let _лексема = regex("\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|[\p{L}_][\p{L}\p{N}_]*(?:\.[\p{L}_][\p{L}\p{N}_]*)?|[-+*/(),]|\S")

/// Лексемы формулы (пробелы пропускаются).
#let _лексемы(с) = с.matches(_лексема).map(м => м.text)

#let _ошибка(с, текст) = panic("формула «" + с + "»: " + текст)

/// Разбор выражения уровня `ур` с лексемы `i`: (узел, следующая лексема).
/// Уровни: 0 — сумма, 1 — произведение, 2 — унарный знак и операнд.
/// Приоритеты как в Typst: унарный минус сильнее `*` и `/`.
#let _разбор(т, i, ур, имена, с) = {
  let кон = т.len()
  if ур < 2 {
    let опы = if ур == 0 { ("+", "-") } else { ("*", "/") }
    let (л, i) = _разбор(т, i, ур + 1, имена, с)
    while i < кон and т.at(i) in опы {
      let (п, j) = _разбор(т, i + 1, ур + 1, имена, с)
      л = (k: "op", o: т.at(i), a: л, b: п)
      i = j
    }
    return (л, i)
  }
  if i >= кон { _ошибка(с, "формула оборвалась") }
  let л = т.at(i)
  if л == "-" or л == "+" {
    let (а, j) = _разбор(т, i + 1, 2, имена, с)
    return (if л == "-" { (k: "neg", a: а) } else { а }, j)
  }
  if л == "(" {
    let (а, j) = _разбор(т, i + 1, 0, имена, с)
    if j >= кон or т.at(j) != ")" { _ошибка(с, "не хватает «)»") }
    return (а, j + 1)
  }
  if л.match(regex("^\d")) != none { return ((k: "n", v: float(л)), i + 1) }
  if л.starts-with("calc.") {
    let имя = л.slice(5)
    if имя in _константы { return ((k: "n", v: _константы.at(имя)), i + 1) }
    if имя not in _функции {
      _ошибка(с, "нет функции «" + л + "»; есть calc." + _функции.keys().join(", calc.") + ", calc.pi, calc.e")
    }
    if i + 1 >= кон or т.at(i + 1) != "(" { _ошибка(с, "после «" + л + "» нужны скобки") }
    let арг = ()
    let j = i + 2
    let (а, j) = _разбор(т, j, 0, имена, с)
    арг.push(а)
    while j < кон and т.at(j) == "," {
      let (а, k) = _разбор(т, j + 1, 0, имена, с)
      арг.push(а)
      j = k
    }
    if j >= кон or т.at(j) != ")" { _ошибка(с, "не хватает «)» после аргументов «" + л + "»") }
    if арг.len() != _функции.at(имя) {
      _ошибка(с, "«" + л + "» принимает аргументов: " + str(_функции.at(имя)))
    }
    return ((k: "f", n: имя, args: арг), j + 1)
  }
  if л in имена { return ((k: "v", n: л), i + 1) }
  if л == "^" { _ошибка(с, "степень пишется calc.pow(x, 2)") }
  if л.match(regex("^[\p{L}_]")) != none {
    _ошибка(с, "неизвестное имя «" + л + "»; можно: " + имена.join(", ") + ", calc.…")
  }
  _ошибка(с, "непонятный знак «" + л + "»")
}

/// Разбирает формулу с переменными `имена` (x, y, параметры).
#let _формула(с, имена) = {
  let т = _лексемы(с)
  if т.len() == 0 { _ошибка(с, "пустая формула") }
  let (узел, i) = _разбор(т, 0, 0, имена, с)
  if i < т.len() and т.at(i) == "^" { _ошибка(с, "степень пишется calc.pow(x, 2)") }
  if i < т.len() { _ошибка(с, "лишнее «" + т.at(i) + "»") }
  узел
}

// Число годно: не none, не NaN, не бесконечность.
#let _годно(v) = v != none and v == v and calc.abs(v) < 1e300

/// Значение узла при переменных `пер` (словарь). Вне области определения
/// (деление на ноль, корень из отрицательного…) — none.
#let _знач(у, пер) = {
  let k = у.k
  if k == "n" { return у.v }
  if k == "v" { return пер.at(у.n) }
  if k == "neg" {
    let a = _знач(у.a, пер)
    return if a == none { none } else { -a }
  }
  if k == "op" {
    let a = _знач(у.a, пер)
    if a == none { return none }
    let b = _знач(у.b, пер)
    if b == none { return none }
    let r = if у.o == "+" { a + b } else if у.o == "-" { a - b } else if у.o == "*" { a * b } else if b == 0 { none } else { a / b }
    return if _годно(r) { r } else { none }
  }
  // функция
  let a = _знач(у.args.at(0), пер)
  if a == none { return none }
  let н = у.n
  let r = if н == "sin" { calc.sin(a) } else if н == "cos" { calc.cos(a) } else if н == "tan" { calc.tan(a) }
  else if н == "asin" { if calc.abs(a) > 1 { none } else { calc.asin(a).rad() } }
  else if н == "acos" { if calc.abs(a) > 1 { none } else { calc.acos(a).rad() } }
  else if н == "atan" { calc.atan(a).rad() }
  else if н == "exp" { if a > 700 { none } else { calc.exp(a) } }
  else if н == "ln" { if a <= 0 { none } else { calc.ln(a) } }
  else if н == "log" { if a <= 0 { none } else { calc.log(a) } }
  else if н == "sqrt" { if a < 0 { none } else { calc.sqrt(a) } }
  else if н == "abs" { calc.abs(a) }
  else if н == "floor" { float(calc.floor(a)) }
  else if н == "ceil" { float(calc.ceil(a)) }
  else if н == "pow" {
    let b = _знач(у.args.at(1), пер)
    if b == none { none }
    else if a == 0 { if b > 0 { 0.0 } else if b == 0 { 1.0 } else { none } }
    else if a < 0 and calc.fract(b) != 0 { none }
    else if b * calc.ln(calc.abs(a)) > 700 { none }
    else { calc.pow(a, b) }
  }
  if _годно(r) { float(r) } else { none }
}

// ── Общее: параметры, деления осей, числа ────────────────────────────────

/// Параметры → массив словарей (name, min, max, step, value) для JSON и PDF.
#let _параметры(п, запрет) = п.pairs().map(((имя, о)) => {
  if имя in запрет or имя.match(regex("^[\p{L}_][\p{L}\p{N}_]*$")) == none {
    panic("параметр «" + имя + "»: нужно имя вроде a, k, ω (не " + запрет.join(", ") + ")")
  }
  let от = float(о.от)
  let до = float(о.до)
  if not (до > от) { panic("параметр «" + имя + "»: нужно от < до") }
  let шаг = float(о.at("шаг", default: (до - от) / 100))
  let знач = float(о.at("знач", default: от))
  (name: имя, min: от, max: до, step: шаг, value: знач)
})

/// Шаг делений: 1, 2 или 5 × 10^k, около пяти делений на диапазон.
#let _шаг(d) = {
  let s = d / 5
  let p = calc.pow(10.0, calc.floor(calc.log(s)))
  let m = s / p
  (if m < 1.5 { 1 } else if m < 3.5 { 2 } else if m < 7.5 { 5 } else { 10 }) * p
}

/// Число для подписи: знаков после запятой — как у шага, минус типографский.
/// обрезать: убрать хвостовые нули («1.0» → «1») — для подписи значений.
#let _число(v, шаг, обрезать: false) = {
  let зн = calc.max(0, -calc.floor(calc.log(шаг) + 1e-9))
  let r = calc.round(v, digits: зн)
  if calc.abs(r) < шаг / 1e6 { r = 0 }
  let с = str(calc.abs(r))
  if зн > 0 {
    let (ц, д) = if с.contains(".") { с.split(".") } else { (с, "") }
    с = ц + "." + д + "0" * (зн - д.len())
  }
  if обрезать and с.contains(".") { с = с.trim("0", at: end).trim(".", at: end) }
  (if r < 0 { "−" } else { "" }) + с
}

/// Деления в [a, b] с шагом `шаг`.
#let _деления(a, b, шаг) = {
  let i0 = calc.ceil(a / шаг - 1e-9)
  let i1 = calc.floor(b / шаг + 1e-9)
  range(i0, i1 + 1).map(i => i * шаг)
}

/// Подпись значений параметров: «a = 1, b = 0.5».
#let _значения(пар) = пар.map(п => п.name + " = " + _число(п.value, п.step, обрезать: true)).join(", ")

/// Словарь переменных для вычисления: параметры при значениях по умолчанию.
#let _окружение(пар) = пар.map(п => (п.name, п.value)).to-dict()

#let _json(данные) = json.encode(данные, pretty: false)

/// Цвет рисунка по имени: линия, второй, третий, грань, акцент.
#let _цвет-рис(т, имя) = if имя == "акцент" { т.цвет.акцент } else { т.цвет.рис.at(имя, default: т.цвет.рис.линия) }

/// Имя цвета → CSS-переменная (в HTML цвета задаёт тема, не разметка).
#let _css-цвет(имя) = if имя == "акцент" { "var(--k-accent)" } else {
  "var(--k-fig-" + (линия: "line", второй: "second", третий: "third", грань: "face").at(имя, default: "line") + ")"
}

/// Обёртка: в HTML — div.k-plot с JSON и кадром внутри, в PDF — кадр и подпись.
/// легенда — кривые с подписями (под рисунком: на поле подпись наезжает на оси).
#let _рисунок(данные, кадр, пар, легенда: ()) = context {
  let т = тема()
  let подпись = if пар.len() > 0 { _значения(пар) }
  let знак(к) = if к.dashed { "- - " } else { "— " }
  if веб() {
    эл("div", "k-plot", ..("data-k-plot": _json(данные)), {
      кадр
      if легенда.len() > 0 {
        эл("div", "k-plot-legend", легенда.map(к => эл("span", "k-plot-key", ..(style: "color: " + _css-цвет(к.color)), знак(к) + к.label)).join())
      }
      if подпись != none { эл("div", "k-plot-values", подпись) }
    })
  } else {
    block(breakable: false, {
      кадр
      set text(size: т.кегль.мелкий)
      if легенда.len() > 0 {
        v(0.3em, weak: true)
        align(center, легенда.map(к => text(fill: _цвет-рис(т, к.color), знак(к) + emph(к.label))).join(h(1.2em)))
      }
      if подпись != none {
        v(0.2em, weak: true)
        align(center, text(fill: т.цвет.приглушённый, подпись))
      }
    })
  }
}

// ── 2D ───────────────────────────────────────────────────────────────────

/// Участки кривой внутри полосы y ∈ [y0, y1]: точки — (x, y) или (x, none).
/// Отрезок, пересекающий полосу, обрезается по её краю; отрезок между
/// точками по разные стороны от полосы — разрыв (асимптота, как у tg x).
#let _участки(т, y0, y1) = {
  let все = ()
  let тек = ()
  for i in range(т.len() - 1) {
    let (xa, ya) = т.at(i)
    let (xb, yb) = т.at(i + 1)
    if ya == none or yb == none or (ya > y1 and yb < y0) or (ya < y0 and yb > y1) {
      if тек.len() > 1 { все.push(тек) }
      тек = ()
      continue
    }
    // обрезка отрезка по полосе: параметры входа и выхода t ∈ [0, 1]
    let (t0, t1) = (0.0, 1.0)
    for (гр, выше) in ((y0, false), (y1, true)) {
      let вне-a = if выше { ya > гр } else { ya < гр }
      let вне-b = if выше { yb > гр } else { yb < гр }
      if вне-a and вне-b { t0 = 2.0 }
      else if вне-a { t0 = calc.max(t0, (гр - ya) / (yb - ya)) }
      else if вне-b { t1 = calc.min(t1, (гр - ya) / (yb - ya)) }
    }
    if t0 > t1 {
      if тек.len() > 1 { все.push(тек) }
      тек = ()
      continue
    }
    let p(t) = (xa + (xb - xa) * t, ya + (yb - ya) * t)
    if тек.len() == 0 or t0 > 0 {
      if тек.len() > 1 { все.push(тек) }
      тек = (p(t0),)
    }
    тек.push(p(t1))
    if t1 < 1 {
      if тек.len() > 1 { все.push(тек) }
      тек = ()
    }
  }
  if тек.len() > 1 { все.push(тек) }
  все
}

/// Точки кривой: n + 1 точка на [a, b].
#let _выборка(ф, a, b, n, пер) = range(n + 1).map(i => {
  let x = a + (b - a) * i / n
  (x, _знач(ф, пер + (x: x)))
})

/// Диапазон y по точкам: без выбросов (2 % с каждой стороны) и с полями.
#let _диапазон(значения) = {
  let з = значения.filter(v => v != none).sorted()
  if з.len() == 0 { return (-1.0, 1.0) }
  let k = calc.floor(з.len() * 0.02)
  let (lo, hi) = (з.at(k), з.at(з.len() - 1 - k))
  if hi - lo < 1e-9 { return (lo - 1, hi + 1) }
  let поле = (hi - lo) * 0.08
  (lo - поле, hi + поле)
}

/// Кривые: строка, словарь (f:, цвет:, подпись:, пунктир:) или массив таких.
#let _кривые(ф) = {
  let список = if type(ф) == array { ф } else { (ф,) }
  let цвета = ("линия", "второй", "третий")
  список.enumerate().map(((i, к)) => {
    let к = if type(к) == str { (f: к) } else { к }
    (
      f: к.f,
      color: к.at("цвет", default: цвета.at(calc.rem(i, 3))),
      label: к.at("подпись", default: none),
      dashed: к.at("пунктир", default: false),
    )
  })
}

/// График y = f(x) с ползунками параметров.
/// формула: "a * calc.sin(x)" | (f: "…", цвет: "второй", подпись: "sin", пунктир: true) | массив.
/// параметры: (a: (от: 0, до: 2, шаг: 0.1, знач: 1), …) — шаг по умолчанию 1/100 диапазона,
///   знач — от.
/// y: (y0, y1) или auto (по кадру при значениях по умолчанию; лучше задать).
/// ширина, высота — размер поля графика в см.
#let график-интерактив(формула, a, b, параметры: (:), y: auto, подписи: ("x", "y"), ширина: 8, высота: 5, n: 160) = {
  let пар = _параметры(параметры, ("x", "y", "calc"))
  let имена = ("x",) + пар.map(п => п.name)
  let кривые = _кривые(формула)
  let деревья = кривые.map(к => _формула(к.f, имена))
  let пер = _окружение(пар)
  let (a, b) = (float(a), float(b))
  if not (b > a) { panic("график-интерактив: нужно a < b") }
  let точки = деревья.map(д => _выборка(д, a, b, n, пер))
  let (y0, y1) = if y == auto { _диапазон(точки.join().map(т => т.at(1))) } else { y.map(float) }
  if not (y1 > y0) { panic("график-интерактив: нужно y0 < y1") }
  let данные = (
    kind: "2d", curves: кривые, x: (a, b), y: (y0, y1), params: пар,
    labels: подписи, width: ширина, height: высота,
  )
  let (sx, sy) = (ширина / (b - a), высота / (y1 - y0))
  let к-холсту(p) = ((p.at(0) - a) * sx, (p.at(1) - y0) * sy)
  let кадр = холст(т => {
    import cetz.draw: *
    let р = т.цвет.рис
    // оси: через ноль, если он в диапазоне, иначе по краю
    let ox = if a <= 0 and 0 <= b { 0.0 } else { a }
    let oy = if y0 <= 0 and 0 <= y1 { 0.0 } else { y0 }
    let (hx, hy) = (_шаг(b - a), _шаг(y1 - y0))
    let м = (end: "stealth", fill: р.ось, stroke: 0pt, scale: 0.55)
    let штрих = 0.6pt + р.ось
    let сетка = 0.3pt + р.сетка
    for x in _деления(a, b, hx) {
      line(к-холсту((x, y0)), к-холсту((x, y1)), stroke: сетка)
    }
    for yy in _деления(y0, y1, hy) {
      line(к-холсту((a, yy)), к-холсту((b, yy)), stroke: сетка)
    }
    line(к-холсту((a, oy)), к-холсту((b, oy)), stroke: штрих, mark: м)
    line(к-холсту((ox, y0)), к-холсту((ox, y1)), stroke: штрих, mark: м)
    let мелко(с) = text(size: 0.85em, fill: р.ось, с)
    for x in _деления(a, b, hx) {
      if calc.abs(x - ox) > hx / 2 and a + hx / 3 < x and x < b - hx / 3 {
        content(к-холсту((x, oy)), anchor: "north", padding: 2pt, мелко(_число(x, hx)))
      }
    }
    for yy in _деления(y0, y1, hy) {
      if calc.abs(yy - oy) > hy / 2 and y0 + hy / 3 < yy and yy < y1 - hy / 3 {
        content(к-холсту((ox, yy)), anchor: "east", padding: 2pt, мелко(_число(yy, hy)))
      }
    }
    content(к-холсту((b, oy)), anchor: "north-east", padding: (top: 7pt), emph(подписи.at(0)))
    content(к-холсту((ox, y1)), anchor: "north-west", padding: (left: 5pt), emph(подписи.at(1)))
    for (к, тч) in кривые.zip(точки) {
      let ц = _цвет-рис(т, к.color)
      let участки = _участки(тч, y0, y1)
      for у in участки {
        line(..у.map(к-холсту), stroke: (paint: ц, thickness: 1.1pt, join: "round", dash: if к.dashed { "dashed" } else { none }))
      }
    }
  })
  _рисунок(данные, кадр, пар, легенда: кривые.filter(к => к.label != none))
}

// ── 3D ───────────────────────────────────────────────────────────────────
//
// Поверхность нормируется в коробку [-1, 1] × [-1, 1] × [-0.7, 0.7] и
// рисуется ортогонально: поворот вокруг вертикали на угол `поворот`
// (азимут), потом наклон на `наклон`. Клиент вращает с того же вида.

#let _Z = 0.7

/// Экранные координаты и глубина (больше — ближе к зрителю) точки коробки.
#let _вид(p, θ, φ) = {
  let (x, y, z) = p
  let xr = x * calc.cos(θ) - y * calc.sin(θ)
  let yr = x * calc.sin(θ) + y * calc.cos(θ)
  (xr, z * calc.cos(φ) + yr * calc.sin(φ), -yr * calc.cos(φ) + z * calc.sin(φ))
}

/// Освещённость 0..1 грани с нормалью n (в осях экрана: u, v, глубина).
/// Свет — от зрителя слева сверху; обе стороны грани освещены одинаково.
#let _освещ(n) = {
  let (a, b, c) = n
  let len = calc.sqrt(a * a + b * b + c * c)
  if len == 0 { return 0.5 }
  let (lu, lv, ld) = (-0.38, 0.62, 0.69)
  calc.abs(a * lu + b * lv + c * ld) / len
}

/// Цвет грани: смесь тени и света темы (как `--k-fig-*-dark/-light` в CSS).
#let _тон(т, база, k) = {
  let (тень, свет) = тени-рисунка(т, база)
  color.mix((тень, 100% - k * 100%), (свет, k * 100%), space: rgb)
}

/// Поверхность z = f(x, y) с вращением перетаскиванием и ползунками.
/// формула: "calc.sin(a * x) * calc.cos(y)"; xr, yr — диапазоны (x0, x1).
/// z: (z0, z1) или auto (по кадру при значениях по умолчанию).
/// вид: "свет" — грани со светотенью, "сетка" — каркас.
/// цвет: "грань" (по умолчанию), "линия", "второй", "третий".
/// поворот, наклон — начальный вид (в градусах).
#let поверхность-интерактив(формула, xr, yr, параметры: (:), z: auto, подписи: ("x", "y", "z"), n: 24, вид: "свет", цвет: "грань", поворот: -30, наклон: 28, размер: 3) = {
  let пар = _параметры(параметры, ("x", "y", "calc"))
  let имена = ("x", "y") + пар.map(п => п.name)
  let д = _формула(формула, имена)
  let пер = _окружение(пар)
  let (x0, x1) = xr.map(float)
  let (y0, y1) = yr.map(float)
  if not (x1 > x0 and y1 > y0) { panic("поверхность-интерактив: нужно x0 < x1 и y0 < y1") }
  if цвет not in ("грань", "линия", "второй", "третий") { panic("поверхность-интерактив: цвет — грань, линия, второй или третий") }
  // значения в узлах сетки (n + 1) × (n + 1)
  let узлы = range(n + 1).map(i => range(n + 1).map(j => {
    let x = x0 + (x1 - x0) * i / n
    let y = y0 + (y1 - y0) * j / n
    _знач(д, пер + (x: x, y: y))
  }))
  let (z0, z1) = if z == auto {
    let з = узлы.join().filter(v => v != none)
    if з.len() == 0 { (-1.0, 1.0) } else {
      let (lo, hi) = (calc.min(..з), calc.max(..з))
      if hi - lo < 1e-9 { (lo - 1, hi + 1) } else { (lo, hi) }
    }
  } else { z.map(float) }
  if not (z1 > z0) { panic("поверхность-интерактив: нужно z0 < z1") }
  let данные = (
    kind: "3d", f: формула, x: (x0, x1), y: (y0, y1), z: (z0, z1), params: пар,
    labels: подписи, n: n, style: вид, color: цвет, view: (поворот, наклон), size: размер,
  )
  let (θ, φ) = (поворот * 1deg, наклон * 1deg)
  let S = размер / 2
  // узел → точка коробки
  let к(i, j, v) = (-1 + 2 * i / n, -1 + 2 * j / n, -_Z + 2 * _Z * (v - z0) / (z1 - z0))
  let экран(p) = { let (u, vv, _) = _вид(p, θ, φ); (u * S, vv * S) }
  let кадр = холст(т => {
    import cetz.draw: *
    let р = т.цвет.рис
    let база = р.at(цвет)
    let ось = 0.5pt + р.ось
    // пол коробки и вертикальное ребро в дальнем углу
    let углы = ((-1, -1), (1, -1), (1, 1), (-1, 1))
    let дальний = углы.sorted(key: ((x, y)) => _вид((x, y, 0), θ, φ).at(2)).first()
    line(..углы.map(((x, y)) => экран((x, y, -_Z))), close: true, stroke: ось)
    line(экран((..дальний, -_Z)), экран((..дальний, _Z)), stroke: ось)
    // грани: дальние раньше (алгоритм художника); точки вне [z0, z1] — не рисуем
    let грани = ()
    for i in range(n) {
      for j in range(n) {
        let зн = (узлы.at(i).at(j), узлы.at(i + 1).at(j), узлы.at(i + 1).at(j + 1), узлы.at(i).at(j + 1))
        if зн.any(v => v == none or v < z0 or v > z1) { continue }
        let p = (к(i, j, зн.at(0)), к(i + 1, j, зн.at(1)), к(i + 1, j + 1, зн.at(2)), к(i, j + 1, зн.at(3)))
        let в = p.map(q => _вид(q, θ, φ))
        let глуб = в.map(q => q.at(2)).sum() / 4
        грани.push((глуб, в))
      }
    }
    let край = 0.3pt + р.линия.transparentize(35%)
    for (_, в) in грани.sorted(key: г => г.at(0)) {
      let pts = в.map(q => (q.at(0) * S, q.at(1) * S))
      if вид == "сетка" {
        line(..pts, close: true, stroke: 0.5pt + база)
      } else {
        // нормаль по диагоналям грани (в осях экрана)
        let d1 = range(3).map(c => в.at(2).at(c) - в.at(0).at(c))
        let d2 = range(3).map(c => в.at(3).at(c) - в.at(1).at(c))
        let nrm = (
          d1.at(1) * d2.at(2) - d1.at(2) * d2.at(1),
          d1.at(2) * d2.at(0) - d1.at(0) * d2.at(2),
          d1.at(0) * d2.at(1) - d1.at(1) * d2.at(0),
        )
        line(..pts, close: true, fill: _тон(т, база, _освещ(nrm)), stroke: край)
      }
    }
    // подписи осей — поверх граней
    let мелко(с) = text(fill: р.ось, emph(с))
    // x — у ближнего к зрителю ребра вдоль x, y — у ближнего вдоль y
    let ближе(a, b) = if _вид(a, θ, φ).at(2) > _вид(b, θ, φ).at(2) { a } else { b }
    content(экран(ближе((0, -1.22, -_Z), (0, 1.22, -_Z))), мелко(подписи.at(0)))
    content(экран(ближе((-1.22, 0, -_Z), (1.22, 0, -_Z))), мелко(подписи.at(1)))
    content(экран((дальний.at(0), дальний.at(1), _Z + 0.14)), мелко(подписи.at(2)))
  })
  _рисунок(данные, кадр, пар)
}
