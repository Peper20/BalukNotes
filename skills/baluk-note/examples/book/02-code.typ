#import "/_baluk/lib.typ": *

= Code
#lead[Implementation: build in $O(n)$, query in $O(1)$ (idea in section @sec-prefix).]

== Listing

// listing: code in ```lang ... ```; highlight: lines; callouts: markers at lines; complexity: a badge.
#listing(
  caption: [building prefixes],
  highlight: (4,),
  callouts: ("4": 1),
  complexity: [$O(n)$],
  ```python
  def prefix(a):
      p = [0] * (len(a) + 1)
      for i, x in enumerate(a):
          p[i + 1] = p[i] + x
      return p
  ```,
)

Line #callout(1): each prefix is one element longer than the previous.

== Code from a file

// code-from-file: path FROM THE VAULT ROOT, starts with "/"; region: lines between
// "// region: name" and "// endregion: name" in the file (language from the extension).
#code-from-file("/examples/book/code/prefix.cpp", region: "query", caption: [query], complexity: [$O(1)$])

== Vault graph

// Graph of notes inside the text: notes tagged "example" and links between them.
// Other filters: around: "Folder/Note", depth: 2 for neighbors; folders: ("Folder",).
#vault-graph(tag: "example", width: 8cm)
