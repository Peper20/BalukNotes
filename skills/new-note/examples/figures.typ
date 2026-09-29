// Пример навыка /new-note: РИСУНКИ — 2D, диаграммы по данным, 3D.
// Каждый рисунок: #fig(canvas(…), [Подпись-утверждение]). Копируй вызов,
// меняй функции, диапазоны и подписи. Цвета — только именами темы
// ("line", "second", "third", "accent") или theme.color.fig.*.
#import "/_baluk/lib.typ": *
#show: note.with(title: [Рисунки], tags: ("пример", "рисунки"))

= Графики функций

// canvas(unit: …, theme => { … }) — холст; theme — текущая тема (для своих цветов).
// Внутри — помощники библиотеки; import cetz.draw: * — если нужны свои line/rect/circle.
// Ссылка на рисунок: label: "рис-…" и в тексте рис. @рис-….
Площадь между $y = x$ и $y = x^2$ залита (рис. @рис-площадь): спица
показывает, что при фиксированном $x$ точка идёт от параболы к прямой.

#fig(
  canvas(unit: 2.5cm, theme => {
    import cetz.draw: *
    fill-between(x => x * x, x => x, 0, 1)                // заливка между нижней и верхней кривой
    axes(x: (-0.1, 1.3), y: (-0.1, 1.2))                  // оси: диапазоны x и y
    plot(x => x * x, 0, 1.08, label: $y = x^2$, label-x: 0.9, label-anchor: "north-west")
    plot(x => x, 0, 1.08, color: "third", dashed: true, label: $y = x$, label-x: 0.7, label-anchor: "south-east")
    spoke(0.6, 0.36, 0.6)                                 // вертикальная двусторонняя стрелка x = 0.6 от 0.36 до 0.6
    point((1, 1), label: $(1, 1)$)                        // точка с подписью
    tick(1, $1$)                                          // засечка на оси x
    tick(0.6, $dc("0,6")$)
    line((0.6, 0), (0.6, 0.36), stroke: (paint: theme.color.fig.second, dash: "dotted"))   // своя линия — цветом темы
  }),
  [Спица при $x = dc("0,6")$ входит в область через параболу и выходит через прямую],
  label: "рис-площадь",
)

// Параметрическая кривая, горизонтальная спица, бледная заливка pale(theme, цвет).
#fig(
  canvas(unit: 1.1cm, theme => {
    import cetz.draw: *
    circle((0, 0), radius: 1.3, fill: pale(theme, theme.color.fig.second), stroke: none)
    axes(x: (-1.5, 1.6), y: (-1.4, 1.5))
    parametric(t => calc.cos(t), t => calc.sin(t), 0, 2 * calc.pi, closed: true, fill-color: auto)
    spoke(0, -1, 1, horizontal: true, color: "third")     // горизонтальная: y = 0, x от −1 до 1
  }),
  [Единичная окружность $x = cos t$, $y = sin t$ лежит внутри бледного круга радиуса $dc("1,3")$],
)

// Линии уровня: contours(f, уровни, xr:, yr:).
#fig(
  canvas(unit: 0.6cm, {
    contours((x, y) => x * x + y * y / 2, (0.5, 1.5, 3), xr: (-2.5, 2.5), yr: (-2.5, 2.5), n: 50)
    axes(x: (-2.8, 2.8), y: (-2.8, 2.8))
  }),
  [Линии уровня $x^2 + y^2 / 2 = c$ — эллипсы, вытянутые вдоль $y$],
)

= Несколько рисунков в ряд

// in-row — холсты в ряд; separators — знаки между ними (формула из картинок).
#fig(
  in-row(
    canvas(unit: 0.7cm, { axes(x: (-0.2, 3), y: (-0.2, 2)); plot(x => x / 2, 0, 3) }),
    canvas(unit: 0.7cm, { axes(x: (-0.2, 3), y: (-0.2, 2)); plot(x => 2 - x / 2, 0, 3, color: "second") }),
    separators: ($+$,),
  ),
  [Сумма двух линейных функций постоянна: $x / 2 + (2 - x / 2) = 2$],
)

= Диаграммы по данным

// chart(ряд, ряд, …, width:, height:, labels:, legend:) — внутри canvas.
// Ряды: "line" (points:, dashed:), "points", "steps", "bars" ((подпись, значение), …), "function" (f, from, to).
#fig(
  canvas(chart(
    (kind: "line", points: true, label: [перебор], data: ((1, 1), (2, 4), (3, 9), (4, 16))),
    (kind: "function", f: x => x * calc.log(x, base: 2) + 1, from: 1, to: 4, label: [$n log n$], color: "third"),
    (kind: "points", label: [замеры], color: "second", data: ((1.5, 3), (2.5, 5), (3.5, 8))),
    width: 5.2, height: 3.2, labels: ([$n$], [мс]), legend: "inside",
  )),
  [Перебор растёт квадратично: при $n = 4$ он уже почти вдвое медленнее, чем $n log n$],
)

#fig(
  in-row(
    canvas(chart((kind: "bars", data: (([массив], 4), ([префиксы], 8), ([дерево], 16))),
      width: 4.4, height: 3, labels: (none, [МБ]), legend: none)),
    canvas(chart((kind: "steps", data: ((0, 0), (1, 0.25), (2, 0.75), (3, 1))),
      width: 4.4, height: 3, labels: ([$x$], [$F(x)$]), legend: none)),
  ),
  [Слева — память растёт вдвое на каждом уровне; справа — функция распределения ступеньками],
)

= Пространство (3D)

// 3D: x вправо, z вверх, y — «от нас». p3(x, y, z) — точка холста; axes3d — оси.
#let bump(x, y) = 1.2 - 0.15 * (x - 1.5) * (x - 1.5) - 0.2 * (y - 1.2) * (y - 1.2)

#fig(
  in-row(
    canvas(unit: 0.9cm, {
      surface(bump, (0, 3), (0, 2.4), n: 12, style: "shaded")   // "shaded" / "flat" / "wire"
      cross-section(bump, 1.2, xr: (0, 3))                      // сечение плоскостью y = 1.2
      axes3d(x: 3.6, y: 3.1, z: 2.2)
    }),
    canvas(unit: 0.9cm, {
      base-shape((0, 0), (3, 0), (3, 2.4), (0, 2.4), label: $D$)   // область в плоскости z = 0
      prisms(bump, (0, 3), (0, 2.4), n: 4, m: 3, gap: 0.05, highlight: ((1, 1),))
      axes3d(x: 3.6, y: 3.1, z: 2.2)
    }),
    canvas(unit: 0.9cm, theme => {
      import cetz.draw: *
      revolution(z => 0.4 + 0.3 * z, zmin: 0, zmax: 2, rings: 6)   // тело вращения радиуса r(z)
      line(p3(0, 0, 0), p3(0, 0, 2.2), stroke: (paint: theme.color.fig.second, dash: "dashed"))
      axes3d(x: 1.4, y: 1.4, z: 2.6)
    }),
  ),
  [Объём под поверхностью: сечение $y = dc("1,2")$, интегральная сумма столбиками
   над $D$ и тело вращения вокруг оси $z$],
)

// Крупный рисунок в PDF можно отпустить вверх/вниз страницы: floating: true.
#fig(
  canvas(unit: 1.2cm, {
    surface((x, y) => 0.3 * (x * x - y * y), (-1.5, 1.5), (-1.5, 1.5), n: 10, style: "wire")
    axes3d(x: 2, y: 2, z: 1)
  }),
  [Седло $z = dc("0,3") (x^2 - y^2)$: вдоль $x$ — минимум, вдоль $y$ — максимум],
  floating: true,
)
