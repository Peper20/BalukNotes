// ═════════════════════════════════════════════════════════════════════════
//  baluk - the BalukNotes Typst library for notes and books.
//
//  The app's vault sees it as /_baluk/ (virtual). Usage:
//
//    #import "/_baluk/lib.typ": *
//    #show: note.with(title: [...])      // a note
//    #show: book.with(title: [...])     // a book (large notes), root - main.typ
//    #show: chapter.with(title: [...])  // a book chapter
//
//  One external dependency: @preview/cetz:0.4.2 (cached, works offline).
//  Content rules - the project's docs/writing.md, API - README.md.
// ═════════════════════════════════════════════════════════════════════════

#import "@preview/cetz:0.4.2"
#import "theme.typ": themes, customize, current-theme, pale
#import "template.typ": book, chapter, note
#import "links.typ": see
#import "blocks.typ": small-caps, data-table, definition, theorem, example, remark, pitfall, idea, algorithm, step, answer, proof, formula, margin-note, side-by-side, lead, plan, summary, quiz, pitfalls
#import "code.typ": listing, code-from-file, callout
#import "figures.typ": canvas, fig, in-row, axes, tick, plot, parametric, fill-between, spoke, point, contours, p3, axes3d, surface, prisms, base-shape, revolution, cross-section, array-cells, matrix-cells, graph, tree-layout, binary-layout, binary-edges, circle-layout
#import "plots.typ": interactive-plot, interactive-surface
#import "frames.typ": frames
#import "graph.typ": vault-graph
#import "charts.typ": chart
#import "math.typ": *
