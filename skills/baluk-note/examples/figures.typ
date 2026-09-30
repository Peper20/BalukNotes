// Skill sample: FIGURES, 2D, data charts, 3D.
// Every figure: #fig(canvas(...), [Caption as a statement]). Start from a call
// and extend it freely with own CeTZ drawing. Colors only by theme names
// ("line", "second", "third", "accent") or theme.color.fig.*.
#import "/_baluk/lib.typ": *
#show: note.with(lang: "en", title: [Figures], tags: ("example", "figures"))

= Function graphs

// canvas(unit: ..., theme => { ... }): theme is the current theme (for own colors).
// import cetz.draw: * only if you need own line/rect/circle.
// Figure reference: label: "fig-..." and in text fig. @fig-....
The area between $y = x$ and $y = x^2$ is filled (fig. @fig-area): the spoke
shows that for a fixed $x$ the point goes from the parabola to the line.

#fig(
  canvas(unit: 2.5cm, theme => {
    import cetz.draw: *
    fill-between(x => x * x, x => x, 0, 1)                // fill between lower and upper curve
    axes(x: (-0.1, 1.3), y: (-0.1, 1.2))                  // axes: x and y ranges
    plot(x => x * x, 0, 1.08, label: $y = x^2$, label-x: 0.9, label-anchor: "north-west")
    plot(x => x, 0, 1.08, color: "third", dashed: true, label: $y = x$, label-x: 0.7, label-anchor: "south-east")
    spoke(0.6, 0.36, 0.6)                                 // vertical double arrow at x = 0.6 from 0.36 to 0.6
    point((1, 1), label: $(1, 1)$)
    tick(1, $1$)                                          // tick on the x axis
    tick(0.6, $0.6$)
    line((0.6, 0), (0.6, 0.36), stroke: (paint: theme.color.fig.second, dash: "dotted"))   // own line in a theme color
  }),
  [The spoke at $x = 0.6$ enters the region through the parabola and leaves through the line],
  label: "fig-area",
)

// Parametric curve, horizontal spoke, pale fill pale(theme, color).
#fig(
  canvas(unit: 1.1cm, theme => {
    import cetz.draw: *
    circle((0, 0), radius: 1.3, fill: pale(theme, theme.color.fig.second), stroke: none)
    axes(x: (-1.5, 1.6), y: (-1.4, 1.5))
    parametric(t => calc.cos(t), t => calc.sin(t), 0, 2 * calc.pi, closed: true, fill-color: auto)
    spoke(0, -1, 1, horizontal: true, color: "third")     // horizontal: y = 0, x from -1 to 1
  }),
  [The unit circle $x = cos t$, $y = sin t$ lies inside the pale disk of radius $1.3$],
)

// Level lines: contours(f, levels, xr:, yr:).
#fig(
  canvas(unit: 0.6cm, {
    contours((x, y) => x * x + y * y / 2, (0.5, 1.5, 3), xr: (-2.5, 2.5), yr: (-2.5, 2.5), n: 50)
    axes(x: (-2.8, 2.8), y: (-2.8, 2.8))
  }),
  [Level lines $x^2 + y^2 / 2 = c$ are ellipses stretched along $y$],
)

= Figures in a row

// in-row: canvases in a row; separators: signs between them.
#fig(
  in-row(
    canvas(unit: 0.7cm, { axes(x: (-0.2, 3), y: (-0.2, 2)); plot(x => x / 2, 0, 3) }),
    canvas(unit: 0.7cm, { axes(x: (-0.2, 3), y: (-0.2, 2)); plot(x => 2 - x / 2, 0, 3, color: "second") }),
    separators: ($+$,),
  ),
  [The sum of two linear functions is constant: $x / 2 + (2 - x / 2) = 2$],
)

= Data charts

// chart(series, series, ...) inside canvas; a series is a dict with kind: "line", "points", "steps", "bars", "function".
#fig(
  canvas(chart(
    (kind: "line", points: true, label: [brute force], data: ((1, 1), (2, 4), (3, 9), (4, 16))),
    (kind: "function", f: x => x * calc.log(x, base: 2) + 1, from: 1, to: 4, label: [$n log n$], color: "third"),
    (kind: "points", label: [measured], color: "second", data: ((1.5, 3), (2.5, 5), (3.5, 8))),
    width: 5.2, height: 3.2, labels: ([$n$], [ms]), legend: "inside",
  )),
  [Brute force grows quadratically: at $n = 4$ it is almost twice as slow as $n log n$],
)

#fig(
  in-row(
    canvas(chart((kind: "bars", data: (([array], 4), ([prefix], 8), ([tree], 16))),
      width: 4.4, height: 3, labels: (none, [MB]), legend: none)),
    canvas(chart((kind: "steps", data: ((0, 0), (1, 0.25), (2, 0.75), (3, 1))),
      width: 4.4, height: 3, labels: ([$x$], [$F(x)$]), legend: none)),
  ),
  [Left: memory doubles at each level; right: a distribution function as steps],
)

= Space (3D)

// 3D: x right, z up, y away. p3(x, y, z) is a canvas point; axes3d draws axes.
#let bump(x, y) = 1.2 - 0.15 * (x - 1.5) * (x - 1.5) - 0.2 * (y - 1.2) * (y - 1.2)

#fig(
  in-row(
    canvas(unit: 0.9cm, {
      surface(bump, (0, 3), (0, 2.4), n: 12, style: "shaded")   // "shaded" / "flat" / "wire"
      cross-section(bump, 1.2, xr: (0, 3))                      // section by the plane y = 1.2
      axes3d(x: 3.6, y: 3.1, z: 2.2)
    }),
    canvas(unit: 0.9cm, {
      base-shape((0, 0), (3, 0), (3, 2.4), (0, 2.4), label: $D$)   // region in the plane z = 0
      prisms(bump, (0, 3), (0, 2.4), n: 4, m: 3, gap: 0.05, highlight: ((1, 1),))
      axes3d(x: 3.6, y: 3.1, z: 2.2)
    }),
    canvas(unit: 0.9cm, theme => {
      import cetz.draw: *
      revolution(z => 0.4 + 0.3 * z, zmin: 0, zmax: 2, rings: 6)   // solid of revolution with radius r(z)
      line(p3(0, 0, 0), p3(0, 0, 2.2), stroke: (paint: theme.color.fig.second, dash: "dashed"))
      axes3d(x: 1.4, y: 1.4, z: 2.6)
    }),
  ),
  [Volume under a surface: the section $y = 1.2$, a Riemann sum of bars
   over $D$ and a solid of revolution around the $z$ axis],
)

// A big figure may float to the top or bottom of a PDF page: floating: true.
#fig(
  canvas(unit: 1.2cm, {
    surface((x, y) => 0.3 * (x * x - y * y), (-1.5, 1.5), (-1.5, 1.5), n: 10, style: "wire")
    axes3d(x: 2, y: 2, z: 1)
  }),
  [Saddle $z = 0.3 (x^2 - y^2)$: a minimum along $x$, a maximum along $y$],
  floating: true,
)
