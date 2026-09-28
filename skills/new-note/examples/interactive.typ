// Пример навыка /new-note: ИНТЕРАКТИВ — график с ползунками, поверхность с
// вращением, кадры с числовым параметром. В приложении рисунок живой, в PDF —
// кадр при значениях по умолчанию (value:).
#import "/_baluk/lib.typ": *
#show: note.with(title: [Интерактив], tags: ("пример", "рисунки"))

= Парабола с ползунками

// interactive-plot(ФОРМУЛА-СТРОКА, x-от, x-до, params: (…), y: (…)) — и всё это внутри #fig(…, [Подпись]).
// Формула — СТРОКА в кавычках: x, имена параметров, + - * /, скобки, calc.sin(…) и т. п.
// Степени «**» и «^» нет: x² — это x * x или calc.pow(x, 2).
// Параметр: имя: (from: мин, to: макс, step: шаг, value: начальное).
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
  [Парабола $y = a x^2 + b x + c$: $a$ меняет раствор и направление ветвей, $c$ сдвигает вверх-вниз],
)

= Несколько кривых

// Несколько кривых — массив; кривая со стилем — словарь (f:, label:, dashed:, color:).
// labels — подписи осей; width, height — размер поля в см (числа, без cm).
#fig(
  interactive-plot(
    ("a * calc.sin(b * x)", (f: "calc.sin(x)", label: "sin x", dashed: true, color: "second")),
    -5, 5,
    params: (a: (from: 0, to: 3, step: 0.1, value: 1), b: (from: 0.5, to: 4, step: 0.1, value: 1)),
    y: (-3, 3), labels: ("x", "y"), width: 8, height: 5,
  ),
  [Амплитуда $a$ растягивает синусоиду по вертикали, частота $b$ сжимает по горизонтали],
)

= Поверхность

// interactive-surface(формула от x и y, (x-от, x-до), (y-от, y-до), params:, z:) — вращается мышью.
#fig(
  interactive-surface("calc.sin(k * x) * calc.cos(y)", (-3, 3), (-3, 3),
    params: (k: (from: 0.2, to: 2, step: 0.1, value: 1)), z: (-1, 1),
    style: "shaded", rotation: -30, tilt: 28),
  [Поверхность $z = sin k x cos y$: при большом $k$ волны вдоль $x$ чаще],
)

= Кадры с числовым параметром

// frames(n => canvas(…), n: (from:, to:, value:)) — любой рисунок с параметром.
// Невидимая рамка rect(…, stroke: none) постоянного размера — чтобы кадры не «прыгали».
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
  [Нижняя сумма Римана: с ростом $n$ прямоугольники заполняют площадь под кривой],
)
