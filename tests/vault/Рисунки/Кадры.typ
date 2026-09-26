#import "/_baluk/lib.typ": *
#show: note.with(title: [Кадры], tags: ("рисунки", "фикстура"))

Рисунок с параметром: Typst собирает кадр для каждого значения, в
приложении их переключает ползунок, кнопка проигрывания показывает по очереди. В PDF и
без JS — кадр по умолчанию и подпись значения.

= Диапазон целых

#fig(frames(n => canvas(unit: 1.4cm, theme => {
  import cetz.draw: *
  // рамка постоянного размера: кадры одинаковой величины не «прыгают»
  rect((-0.1, -0.1), (3.1, 1.15), stroke: none)
  axes(x: (0, 3.1), y: (0, 1.15), labels: ($x$, $y$), origin: none)
  let f = x => 1 - x * x / 9
  let h = 3 / n
  for i in range(n) {
    let x = i * h
    rect((x, 0), (x + h, f(x + h)), fill: theme.color.fig.fill, stroke: 0.5pt + theme.color.fig.line)
  }
  plot(f, 0, 3)
}), n: (from: 1, to: 12, value: 4)), [Нижняя сумма Римана: $n$ прямоугольников])

= Дробный шаг и своя подпись

#fig(frames(t => canvas(theme => {
  import cetz.draw: *
  rect((-1.25, -1.25), (1.25, 1.25), stroke: none)
  circle((0, 0), radius: 1, stroke: 0.6pt + theme.color.fig.axis)
  let a = t * 360deg
  line((0, 0), (calc.cos(a), calc.sin(a)), stroke: 1.1pt + theme.color.fig.second)
  point((calc.cos(a), calc.sin(a)))
}), t: (from: 0, to: 1, step: 0.125, value: 0.25), label: t => [поворот на $#calc.round(t * 360)°$], fps: 4, loop: true),
  [Точка на окружности; проигрывание — по кругу])

= Шаги алгоритма

#let passes = ((5, 2, 4, 1), (2, 4, 1, 5), (2, 1, 4, 5), (1, 2, 4, 5))
#fig(frames(k => canvas(theme => {
  array-cells(passes.at(k), highlight: range(4 - k, 4).map(i => (str(i), "third")).to-dict())
}), k: (values: (0, 1, 2, 3)), label: k => if k == 0 [исходный массив] else [проход #k]),
  [Пузырьковая сортировка: после прохода $k$ последние $k$ элементов на месте])
