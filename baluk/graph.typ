// The vault graph in a note: notes are nodes, #see links are edges.
//
//   #vault-graph()                                   // the whole vault
//   #vault-graph(around: "Network/SSH", depth: 2)    // neighbors of a note
//   #vault-graph(folders: ("Math",), missing: false)
//   #vault-graph(tag: "linalg", orphans: false)
//
// The app computes the filter and layout (`notes-core::vault_graph`) - the
// same as for the graph page: the note reads the ready graph from the virtual
// file `/_vault/graph/<filter>.json`. CeTZ draws it: in PDF and in HTML
// without scripts the graph is a picture; the app brings it to life at the
// same coordinates (hover, dragging within the figure frame, going to a note
// on click).
//
// A note with a graph depends on the whole vault: it is rebuilt when any
// note changes.

#import "@preview/cetz:0.4.2"
#import "theme.typ": current-theme
#import "web.typ": is-web, elem, frame

// The label font size and its offset from the circle - in layout units (as in the core).
#let _label-size = 11
#let _label-gap = 2

// The path in a filter is a JSON string; `/` in a file name is escaped (the core parses it back).
#let _escape(s) = s.replace("%", "%25").replace("/", "%2F").replace("\\", "%5C")

// Group colors (top-level folders) - the same as the app's graph.
#let _group-colors(theme) = {
  let (c, b) = (theme.color, theme.color.boxes)
  (c.accent, b.example, c.secondary, b.idea, b.algorithm, b.pitfall, b.theorem)
}

/// The vault graph, whole or filtered.
/// - around, depth: only the neighbors of a note within depth steps (the note itself is in the center);
/// - folders: only these top-level folders ("в корне" - notes in the root);
/// - hidden: hide these folders;
/// - tag: only notes with the tag;
/// - missing: show unwritten notes (linked to but absent);
/// - orphans: show notes without links;
/// - width: the largest figure width.
#let vault-graph(
  around: none, depth: 1, folders: (), hidden: (), tag: none,
  missing: true, orphans: true, width: 15cm,
) = {
  assert(around == none or type(around) == str, message: "vault-graph: around is a note path, e.g. \"Network/SSH\"")
  assert(type(depth) == int and depth >= 1, message: "vault-graph: depth is an integer, at least 1")
  assert(type(folders) == array and type(hidden) == array, message: "vault-graph: folders and hidden are arrays of strings, e.g. (\"Network\",)")
  assert(tag == none or type(tag) == str, message: "vault-graph: tag is a string")
  let filter = (
    folders: folders, hidden: hidden, tag: tag, missing: missing, orphans: orphans,
    around: around, depth: depth,
  )
  let path = "/_vault/graph/" + _escape(json.encode(filter, pretty: false)) + ".json"
  let data = json(path)
  let (x0, y0, x1, y1) = data.bounds
  // The layout unit: a label is ~ 8.8 pt; a large graph shrinks to width.
  let unit = calc.min(0.8pt, width / calc.max(x1 - x0, 1))

  let drawing = context {
    let theme = current-theme()
    let colors = _group-colors(theme)
    let color-of(group) = colors.at(calc.rem(calc.max(data.groups.position(g => g == group), 0), colors.len()))
    let at(n) = (n.x, -n.y)
    let by-id = (:)
    for n in data.nodes { by-id.insert(n.id, n) }
    cetz.canvas(length: unit, {
      import cetz.draw: *
      for e in data.edges {
        let (a, b) = (by-id.at(e.from), by-id.at(e.to))
        let w = calc.min(1 + e.count * 0.4, 3)
        line(at(a), at(b), stroke: (paint: theme.color.muted.transparentize(35%), thickness: w * 0.45pt))
      }
      for n in data.nodes {
        let col = color-of(n.group)
        let book = n.kind == "book"
        let center = n.id == data.center
        circle(at(n), radius: n.r,
          fill: if n.kind == none { none } else { col },
          stroke: if center { 1.6pt + theme.color.text } else if n.kind == none { (paint: col, thickness: 0.8pt, dash: "dashed") } else { 0.6pt + col })
        let size = _label-size * unit * if book { 1.1 } else { 1 }
        content((n.x, -(n.y + n.r + _label-gap)), anchor: "north", padding: 0pt,
          text(size: size, fill: if n.kind == none { theme.color.muted } else { theme.color.text },
            weight: if book or center { "bold" } else { "regular" }, n.name))
      }
    })
  }
  context if is-web() {
    // Data for the client: it brings the graph to life at the same coordinates.
    elem("div", "k-graph", ..("data-k-graph": read(path)), frame(drawing))
  } else {
    align(center, drawing)
  }
}
