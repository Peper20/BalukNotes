// Figures on cetz 0.4.2 in the current theme's colors.
//
// All helpers return cetz elements and take colors from the theme themselves
// (via get-ctx -> state), so they work in any cetz.canvas. Handiest inside
// #canvas(theme => { ... }), where theme is the current theme:
//
//   #fig(canvas(theme => {
//     import cetz.draw: *
//     axes(x: (-0.5, 4), y: (-0.5, 3))
//     plot(x => x * x / 4, 0, 3.4, label: $y = x^2/4$)
//     line((1, 0), (1, 2), stroke: 1pt + theme.color.fig.second)
//   }), [Caption: what exactly the figure shows])
//
// AXIS CONVENTION: in all helpers the y axis points UP (as in math), EXCEPT
// `matrix-cells` and `array-cells`, where rows go top to bottom, as in a
// matrix. In the ICPC notes a mix-up of axis direction gave mirrored figures
// without a single build error.
//
// IMPORTANT: inside `import cetz.draw: *` the names line/rect/content/fill/
// stroke are cetz functions - do not name your variables so.

//
// Helpers are split by topic in `figures/`; this file gathers them
// (`lib.typ` and `charts.typ` import it):
//   canvas.typ  canvas, captioned figure, figures in a row, color by name
//   plane.typ   2D: axes, functions, parametric curves, fills, contour lines
//   space.typ   pseudo-3D: axes, surfaces, prisms, solids of revolution, sections
//   cells.typ   arrays and matrices
//   graphs.typ  graphs and tree layouts

#import "figures/canvas.typ": *
#import "figures/plane.typ": *
#import "figures/space.typ": *
#import "figures/cells.typ": *
#import "figures/graphs.typ": *
