// Пример навыка /baluk-note: АЛГОРИТМЫ И СТРУКТУРЫ ДАННЫХ — массивы,
// таблицы ДП, графы, деревья, работа алгоритма кадрами, трассировка.
// Клетки подсвечиваются словарём ("индекс": "цвет"): ключ — строка.
#import "/_baluk/lib.typ": *
#show: note.with(title: [Алгоритмы], tags: ("пример", "алгоритмы"))

= Массивы

// array-cells(значения, highlight:, spans:, pointers:, arcs:, block-size:, block-values:, index-from:).
#fig(
  canvas(array-cells(
    (3, 1, 4, 1, 5, 9, 2, 6), index-from: 0,
    highlight: ("2": "second", "3": "second", "4": "second", "5": "second"),
    spans: ((2, 5, "second", [окно $[2, 5]$]),),          // скобка под отрезком: (от, до, цвет, подпись)
    pointers: ((2, $l$), (5, $r$)),                        // стрелки над клетками: (индекс, подпись)
  )),
  [Окно $[2, 5]$: сумма $4 + 1 + 5 + 9 = 19$],
)

#fig(
  canvas(array-cells(
    (3, 1, 4, 1, 5, 9, 2, 6), block-size: 4, block-values: (9, 22),   // блоки и своды над ними
    arcs: ((1, 6, [обмен], "third"),),                                  // дуга между клетками
  )),
  [Два блока по четыре клетки со сводами-суммами и дуга обмена клеток 1 и 6],
)

= Таблица ДП

// matrix-cells(матрица, cells: ("строка,столбец": цвет), arrows: ((r1, c1, r2, c2), …)); нумерация с 1.
#let paths = ((1, 1, 1, 1), (1, 2, 3, 4), (1, 3, 6, 10))
#side-by-side(
  [
    Число путей вправо-вниз в клетку равно сумме соседа сверху и соседа
    слева: $10 = 4 + 6$. Стрелки показывают, откуда пришло значение.
  ],
  fig(
    canvas(matrix-cells(paths, cells: ("3,4": "line", "2,4": "second", "3,3": "second"),
      arrows: ((2, 4, 3, 4), (3, 3, 3, 4)))),
    [Переход в клетку $(3, 4)$],
  ),
  width: 42%,
)

= Графы и деревья

// graph(вершины: ("имя": (x, y)), рёбра: ((u, v, стиль, подпись), …), directed:, highlight:, marks:).
// Стили рёбер: "normal", "bold", "second", "dim", "dashed". Вершины можно разложить помощниками.
#let edges = (("1", "2"), ("1", "3"), ("2", "4"), ("2", "5"), ("3", "6"))
#fig(
  in-row(
    canvas(graph(tree-layout(edges, "1"), edges.map(e => (e.at(0), e.at(1), "bold")),
      highlight: ("1",), marks: ("1": ([$d = 0$], "west"), "4": ([$d = 2$], "west")))),
    canvas(graph(circle-layout(("a", "b", "c", "d")),
      (("a", "b", "normal", [3]), ("b", "c", "second", [1]), ("c", "d", "normal", [4]),
       ("d", "a", "dashed", [2], (side: "right", at: 0.3))),   // подпись справа, ближе к началу
      directed: true, highlight: ("b",))),
    gap: 2em,
  ),
  [Слева — дерево обхода в ширину по уровням; справа — ориентированный цикл с весами],
)

// Дерево поиска: binary-layout раскладывает по порядку ключей, binary-edges даёт рёбра.
#let children = ("4": ("2", "6"), "2": ("1", "3"), "6": ("5", none))
#fig(
  canvas(graph(binary-layout(children, "4"), binary-edges(children), highlight: ("4",))),
  [Дерево поиска: слева от вершины — меньшие ключи, справа — большие],
)

// current-theme() — тема вне canvas(theme => …), только внутри context: здесь — серые приоритеты.
#let prio = ("4": 9, "2": 7, "6": 5, "1": 3, "3": 2, "5": 4)
#context fig(
  canvas(graph(binary-layout(children, "4"), binary-edges(children), shape: "rect", size: (0.62, 0.52),
    labels: prio.keys().map(k => (k, text(size: 0.78em)[#k \ #text(size: 0.82em, fill: current-theme().color.muted)[#prio.at(k)]])).to-dict())),
  [Декартово дерево: по ключам — дерево поиска, по приоритетам (серым) — куча],
)

= Алгоритм кадрами

// frames(k => рисунок, k: (values: …) или (from:, to:, step:, value:)) — в приложении ползунок и проигрывание.
// label: — подпись кадра; pdf: — какие кадры (номера с 1) показать в PDF в ряд.
#let passes = ((5, 2, 4, 1), (2, 4, 1, 5), (2, 1, 4, 5), (1, 2, 4, 5))
#fig(
  frames(k => canvas(array-cells(passes.at(k),
      highlight: range(4 - k, 4).map(i => (str(i), "third")).to-dict())),
    k: (values: (0, 1, 2, 3)),
    label: k => if k == 0 [исходный массив] else [проход #k],
    pdf: (1, 2, 3, 4)),
  [Пузырьковая сортировка: после прохода $k$ последние $k$ элементов на месте],
)

// Трассировка — data-table с подсветкой строки шага.
Два указателя ищут пару с суммой $7$ в массиве $(1, 2, 4, 5)$:

#data-table(
  (auto, auto, auto, 1fr),
  header: ([шаг], [$l$], [$r$], [что делаем]),
  highlight: ("2,*": "third"),
  [1], [0], [3], [$1 + 5 = 6 < 7$ — двигаем $l$ вправо],
  [2], [1], [3], [$2 + 5 = 7$ — нашли],
)
