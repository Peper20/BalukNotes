// Выгрузка для приложения: названия, цвета и шрифты всех тем (для CSS) —
// typst query css.typ "<k-css>"; языки словарей оформления — "<k-langs>"
// (`notes check` предупреждает о `lang:` без словаря).
// Не документ — служебный файл сборщика.
#import "theme.typ": themes, css-colors
#import "i18n.typ": words

// Шрифты темы — основные и запасные (все списки `font`): приложение отдаёт
// их браузеру, и знак, которого нет в основном шрифте, выглядит как в PDF.
// Шрифты формул (`font.math`) — отдельно: их приложение не перестраивает.
#let _list(f) = if type(f) == array { f } else { (f,) }
#let web-fonts(theme) = theme.font.values().map(_list).flatten()

#metadata(themes.pairs().map(((name, theme)) => (name, (
  title: theme.title,
  colors: css-colors(theme),
  fonts: web-fonts(theme),
  math-fonts: _list(theme.font.math),
))).to-dict()) <k-css>

#metadata(words.keys()) <k-langs>
