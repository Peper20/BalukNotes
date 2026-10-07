// Export for the app: titles, colors and fonts of all themes (for CSS) -
// typst query css.typ "<k-css>"; languages of the word dictionaries -
// "<k-langs>" (`notes check` warns about a `lang:` without a dictionary).
// Not a document - an internal file of the build.
#import "theme.typ": themes, css-colors
#import "i18n.typ": words

// Theme fonts - main and fallback (all `font` lists): the app hands them to
// the browser, so a glyph missing from the main font looks as in the PDF.
// Math fonts (`font.math`) are separate: the app does not rearrange them.
#let _list(f) = if type(f) == array { f } else { (f,) }
#let web-fonts(theme) = theme.font.values().map(_list).flatten()

#metadata(themes.pairs().map(((name, theme)) => (name, (
  title: theme.title,
  colors: css-colors(theme),
  fonts: web-fonts(theme),
  math-fonts: _list(theme.font.math),
))).to-dict()) <k-css>

#metadata(words.keys()) <k-langs>
