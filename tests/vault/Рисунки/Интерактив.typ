#import "/_baluk/lib.typ": *
#import "/_baluk/plots.typ": _formula, _eval
#show: note.with(title: [Интерактивные рисунки], tags: ("рисунки", "фикстура"))

График с ползунками и поверхность с вращением: в PDF и без JS — кадр при
значениях по умолчанию, в приложении — живой рисунок.

= График с параметрами

#fig(interactive-plot("a * calc.sin(b * x)", -5, 5,
  params: (a: (from: 0, to: 3, step: 0.1, value: 1), b: (from: 0.5, to: 4, step: 0.1, value: 1)),
  y: (-3, 3)), [Синусоида: амплитуда $a$ и частота $b$])

= Несколько кривых и разрывы

#fig(interactive-plot(
  ((f: "calc.tan(x)", label: "tg x"), (f: "1 / (x - c)", label: "1/(x−c)", dashed: true), "calc.sqrt(x)"),
  -4, 4, params: (c: (from: -2, to: 2, value: 1)), y: (-4, 4)),
  [Асимптоты рвут кривую, корень определён только при $x >= 0$])

= Поверхность

#fig(interactive-surface("calc.sin(k * x) * calc.cos(y)", (-3, 3), (-3, 3),
  params: (k: (from: 0.2, to: 2, step: 0.1, value: 1)), z: (-1, 1)),
  [Поверхность $z = sin k x cos y$])

= Сверка вычислений

Эти значения посчитал Typst; клиент (`app/src/lib/plot/`) сверяет с ними
свой разборщик (Vitest читает снимок этой заметки).

#let cross-check = (
  ("x * x - 3 * x + 2", (x: 1.5)),
  ("-x * -2", (x: 3)),
  ("-calc.pow(x, 2)", (x: 3)),
  ("2 - 3 - 4 + 1", (:)),
  ("12 / 3 / 2", (:)),
  ("1 / x", (x: 0)),
  ("calc.sqrt(x)", (x: -1)),
  ("calc.ln(x) + calc.log(x)", (x: 100)),
  ("calc.asin(x) + calc.acos(x) + calc.atan(x)", (x: 0.3)),
  ("calc.asin(x)", (x: 1.5)),
  ("calc.exp(x)", (x: 800)),
  ("calc.exp(-x) * calc.cos(a * x)", (x: 1.2, a: 2.5)),
  ("calc.pow(x, 1 / 3)", (x: -8)),
  ("calc.pow(x, 3)", (x: -2)),
  ("calc.pow(0, -1)", (:)),
  ("calc.floor(x) + calc.ceil(x) + calc.abs(-x)", (x: 2.5)),
  ("calc.pi * calc.e", (:)),
  ("1e3 + 2.5e-1", (:)),
  ("calc.tan(x) / calc.sin(x) * calc.cos(x)", (x: 0.7)),
  ("(x + y) * (x - y)", (x: 3, y: 2)),
)
#context if target() == "html" {
  let rows = cross-check.map(((ff, vars)) => {
    let names = ("x", "y", "a")
    let vars = vars.pairs().map(((kk, v)) => (kk, float(v))).to-dict()
    (f: ff, vars: vars, value: _eval(_formula(ff, names), vars))
  })
  html.elem("pre", attrs: (class: "k-plot-check"), json.encode(rows, pretty: false))
} else [Только в HTML.]
