// Skill sample: TEXT BLOCKS, a whole note from template to quiz.
// Start from these calls; the comments say what each block is for.
#import "/_baluk/lib.typ": *
#show: note.with(
  lang: "en",                                          // block words in English; omit for Russian
  words: (example: "Worked example"),                  // own words over the dictionary
  title: [Blocks],
  description: [All text blocks on one topic],         // line in the note list
  tags: ("example", "math"),
)

// Lead: 2-4 sentences before the first section.
#lead[
  The limit of a sequence underlies analysis: derivatives, integrals and
  series sums are defined through it. We rely only on the absolute value.
]

// Plan: what the reader will learn (usually at the start of a book chapter).
#plan([read the definition], [prove convergence by squeezing], [keep the order of $epsilon$ and $N$])

// Section "=", subsection "==". A label <...> after a heading is for @... links.
= Definition <sec-def>

The numbers $x_n$ get closer to $a$; how to say it strictly? A new term in
*bold*, explained right away.

#definition(title: "limit of a sequence")[
  A number $a$ is the *limit* of $x_n$ if for every $epsilon > 0$ there is
  $N$ such that $|x_n - a| < epsilon$ for all $n >= N$.
]

// Key formula of a section, in a box; one or two per section.
#formula[$ lim_(n -> oo) x_n = a $]

// Callouts: at most one per half page.
#idea[The opponent names the precision $epsilon$ first, then we answer with $N$.]

#remark[Russian decimal comma only via `dc`: $dc("0,5") + dc("0,25") = dc("0,75")$.]

= Squeeze theorem <sec-squeeze>

#theorem(title: "squeeze")[
  If $a_n <= x_n <= b_n$ and $a_n -> a$, $b_n -> a$, then $x_n -> a$.
]

#proof[
  For $epsilon > 0$, eventually $a - epsilon < a_n <= x_n <= b_n < a + epsilon$.
]

// Worked example: #step is "Step 1.", #answer is the boxed answer.
#example(title: "squeeze with sine")[
  Find $lim_(n -> oo) (sin n) / n$.

  #step[Bound] $-1 / n <= (sin n) / n <= 1 / n$ since $|sin n| <= 1$.

  #step[Limits of the bounds] Both tend to zero.

  #answer[$0$]
]

// The main trap, right after the example.
#pitfall[Choosing $N$ before $epsilon$: then every bounded sequence has a "limit".]

#algorithm(title: "proving convergence")[
  + Write $|x_n - a|$.
  + Bound it by an expression that clearly tends to zero.
  + Find $N$ from $epsilon$.
]

// Own section: @label.
The definition is in section @sec-def, the squeeze in section @sec-squeeze.

// Another note: only #see, path from the vault root without .typ.
// ";" right after a call is eaten by Typst, hence \;
Figures: #see("examples/figures")\; a section of another note:
#see("examples/figures", anchor: "Function graphs")[function graphs].

= Text beside a table

#margin-note[A side note: useful, but it would break the story.]
A paragraph next to the side note. Inline code `a[i] += 1`; small caps
#small-caps[gost].

// Text and a small figure or table side by side (width is the share of the second).
#side-by-side(
  [The first terms of $x_n = 1 / n$ drop fast: at $n = 4$ the distance to
   zero is below $0.3$. The table is a trace.],
  data-table(
    (auto, auto),
    header: ([$n$], [$x_n$]),
    highlight: ("4,*": "second"),        // row 4; "*,2" column 2; "2,2" one cell
    [1], [$1$], [2], [$0.5$], [3], [$0.33$], [4], [$0.25$],
  ),
  width: 40%,
)

// Plain Typst table works too.
#table(
  columns: (auto, 1fr),
  table.header([Notation], [Meaning]),
  [$x_n -> a$], [converges to $a$],
  [$x_n -> oo$], [grows without bound],
)

= Math: Russian-style operators

// tg ctg arctg arcctg sh ch th cth rot grad const: operators; dd: differential; defeq: ":=".
$ tg x = (sin x) / (cos x), quad ctg x, quad arctg x + arcctg x = pi / 2 $
$ sh x, quad ch x, quad th x, quad cth x $
$ rot bold(F), quad grad f, quad C = const $
$ integral_0^1 x dd(x) = 1 / 2, quad f(x) defeq x^2 $

// At the end of a chapter (section): mistakes, summary, questions.
#pitfalls(
  ([Choosing $N$ before $epsilon$], [first "for every $epsilon$", then "there is $N$"]),
  ([Writing 0,5 in a Russian formula], [`dc("0,5")`]),
)

#summary(
  [A limit: for every precision there is an index.],
  [Squeeze: the unknown between two known ones with a common limit.],
)

#quiz(
  ([What is $lim (sin n) / n$?], [$0$]),
  ([What comes first, $epsilon$ or $N$?], [$epsilon$]),
)
