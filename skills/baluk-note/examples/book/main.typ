// Skill sample: BOOK, a folder with main.typ and chapters NN-topic.typ.
// `notes new --book` creates main.typ; chapters are plain files next to it,
// each included by an #include line.
#import "/_baluk/lib.typ": *
#show: book.with(
  lang: "en",
  kind: [Course notes],             // label above the title: [Problem book], [Cheat sheet]...
  title: [Prefix sums],
  subtitle: [Range sum in $O(1)$],
  author: [Algorithms, term 1],
  date: [2026],
  description: [For readers who know loops and arrays. Read the chapters in order.],
  tags: ("example", "algorithms"),
)

#include "01-idea.typ"
#include "02-code.typ"
