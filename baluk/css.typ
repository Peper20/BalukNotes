// Выгрузка названий и цветов всех тем для CSS: typst query css.typ "<k-css>".
// Не документ — служебный файл сборщика.
#import "theme.typ": themes, css-colors
#metadata(themes.pairs().map(((name, theme)) => (name, (title: theme.title, colors: css-colors(theme)))).to-dict()) <k-css>
