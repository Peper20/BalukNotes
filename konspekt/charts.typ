// Диаграммы по данным: оси с засечками, сетка, легенда и ряды.
//
// Отличие от `оси` + `график` из figures.typ: там оси пересекаются в начале
// координат и рисуют математическую картинку в единицах холста. Здесь — поле
// с рамкой слева и снизу, данные в своих единицах (секунды, мегабайты,
// количество), и всё масштабируется само.
//
//   #рис(холст(диаграмма(
//     (вид: "столбцы", данные: (([n²], 120), ([n log n], 14), ([n], 3))),
//   )), [Время работы])
//
// cetz-plot тут не используется: версия из кэша не запускается на Typst 0.15.

#import "@preview/cetz:0.4.2"
#import "theme.typ": тема, тёмная
#import "figures.typ": _цвет

#let _draw = cetz.draw

// ── Числа и засечки ───────────────────────────────────────────────────────
#let _число(v) = {
  let r = calc.round(v, digits: 3)
  let s = str(r)
  let s = if s.ends-with(".0") { s.slice(0, -2) } else { s }
  s.replace(".", ",") // десятичная запятая: подписи осей — обычный текст
}

// «Круглый» шаг засечек: 1, 2, 5 × 10^k — ближайший к размаху / сколько.
#let _шаг(размах, сколько) = {
  if размах <= 0 { return 1 }
  let грубый = размах / calc.max(сколько, 1)
  let p = calc.pow(10.0, calc.floor(calc.log(грубый, base: 10)))
  let m = грубый / p
  let к = if m <= 1 { 1 } else if m <= 2 { 2 } else if m <= 5 { 5 } else { 10 }
  к * p
}

#let _засечки(мин, макс, сколько) = {
  let ш = _шаг(макс - мин, сколько)
  let первая = calc.ceil(мин / ш) * ш
  let итог = ()
  let v = первая
  while v <= макс + ш * 0.001 {
    итог.push(v)
    v += ш
  }
  итог
}

// ── Диапазоны по рядам ────────────────────────────────────────────────────
#let _точки-ряда(ряд) = {
  let вид = ряд.at("вид", default: "ломаная")
  if вид == "функция" {
    let (от, до) = (ряд.at("от", default: 0), ряд.at("до", default: 1))
    range(41).map(i => { let x = от + (до - от) * i / 40; (x, (ряд.f)(x)) })
  } else if вид == "столбцы" {
    ряд.данные.enumerate().map(((i, d)) => (i, if type(d) == array { d.at(1) } else { d }))
  } else {
    ряд.данные
  }
}

// ── Диаграмма ─────────────────────────────────────────────────────────────
/// Ряды — позиционные словари:
///   (вид: "ломаная",  данные: ((x, y), ...), подпись:, цвет:, точки: true, пунктир: false)
///   (вид: "точки",    данные: ((x, y), ...), подпись:, цвет:)
///   (вид: "ступени",  данные: ((x, y), ...), подпись:, цвет:)
///   (вид: "столбцы",  данные: ((подпись, значение), ...) или (значение, ...), цвет:)
///   (вид: "функция",  f: x => ..., от:, до:, подпись:, цвет:)
/// Диапазоны x и y по умолчанию считаются по данным (auto).
/// легенда: "справа" | "внутри" | none.
#let диаграмма(
  ..ряды,
  x: auto,
  y: auto,
  ширина: 6,
  высота: 3.6,
  подписи: (none, none),
  засечки-x: auto,
  засечки-y: auto,
  формат-x: _число,
  формат-y: _число,
  сетка: true,
  легенда: "справа",
  нуль: true,
) = _draw.get-ctx(ctx => {
  let т = тема()
  let р = т.цвет.рис
  let u = 1pt / ctx.length
  import cetz.draw: *

  let рр = ряды.pos()
  let столбцы = рр.filter(р2 => р2.at("вид", default: "ломаная") == "столбцы")
  let все = ()
  for р2 in рр { все += _точки-ряда(р2) }
  let xs = все.map(p => p.at(0))
  let ys = все.map(p => p.at(1))

  let (x0, x1) = if x != auto { x } else if столбцы.len() > 0 {
    (-0.6, calc.max(..xs) + 0.6)
  } else {
    (calc.min(..xs), calc.max(..xs))
  }
  let (y0, y1) = if y != auto { y } else {
    let лоу = calc.min(..ys)
    let хай = calc.max(..ys)
    if нуль { лоу = calc.min(лоу, 0) }
    if хай == лоу { хай = лоу + 1 }
    (лоу, хай + (хай - лоу) * 0.08)
  }
  let к(px, py) = (
    (px - x0) / (x1 - x0) * ширина,
    (py - y0) / (y1 - y0) * высота,
  )

  // ── сетка и засечки
  let зx = if засечки-x != auto { засечки-x } else if столбцы.len() > 0 {
    столбцы.first().данные.enumerate().map(((i, _)) => i)
  } else { _засечки(x0, x1, 5) }
  let зy = if засечки-y != auto { засечки-y } else { _засечки(y0, y1, 4) }

  if сетка {
    for v in зy {
      line(к(x0, v), к(x1, v), stroke: 0.4pt + р.сетка.transparentize(15%))
    }
  }
  line(к(x0, y0), к(x1, y0), stroke: 0.7pt + р.ось)
  line(к(x0, y0), к(x0, y1), stroke: 0.7pt + р.ось)

  for v in зx {
    let (px, py) = к(v, y0)
    line((px, py), (px, py - 3 * u), stroke: 0.6pt + р.ось)
    let подпись = if столбцы.len() > 0 {
      let d = столбцы.first().данные.at(v, default: none)
      if type(d) == array { d.at(0) } else { [#_число(v)] }
    } else { [#(формат-x)(v)] }
    content((px, py - 4 * u), anchor: "north", text(size: 0.82em, fill: т.цвет.приглушённый, подпись))
  }
  for v in зy {
    let (px, py) = к(x0, v)
    line((px, py), (px - 3 * u, py), stroke: 0.6pt + р.ось)
    content((px - 4 * u, py), anchor: "east", text(size: 0.82em, fill: т.цвет.приглушённый, [#(формат-y)(v)]))
  }
  if подписи.at(0) != none {
    content((ширина / 2, -0.62), anchor: "north", text(size: 0.85em, подписи.at(0)))
  }
  if подписи.at(1) != none {
    content((-0.72, высота / 2), anchor: "south", angle: 90deg, text(size: 0.85em, подписи.at(1)))
  }

  // ── ряды
  let цвета = ("линия", "второй", "третий", "акцент")
  let легенда-элементы = ()
  for (i, ряд) in рр.enumerate() {
    let вид = ряд.at("вид", default: "ломаная")
    let ц = _цвет(т, ряд.at("цвет", default: цвета.at(calc.rem(i, 4))))
    let подпись = ряд.at("подпись", default: none)
    if подпись != none { легенда-элементы.push((вид, ц, подпись)) }

    if вид == "столбцы" {
      let n = ряд.данные.len()
      let ш = ряд.at("ширина-столбца", default: 0.62) * ширина / n
      for (j, d) in ряд.данные.enumerate() {
        let v = if type(d) == array { d.at(1) } else { d }
        let (px, _) = к(j, y0)
        let (_, py) = к(0, v)
        let (_, низ) = к(0, calc.max(y0, 0))
        rect((px - ш / 2, низ), (px + ш / 2, py), fill: ц.transparentize(if тёмная(т) { 55% } else { 70% }), stroke: 0.8pt + ц)
        if ряд.at("значения", default: true) {
          content((px, py), anchor: "south", padding: 2pt, text(size: 0.78em, fill: ц, [#_число(v)]))
        }
      }
    } else if вид == "точки" {
      for p in ряд.данные {
        circle(к(p.at(0), p.at(1)), radius: 2.2pt / ctx.length, fill: ц, stroke: 0.4pt + т.цвет.фон)
      }
    } else if вид == "ступени" {
      let пути = ()
      for (j, p) in ряд.данные.enumerate() {
        if j > 0 { пути.push(к(p.at(0), ряд.данные.at(j - 1).at(1))) }
        пути.push(к(p.at(0), p.at(1)))
      }
      line(..пути, stroke: (paint: ц, thickness: 1.1pt, join: "round"))
    } else {
      let точки = _точки-ряда(ряд).map(p => к(p.at(0), p.at(1)))
      line(..точки, stroke: (
        paint: ц, thickness: 1.2pt, join: "round",
        dash: if ряд.at("пунктир", default: false) { "dashed" } else { none },
      ))
      if ряд.at("точки", default: false) {
        for p in ряд.данные { circle(к(p.at(0), p.at(1)), radius: 2pt / ctx.length, fill: ц, stroke: none) }
      }
    }
  }

  // ── легенда
  if легенда != none and легенда-элементы.len() > 0 {
    let (lx, ly) = if легенда == "внутри" { (0.25, высота - 0.25) } else { (ширина + 0.35, высота) }
    for (j, (вид, ц, подпись)) in легенда-элементы.enumerate() {
      let y = ly - j * 0.42
      if вид == "точки" {
        circle((lx + 0.14, y), radius: 2.2pt / ctx.length, fill: ц, stroke: none)
      } else if вид == "столбцы" {
        rect((lx, y - 0.07), (lx + 0.28, y + 0.07), fill: ц.transparentize(70%), stroke: 0.8pt + ц)
      } else {
        line((lx, y), (lx + 0.3, y), stroke: 1.2pt + ц)
      }
      content((lx + 0.42, y), anchor: "west", text(size: 0.82em, подпись))
    }
  }
})
