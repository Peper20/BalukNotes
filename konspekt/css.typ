// Выгрузка цветов всех тем для CSS: typst query css.typ "<k-css>".
// Не документ — служебный файл сборщика.
#import "theme.typ": темы, css-цвета
#metadata(темы.pairs().map(((имя, т)) => (имя, css-цвета(т))).to-dict()) <k-css>
