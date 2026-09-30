// Skill sample: INTERACTIVE figures: a plot with sliders, a rotating
// surface, frames with a number. Live in the app; the PDF shows the frame
// at the default values (value:).
#import "/_baluk/lib.typ": *
#show: note.with(lang: "en", title: [Interactive], tags: ("example", "figures"))

= Parabola with sliders

// interactive-plot(FORMULA-STRING, x-from, x-to, params: (...), y: (...)), always inside #fig(..., [Caption]).
// The formula is a STRING: x, parameter names, + - * /, parentheses, calc.sin(...) etc.
// No "**" or "^": x^2 is x * x or calc.pow(x, 2).
// A parameter: name: (from: min, to: max, step: step, value: initial).
#fig(
  interactive-plot(
    "a * x * x + b * x + c",
    -5, 5,
    params: (
      a: (from: -3, to: 3, step: 0.1, value: 1),
      b: (from: -5, to: 5, step: 0.5, value: 0),
      c: (from: -5, to: 5, step: 0.5, value: 0),
    ),
    y: (-10, 10),
  ),
  [Parabola $y = a x^2 + b x + c$: $a$ changes the width and direction of the branches, $c$ shifts it up and down],
)

= Several curves

// Several curves: an array; a styled curve: a dict (f:, label:, dashed:, color:).
// labels: axis labels; width, height: plot size in cm (numbers, no cm).
#fig(
  interactive-plot(
    ("a * calc.sin(b * x)", (f: "calc.sin(x)", label: "sin x", dashed: true, color: "second")),
    -5, 5,
    params: (a: (from: 0, to: 3, step: 0.1, value: 1), b: (from: 0.5, to: 4, step: 0.1, value: 1)),
    y: (-3, 3), labels: ("x", "y"), width: 8, height: 5,
  ),
  [Amplitude $a$ stretches the sine vertically, frequency $b$ compresses it horizontally],
)

= Surface

// interactive-surface(formula of x and y, (x-from, x-to), (y-from, y-to), params:, z:): rotates with the mouse.
#fig(
  interactive-surface("calc.sin(k * x) * calc.cos(y)", (-3, 3), (-3, 3),
    params: (k: (from: 0.2, to: 2, step: 0.1, value: 1)), z: (-1, 1),
    style: "shaded", rotation: -30, tilt: 28),
  [Surface $z = sin k x cos y$: larger $k$ makes waves along $x$ denser],
)

= Frames with a number

// frames(n => canvas(...), n: (from:, to:, value:)): any figure with a parameter.
// An invisible rect(..., stroke: none) of constant size keeps frames from jumping.
#fig(
  frames(n => canvas(unit: 1.4cm, theme => {
    import cetz.draw: *
    rect((-0.1, -0.1), (3.1, 1.15), stroke: none)
    axes(x: (0, 3.1), y: (0, 1.15), origin: none)
    let f = x => 1 - x * x / 9
    let h = 3 / n
    for i in range(n) {
      rect((i * h, 0), ((i + 1) * h, f((i + 1) * h)), fill: theme.color.fig.fill, stroke: 0.5pt + theme.color.fig.line)
    }
    plot(f, 0, 3)
  }), n: (from: 1, to: 12, value: 4)),
  [Lower Riemann sum: as $n$ grows, the rectangles fill the area under the curve],
)
