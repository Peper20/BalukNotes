// Shared by the plot and the surface: parameters, axis ticks, numbers, colors,
// the figure wrapper (HTML - `div.k-plot` with data for the client, PDF - the
// frame and a caption with values).

#import "../theme.typ": current-theme
#import "../web.typ": is-web, elem

// ── Shared: parameters, axis ticks, numbers ──────────────────────────────

/// Parameters -> an array of dictionaries (name, min, max, step, value) for JSON and PDF.
#let _params(prm, forbidden) = prm.pairs().map(((name, opts)) => {
  if name in forbidden or name.match(regex("^[\p{L}_][\p{L}\p{N}_]*$")) == none {
    panic("parameter \"" + name + "\": needs a name like a, k, ω (not " + forbidden.join(", ") + ")")
  }
  let from = float(opts.from)
  let to = float(opts.to)
  if not (to > from) { panic("parameter \"" + name + "\": needs from < to") }
  let step = float(opts.at("step", default: (to - from) / 100))
  let value = float(opts.at("value", default: from))
  (name: name, min: from, max: to, step: step, value: value)
})

/// Tick step: 1, 2 or 5 × 10^k, about five ticks per range.
#let _tick-step(d) = {
  let s = d / 5
  let p = calc.pow(10.0, calc.floor(calc.log(s)))
  let m = s / p
  (if m < 1.5 { 1 } else if m < 3.5 { 2 } else if m < 7.5 { 5 } else { 10 }) * p
}

/// A number for a label: decimals as in the step, a typographic minus.
/// trim: drop trailing zeros ("1.0" -> "1") - for the values caption.
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

/// Ticks in [a, b] with step `step`.
#let _divisions(a, b, step) = {
  let i0 = calc.ceil(a / step - 1e-9)
  let i1 = calc.floor(b / step + 1e-9)
  range(i0, i1 + 1).map(i => i * step)
}

/// The caption of parameter values: "a = 1, b = 0.5".
#let _values-label(plist) = plist.map(prm => prm.name + " = " + _num(prm.value, prm.step, trim: true)).join(", ")

/// The dictionary of variables for evaluation: parameters at default values.
#let _env(plist) = plist.map(prm => (prm.name, prm.value)).to-dict()

#let _json(data) = json.encode(data, pretty: false)

/// Figure color by name: line, second, third, face, accent.
#let _fig-color(theme, name) = if name == "accent" { theme.color.accent } else { theme.color.fig.at(name, default: theme.color.fig.line) }

/// Color name -> CSS variable (in HTML the theme sets colors, not the markup).
#let _css-color(name) = if name == "accent" { "var(--k-accent)" } else {
  "var(--k-fig-" + (line: "line", second: "second", third: "third", face: "face").at(name, default: "line") + ")"
}

/// The wrapper: in HTML - div.k-plot with JSON and the frame inside, in PDF - the frame and a caption.
/// legend - curves with labels (below the figure: on the plot area a label overlaps the axes).
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
