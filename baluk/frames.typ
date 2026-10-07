// Frames: a figure with a parameter - a slider and playback.
//
//   #fig(frames(n => canvas(theme => {
//     import cetz.draw: *
//     for i in range(n) { rect((i * 0.5, 0), (i * 0.5 + 0.4, 0.4)) }
//   }), n: (from: 1, to: 8, value: 3)), [n rectangles])
//
// Typst builds the figure for each parameter value. In the app the figure has
// a slider below it, "play" shows the frames in turn (animation, algorithm
// steps); in PDF and without JS - the frame at the default value and the
// caption "n = 3" (or several frames in a row: `pdf: (1, 4, 8)`). Typst
// computes everything: any figure fits, not only a formula.
//
// In HTML - `div.k-frames` with `data-k-frames` (JSON: name, frame count,
// default index, speed) and frames `div.k-frames-item` inside; the default
// frame is marked `data-k-frame-default`. Each frame is a plain `canvas`
// (`div.k-frame`): the core merges themes and shared glyphs as for any figure.

#import "theme.typ": current-theme
#import "web.typ": is-web, elem
#import "plots/common.typ": _num
#import "i18n.typ": word
#import "figures/canvas.typ": in-row

/// More frames - a heavier page (each frame is its own SVG).
#let _max-frames = 60

/// Parameter values from (from:, to:, step:, value:) or (values: (...), value:).
/// -> (values, index of the default value, step for the caption or none)
#let _values(name, spec) = {
  let fail(msg) = panic("frames: parameter \"" + name + "\": " + msg)
  let spec = if type(spec) == array { (values: spec) } else { spec }
  if type(spec) != dictionary {
    fail("needs a dictionary (from: 1, to: 10, step: 1, value: 5) or an array of values")
  }
  let extra = spec.keys().filter(k => k not in ("from", "to", "step", "value", "values"))
  if extra.len() > 0 { fail("unknown keys: " + extra.join(", ") + "; known: from, to, step, value, values") }
  let (values, step) = if "values" in spec {
    if "from" in spec or "to" in spec or "step" in spec { fail("either values or from/to/step") }
    if type(spec.values) != array or spec.values.len() == 0 { fail("values is a non-empty array") }
    (spec.values, none)
  } else {
    if "from" not in spec or "to" not in spec { fail("needs from and to (or values)") }
    let (from, to) = (spec.from, spec.to)
    let step = spec.at("step", default: 1)
    if not (step > 0) { fail("needs step > 0") }
    if not (to >= from) { fail("needs from ≤ to") }
    let count = calc.floor((to - from) / step + 1e-9) + 1
    if count > _max-frames { fail(str(count) + " frames, more than " + str(_max-frames) + " - increase step") }
    // integers stay integers (caption "n = 3", not "n = 3.0")
    let whole = type(from) == int and type(step) == int
    (range(count).map(i => if whole { from + i * step } else { from + i * step * 1.0 }), step)
  }
  if values.len() > _max-frames { fail(str(values.len()) + " frames, more than " + str(_max-frames)) }
  let default = if "value" in spec {
    let i = values.position(v => v == spec.value)
    // a value between range steps - the nearest frame
    if i == none and step != none and type(spec.value) in (int, float) {
      let dist = values.map(v => calc.abs(v - spec.value))
      i = dist.position(d => d == calc.min(..dist))
    }
    if i == none { fail("value is one of the values") }
    i
  } else { 0 }
  (values, default, step)
}

/// A value caption: a number - "n = 2.5", a string or content - "n = ...",
/// anything else - "frame 3 of 8".
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

/// A frames figure: the body is a function parameter value -> figure (usually canvas).
/// Exactly one parameter, as a named argument:
///   n: (from: 1, to: 20, step: 1, value: 5)   - a range (step defaults to 1)
///   k: (values: (1, 2, 4, 8), value: 4)        - explicit values (or just an array)
/// label: auto - "n = 5" under the frame; none - no caption; a function v => [...] - own.
/// fps - frames per second in playback; loop - play in a loop.
/// pdf: auto - the default frame in PDF; frame numbers (from 1), e.g.
///   (1, 4, 8) - these frames in a row, each with its caption (HTML does not change).
#let frames(body, label: auto, fps: 2, loop: false, pdf: auto, ..param) = {
  if param.pos().len() > 0 { panic("frames: extra positional arguments; the parameter is named: n: (from: 1, to: 10)") }
  let named = param.named()
  if named.len() != 1 {
    panic("frames: needs exactly one parameter, e.g. n: (from: 1, to: 10); got: " + if named.len() == 0 { "none" } else { named.keys().join(", ") })
  }
  if type(body) != function { panic("frames: the first argument is a function of the value, e.g. n => canvas(...)") }
  // the same limits as the client's (app/src/lib/frames.ts)
  if not (type(fps) in (int, float) and fps > 0 and fps <= 60) { panic("frames: fps is a number from 0 to 60, e.g. 2") }
  if type(loop) != bool { panic("frames: loop is true or false") }
  let (name, spec) = named.pairs().first()
  let (values, default, step) = _values(name, spec)
  let count = values.len()
  if pdf != auto {
    let bad = type(pdf) != array or pdf.len() == 0 or pdf.any(k => type(k) != int or k < 1 or k > count)
    if bad { panic("frames: pdf is auto or frame numbers from 1 to " + str(count) + ", e.g. (1, " + str(calc.min(count, 4)) + ")") }
  }
  let caption(v, i) = if label == none { none } else if label == auto {
    _default-label(name, v, i, count, step)
  } else if type(label) == function { label(v) } else {
    panic("frames: label is auto, none or a function of the value")
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
    // A frame with a caption under it.
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
