// Ссылки между заметками хранилища.
//
//   #see("Сеть/SSH")                       → название заметки: «SSH: основы»
//   #see("Сеть/SSH", anchor: "Туннели")      → «Туннели»
//   #see("Сеть/SSH", anchor: "Туннели")[про туннели]
//
// Путь — от корня хранилища, без `.typ`; книга — путь к её папке. Якорь —
// текст заголовка (как в Obsidian: «Смена порта») или имя метки. Название
// цели (`title:` её шаблона, иначе имя файла) даёт приложение — файл
// `/_vault/title/<путь>`: переименовали заметку — подписи ссылок следом.
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
