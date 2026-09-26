// Выгрузка названий, цветов и шрифтов всех тем для CSS: typst query css.typ "<k-css>".
// Не документ — служебный файл сборщика.
#import "theme.typ": themes, css-colors

// Основные шрифты темы (первый в каждом списке `font`) — их приложение
// отдаёт браузеру; запасные браузеру не нужны.
#let main-fonts(theme) = theme.font.values().map(f => if type(f) == array { f.first() } else { f })

#metadata(themes.pairs().map(((name, theme)) => (name, (
  title: theme.title,
  colors: css-colors(theme),
  fonts: main-fonts(theme),
))).to-dict()) <k-css>
