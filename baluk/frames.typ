// Кадры: рисунок с параметром — ползунок и проигрывание.
//
//   #fig(frames(n => canvas(theme => {
//     import cetz.draw: *
//     for i in range(n) { rect((i * 0.5, 0), (i * 0.5 + 0.4, 0.4)) }
//   }), n: (from: 1, to: 8, value: 3)), [Прямоугольников — n])
//
// Typst собирает рисунок для каждого значения параметра. В приложении под
// рисунком — ползунок, «▶» проигрывает кадры по очереди (анимация, шаги
// алгоритма); в PDF и без JS — кадр при значении по умолчанию и подпись
// «n = 3» (или несколько кадров в ряд: `pdf: (1, 4, 8)`). Считает всё
// Typst: годится любой рисунок, не только формула.
//
// В HTML — `div.k-frames` с `data-k-frames` (JSON: имя, число кадров, номер
// по умолчанию, скорость) и кадрами `div.k-frames-item` внутри; кадр по
// умолчанию помечен `data-k-frame-default`. Каждый кадр — обычный `canvas`
// (`div.k-frame`): ядро склеивает темы и общие глифы как у любого рисунка.

#import "theme.typ": current-theme
#import "web.typ": is-web, elem
#import "plots/common.typ": _num
#import "i18n.typ": word
#import "figures/canvas.typ": in-row

/// Больше кадров — страница тяжелеет (каждый кадр — свой SVG).
#let _max-frames = 60

/// Значения параметра из (from:, to:, step:, value:) или (values: (…), value:).
/// → (значения, номер значения по умолчанию, шаг для подписи или none)
#let _values(name, spec) = {
  let fail(msg) = panic("frames: параметр «" + name + "»: " + msg)
  let spec = if type(spec) == array { (values: spec) } else { spec }
  if type(spec) != dictionary {
    fail("нужен словарь (from: 1, to: 10, step: 1, value: 5) или массив значений")
  }
  let extra = spec.keys().filter(k => k not in ("from", "to", "step", "value", "values"))
  if extra.len() > 0 { fail("неизвестные ключи: " + extra.join(", ") + "; есть from, to, step, value, values") }
  let (values, step) = if "values" in spec {
    if "from" in spec or "to" in spec or "step" in spec { fail("либо values, либо from/to/step") }
    if type(spec.values) != array or spec.values.len() == 0 { fail("values — непустой массив") }
    (spec.values, none)
  } else {
    if "from" not in spec or "to" not in spec { fail("нужны from и to (или values)") }
    let (from, to) = (spec.from, spec.to)
    let step = spec.at("step", default: 1)
    if not (step > 0) { fail("нужен step > 0") }
    if not (to >= from) { fail("нужно from ≤ to") }
    let count = calc.floor((to - from) / step + 1e-9) + 1
    if count > _max-frames { fail("кадров " + str(count) + ", больше " + str(_max-frames) + " — увеличьте step") }
    // целые — целыми (подпись «n = 3», а не «n = 3.0»)
    let whole = type(from) == int and type(step) == int
    (range(count).map(i => if whole { from + i * step } else { from + i * step * 1.0 }), step)
  }
  if values.len() > _max-frames { fail("кадров " + str(values.len()) + ", больше " + str(_max-frames)) }
  let default = if "value" in spec {
    let i = values.position(v => v == spec.value)
    // значение между шагами диапазона — ближайший кадр
    if i == none and step != none and type(spec.value) in (int, float) {
      let dist = values.map(v => calc.abs(v - spec.value))
      i = dist.position(d => d == calc.min(..dist))
    }
    if i == none { fail("value — одно из значений") }
    i
  } else { 0 }
  (values, default, step)
}

/// Подпись значения: число — «n = 2.5», строка и содержимое — «n = …»,
/// прочее — «кадр 3 из 8».
#let _default-label(name, v, i, count, step) = {
  if type(v) in (int, float) {
    let s = if step != none { _num(v, step, trim: true) } else if type(v) == int { str(v).replace("-", "−") } else { _num(v, 0.001, trim: true) }
    [#emph(name) = #s]
  } else if type(v) in (str, content) {
    [#emph(name) = #v]
  } else {
    context [#word("frame") #(i + 1) #word("frame-of") #count]
  }
}

/// Рисунок-кадры: тело — функция значения параметра → рисунок (обычно canvas).
/// Параметр ровно один, именованным аргументом:
///   n: (from: 1, to: 20, step: 1, value: 5)   — диапазон (step по умолчанию 1)
///   k: (values: (1, 2, 4, 8), value: 4)        — явные значения (или просто массив)
/// label: auto — «n = 5» под кадром; none — без подписи; функция v => [...] — своя.
/// fps — кадров в секунду при проигрывании; loop — по кругу.
/// pdf: auto — в PDF кадр по умолчанию; номера кадров (с 1), например
///   (1, 4, 8), — эти кадры в ряд, каждый со своей подписью (HTML не меняется).
#let frames(body, label: auto, fps: 2, loop: false, pdf: auto, ..param) = {
  if param.pos().len() > 0 { panic("frames: лишние позиционные аргументы; параметр — именованный: n: (from: 1, to: 10)") }
  let named = param.named()
  if named.len() != 1 {
    panic("frames: нужен ровно один параметр, например n: (from: 1, to: 10); сейчас: " + if named.len() == 0 { "ни одного" } else { named.keys().join(", ") })
  }
  if type(body) != function { panic("frames: первый аргумент — функция значения, например n => canvas(…)") }
  // те же пределы, что у клиента (app/src/lib/frames.ts)
  if not (type(fps) in (int, float) and fps > 0 and fps <= 60) { panic("frames: fps — число от 0 до 60, например 2") }
  if type(loop) != bool { panic("frames: loop — true или false") }
  let (name, spec) = named.pairs().first()
  let (values, default, step) = _values(name, spec)
  let count = values.len()
  if pdf != auto {
    let bad = type(pdf) != array or pdf.len() == 0 or pdf.any(k => type(k) != int or k < 1 or k > count)
    if bad { panic("frames: pdf — auto или номера кадров от 1 до " + str(count) + ", например (1, " + str(calc.min(count, 4)) + ")") }
  }
  let caption(v, i) = if label == none { none } else if label == auto {
    _default-label(name, v, i, count, step)
  } else if type(label) == function { label(v) } else {
    panic("frames: label — auto, none или функция значения")
  }
  context if is-web() {
    let data = (name: name, count: count, default: default, fps: fps, loop: loop)
    elem("div", "k-frames", ..("data-k-frames": json.encode(data, pretty: false)), elem("div", "k-frames-stack",
      values.enumerate().map(((i, v)) => {
        let attrs = if i == default { ("data-k-frame-default": "") } else { (:) }
        elem("div", "k-frames-item", ..attrs, {
          body(v)
          let c = caption(v, i)
          if c != none { elem("div", "k-frames-label", c) }
        })
      }).join(),
    ))
  } else {
    let theme = current-theme()
    // Кадр с подписью под ним.
    let shot(i) = {
      let value = values.at(i)
      body(value)
      let c = caption(value, i)
      if c != none {
        set text(size: theme.size.small)
        v(0.2em, weak: true)
        align(center, text(fill: theme.color.muted, c))
      }
    }
    if pdf == auto {
      block(breakable: false, shot(default))
    } else {
      block(breakable: false, in-row(..pdf.map(k => block(shot(k - 1)))))
    }
  }
}
