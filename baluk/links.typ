// Links between vault notes.
//
//   #see("Network/SSH")                       -> the note title: "SSH basics"
//   #see("Network/SSH", anchor: "Tunnels")    -> "Tunnels"
//   #see("Network/SSH", anchor: "Tunnels")[about tunnels]
//
// The path is from the vault root, without `.typ`; a book is the path to its
// folder. The anchor is a heading text (as in Obsidian: "Changing the port")
// or a label name. The target title (`title:` of its template, else the file
// name) comes from the app as the file `/_vault/title/<path>`: rename a note
// and link texts follow.
//
// HTML gets <a class="k-link" data-k-target data-k-anchor>: the app sets the
// address, checks that the note and heading exist (`notes check`) and builds
// backlinks and the graph from these links. In PDF a link is plain text in
// the accent color: there are no other notes in a PDF.

#import "theme.typ": current-theme
#import "web.typ": is-web, elem

#let see(id, anchor: none, ..body-args) = {
  assert(type(id) == str, message: "see: the path is a string, e.g. \"Network/SSH\"")
  assert(not id.ends-with(".typ"), message: "see: a path without .typ: \"" + id.trim(".typ", at: end) + "\"")
  assert(anchor == none or type(anchor) == str, message: "see: anchor is a string (a heading text or a label)")
  let display-text = if body-args.pos().len() > 0 { body-args.pos().at(0) } else if anchor != none { anchor } else {
    read("/_vault/title/" + id)
  }
  context if is-web() {
    let attrs = ("data-k-target": id)
    if anchor != none { attrs.insert("data-k-anchor", anchor) }
    html.elem("a", attrs: (class: "k-link") + attrs, display-text)
  } else {
    text(fill: current-theme().color.accent, display-text)
  }
}
