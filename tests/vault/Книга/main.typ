#import "/_baluk/lib.typ": *
#show: book.with(
  title: [Тестовая книга],
  subtitle: [Фикстура: книга из нескольких глав],
  author: [baluk notes · тесты],
  description: [Главы, одинаковые заголовки в разных главах, код из файла, картинка.],
  tags: ("книга", "фикстура"),
  title-page: true,
  toc: true,
  depth: 2,
)

#include "01-основы.typ"
#include "02-продолжение.typ"
#include "03-приложение.typ"
