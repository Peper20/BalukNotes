// A book chapter starts with the import; "=" is a chapter ("Chapter 1"), "==" a section.
#import "/_baluk/lib.typ": *

= Idea
#lead[
  A range sum of an array takes one subtraction if all prefix sums are
  computed in advance. We rely only on arrays.
]
#plan([build the prefix sum array], [answer a range sum query in $O(1)$])

== Prefixes <sec-prefix>

#definition(title: "prefix sum")[
  $P_i = a_0 + a_1 + dots + a_(i - 1)$, $P_0 = 0$.
]

#formula[$ sum_(i = l)^(r) a_i = P_(r + 1) - P_l $]

#example(title: "range sum")[
  $a = (3, 1, 4, 1, 5)$, find the sum on $[1, 3]$.

  #step[Prefixes] $P = (0, 3, 4, 8, 9, 14)$.

  #step[Difference] $P_4 - P_1 = 9 - 3 = 6$.

  #answer[$6$]
]

#pitfall[Off by one: taking $P_r - P_l$ loses $a_r$.]

#summary([Prefixes take one pass.], [A query is one subtraction.])
#quiz(([What is $P_0$?], [$0$]))
