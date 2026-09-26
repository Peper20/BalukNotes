#import "/_baluk/lib.typ": *
#show: note.with(title: [Граф хранилища], tags: ("рисунки", "фикстура"))

Граф заметок прямо в заметке: фильтр и раскладка — из приложения, рисует
CeTZ; в HTML клиент оживляет его по тем же координатам.

= Соседи заметки

#vault-graph(around: "Сеть/SSH", depth: 1)

= Папка без ненаписанных

#vault-graph(folders: ("Сеть",), missing: false, width: 8cm)
