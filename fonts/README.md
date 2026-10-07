# fonts - layout fonts

The layout fonts are embedded in the binary (`notes-core::fonts`) and win over
system fonts: rendering is the same on any machine (reference snapshots,
desktop, Android), and the fonts need not be installed in the system.

| Files | Typeface | Version | Copyright holder |
|---|---|---|---|
| `GentiumPlus-*.ttf` | Gentium Plus: text, headings | 6.200 | (c) 2003-2023 SIL International, Reserved Font Names "Gentium" and "SIL" |
| `JetBrainsMono-*.ttf` | JetBrains Mono: code | 2.304 | (c) 2020 The JetBrains Mono Project Authors |

Both are under the [SIL Open Font License 1.1](OFL.txt): they may be embedded
and distributed with the program, the fonts themselves may not be sold
separately. Formulas use New Computer Modern Math from Typst.

Styles: regular, italic, bold, bold italic. Updating a font - replace the file
+ `UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it snapshots::` (glyphs in
figures change) + a visual check.
