// Skill sample: ALGORITHMS AND DATA STRUCTURES: arrays, DP tables, graphs,
// trees, an algorithm as frames, a trace. Cells are highlighted by a dict
// ("index": "color"): the key is a string.
#import "/_baluk/lib.typ": *
#show: note.with(lang: "en", title: [Algorithms], tags: ("example", "algorithms"))

= Arrays

// Array: highlight, bracket below a range, pointers above.
#fig(
  canvas(array-cells(
    (3, 1, 4, 1, 5, 9, 2, 6), index-from: 0,
    highlight: ("2": "second", "3": "second", "4": "second", "5": "second"),
    spans: ((2, 5, "second", [window $[2, 5]$]),),        // bracket: (from, to, color, label)
    pointers: ((2, $l$), (5, $r$)),                        // arrows: (index, label)
  )),
  [Window $[2, 5]$: sum $4 + 1 + 5 + 9 = 19$],
)

#fig(
  canvas(array-cells(
    (3, 1, 4, 1, 5, 9, 2, 6), block-size: 4, block-values: (9, 22),   // blocks and boxes above
    arcs: ((1, 6, [swap], "third"),),                                   // arc between cells
  )),
  [Two blocks of four cells with block sums and a swap arc of cells 1 and 6],
)

= DP table

// matrix-cells(matrix, cells: ("row,col": color), arrows: ((r1, c1, r2, c2), ...)); numbering from 1.
#let paths = ((1, 1, 1, 1), (1, 2, 3, 4), (1, 3, 6, 10))
#side-by-side(
  [
    The number of right-down paths to a cell is the sum of its upper and
    left neighbors: $10 = 4 + 6$. Arrows show where the value came from.
  ],
  fig(
    canvas(matrix-cells(paths, cells: ("3,4": "line", "2,4": "second", "3,3": "second"),
      arrows: ((2, 4, 3, 4), (3, 3, 3, 4)))),
    [Transition into cell $(3, 4)$],
  ),
  width: 42%,
)

= Graphs and trees

// graph(vertices, edges): vertices ("name": (x, y)) or a layout helper; edge (u, v, style, label).
#let edges = (("1", "2"), ("1", "3"), ("2", "4"), ("2", "5"), ("3", "6"))
#fig(
  in-row(
    canvas(graph(tree-layout(edges, "1"), edges.map(e => (e.at(0), e.at(1), "bold")),
      highlight: ("1",), marks: ("1": ([$d = 0$], "west"), "4": ([$d = 2$], "west")))),
    canvas(graph(circle-layout(("a", "b", "c", "d")),
      (("a", "b", "normal", [3]), ("b", "c", "second", [1]), ("c", "d", "normal", [4]),
       ("d", "a", "dashed", [2], (side: "right", at: 0.3))),   // label on the right, near the start
      directed: true, highlight: ("b",))),
    gap: 2em,
  ),
  [Left: a BFS tree by levels; right: a directed weighted cycle],
)

// Search tree: binary-layout orders by keys, binary-edges gives the edges.
#let children = ("4": ("2", "6"), "2": ("1", "3"), "6": ("5", none))
#fig(
  canvas(graph(binary-layout(children, "4"), binary-edges(children), highlight: ("4",))),
  [Search tree: smaller keys to the left, larger to the right],
)

// current-theme(): the theme outside canvas(theme => ...), only inside context; here gray priorities.
#let prio = ("4": 9, "2": 7, "6": 5, "1": 3, "3": 2, "5": 4)
#context fig(
  canvas(graph(binary-layout(children, "4"), binary-edges(children), shape: "rect", size: (0.62, 0.52),
    labels: prio.keys().map(k => (k, text(size: 0.78em)[#k \ #text(size: 0.82em, fill: current-theme().color.muted)[#prio.at(k)]])).to-dict())),
  [Treap: a search tree by keys, a heap by priorities (gray)],
)

= Algorithm as frames

// frames(k => figure, k: (values: ...) or (from:, to:, step:, value:)): a slider and playback in the app.
// label: frame caption; pdf: which frames (numbers from 1) the PDF shows in a row.
#let passes = ((5, 2, 4, 1), (2, 4, 1, 5), (2, 1, 4, 5), (1, 2, 4, 5))
#fig(
  frames(k => canvas(array-cells(passes.at(k),
      highlight: range(4 - k, 4).map(i => (str(i), "third")).to-dict())),
    k: (values: (0, 1, 2, 3)),
    label: k => if k == 0 [initial array] else [pass #k],
    pdf: (1, 2, 3, 4)),
  [Bubble sort: after pass $k$ the last $k$ elements are in place],
)

// Trace: data-table with the step row highlighted.
Two pointers look for a pair with sum $7$ in $(1, 2, 4, 5)$:

#data-table(
  (auto, auto, auto, 1fr),
  header: ([step], [$l$], [$r$], [action]),
  highlight: ("2,*": "third"),
  [1], [0], [3], [$1 + 5 = 6 < 7$, move $l$ right],
  [2], [1], [3], [$2 + 5 = 7$, found],
)
