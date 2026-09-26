// Выгрузка для приложения: названия, цвета и шрифты всех тем (для CSS) —
// typst query css.typ "<k-css>"; языки словарей оформления — "<k-langs>"
// (`notes check` предупреждает о `lang:` без словаря).
// Не документ — служебный файл сборщика.
#import "theme.typ": themes, css-colors
#import "i18n.typ": words

// Основные шрифты темы (первый в каждом списке `font`) — их приложение
// отдаёт браузеру; запасные браузеру не нужны.
#let main-fonts(theme) = theme.font.values().map(f => if type(f) == array { f.first() } else { f })

#metadata(themes.pairs().map(((name, theme)) => (name, (
  title: theme.title,
  colors: css-colors(theme),
  fonts: main-fonts(theme),
))).to-dict()) <k-css>

#metadata(words.keys()) <k-langs>
