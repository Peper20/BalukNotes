#import "/_baluk/lib.typ": *
#show: book.with(
  lang: "en",
  title: [Notes in English],
  description: [Decoration words follow the language of the note.],
  tags: ("фикстура", "языки"),
)

= Blocks

#plan([define a limit], [prove a theorem], [solve an example])

#definition(title: [limit])[A sequence $x_n$ converges to $a$ if $|x_n - a| -> 0$.]

#theorem[Every convergent sequence is bounded.]

#proof[Take $epsilon = 1$.]

#example(title: [squeeze])[
  #step[Estimate] $-1/n <= (sin n)/n <= 1/n$.
  #answer[$0$]
]

#remark[A remark.] #pitfall[A pitfall.] #idea[An idea.] #algorithm[An algorithm.]

#pitfalls(([dividing by zero], [check the denominator]))

#summary([limits], [bounds])

#quiz(([Is $1/n$ bounded?], [Yes.]))

= Code and figures

#listing(complexity: [$O(n)$], ```python
print(sum(range(10)))
```)

#fig(canvas(theme => {
  import cetz.draw: *
  circle((0, 0), radius: 0.5)
}), [A circle])

#fig(frames(k => canvas(theme => {
  import cetz.draw: *
  circle((0, 0), radius: k.r)
}), k: ((r: 0.3), (r: 0.6))), [Frames without a numeric parameter])
