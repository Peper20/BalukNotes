// Interactive figures: a plot with sliders and a rotatable surface.
//
//   #fig(interactive-plot("a * calc.sin(b * x)", -5, 5,
//     params: (a: (from: 0, to: 3, value: 1), b: (from: 0.5, to: 4, step: 0.1, value: 1)),
//     y: (-3, 3)), [Sine amplitude and frequency])
//
// A formula is a string of Typst code from a subset (`plots/formula.typ`):
// both Typst (the frame for PDF and for a page without JS) and the app client
// (`app/src/lib/plot/`) evaluate it - parsing and evaluation there repeat
// these LINE BY LINE. Change one - change the other; the cross-check is the
// fixture `tests/vault/Рисунки/Интерактив.typ` (Vitest reads its snapshot).
//
// In HTML a figure is `div.k-plot` with `data-k-plot` (JSON: formulas,
// ranges, parameters) and a plain `canvas` frame inside; the client hides
// the frame and draws a live figure. In PDF - the frame at default values
// and a caption with them.
//
// Our own parser, not `eval`: `eval` fails the build on division by zero,
// `calc.sqrt` of a negative and so on - on a plot that is just a gap in the
// curve.
//
// Parts in `plots/`:
//   formula.typ  formula parsing and evaluation (line by line as the client's formula.ts)
//   common.typ   parameters, axis ticks, numbers, the figure wrapper
//   plot.typ     interactive-plot - a plot with sliders
//   surface.typ  interactive-surface - a rotatable surface

#import "plots/formula.typ": *
#import "plots/common.typ": *
#import "plots/plot.typ": *
#import "plots/surface.typ": *
