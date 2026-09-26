// Общее для графика и поверхности: параметры, деления осей, числа, цвета,
// обёртка рисунка (HTML — `div.k-plot` с данными для клиента, PDF — кадр
// и подпись значений).

#import "../theme.typ": current-theme
#import "../web.typ": is-web, elem

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
