// Ссылки между заметками хранилища.
//
//   #see("Сеть/SSH")                       → «SSH»
//   #see("Сеть/SSH", anchor: "Туннели")      → «Туннели»
//   #see("Сеть/SSH", anchor: "Туннели")[про туннели]
//
// Путь — от корня хранилища, без `.typ`; книга — путь к её папке. Якорь —
// текст заголовка (как в Obsidian: «Смена порта») или имя метки.
//
// В HTML получается <a class="k-link" data-k-target data-k-anchor>: адрес
// ставит приложение, оно же проверяет, что заметка и заголовок существуют
// (`notes check`), и строит по этим ссылкам обратные ссылки и граф. В PDF
// ссылка — просто текст цвета акцента: других заметок в PDF нет.

#import "theme.typ": current-theme
#import "web.typ": is-web, elem

#let see(id, anchor: none, ..body-args) = {
  assert(type(id) == str, message: "see: путь — строка, например \"Сеть/SSH\"")
  assert(not id.ends-with(".typ"), message: "see: путь без .typ: \"" + id.trim(".typ", at: end) + "\"")
  assert(anchor == none or type(anchor) == str, message: "see: anchor — строка (текст заголовка или метка)")
  let display-text = body-args.pos().at(0, default: if anchor != none { anchor } else { id.split("/").last() })
  context if is-web() {
    let attrs = ("data-k-target": id)
    if anchor != none { attrs.insert("data-k-anchor", anchor) }
    html.elem("a", attrs: (class: "k-link") + attrs, display-text)
  } else {
    text(fill: current-theme().color.accent, display-text)
  }
}
