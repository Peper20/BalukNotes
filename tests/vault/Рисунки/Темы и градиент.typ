#import "/_baluk/lib.typ": *
#show: note.with(title: [Темы и градиент], tags: ("рисунки", "фикстура"))

Рисунки, которые нельзя склеить в один SVG по цветам, и заливка градиентом.

= Форма зависит от темы

В светлой теме — квадрат, в тёмной — круг: темы различаются не только
цветами, поэтому ядро оставляет по варианту на тему (запасной путь).

#fig(canvas(theme => {
  let is-dark = theme.color.bg.components().at(0) < 50%
  if is-dark {
    cetz.draw.circle((1, 1), radius: 1, fill: theme.color.fig.line)
  } else {
    cetz.draw.rect((0, 0), (2, 2), fill: theme.color.fig.line)
  }
}), [Разная форма в темах])

= Градиент

#fig(canvas(theme => {
  cetz.draw.rect((0, 0), (5, 1), fill: gradient.linear(theme.color.accent, theme.color.bg), stroke: none)
}), [Заливка градиентом от акцента к фону])

= Обычный рисунок

#fig(canvas(unit: 0.8cm, theme => {
  axes(x: (-0.3, 4), y: (-0.3, 2.5))
  plot(x => calc.sqrt(x), 0, 3.8, label: $sqrt(x)$)
}), [Темы различаются только цветами — один SVG])
