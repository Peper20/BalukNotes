# fonts — шрифты оформления, встроенные в приложение

Шрифты konspekt встроены в бинарник (`notes-core::fonts`) и выбираются
раньше системных: отрисовка одинакова на любой машине (эталонные снимки,
десктоп, Android), и шрифты не нужно ставить в систему.

| Файлы | Гарнитура | Версия | Правообладатель |
|---|---|---|---|
| `GentiumPlus-*.ttf` | Gentium Plus — текст, заголовки | 6.200 | © 2003–2023 SIL International, Reserved Font Names «Gentium» и «SIL» |
| `JetBrainsMono-*.ttf` | JetBrains Mono — код | 2.304 | © 2020 The JetBrains Mono Project Authors |

Обе — под [SIL Open Font License 1.1](OFL.txt): встраивать и распространять
вместе с программой можно, продавать сами шрифты отдельно — нельзя.
Формулы — New Computer Modern Math, он встроен в Typst.

Начертания — только нужные библиотеке: обычное, курсив, жирный, жирный курсив.
Обновление шрифта = замена файла + `UPDATE_SNAPSHOTS=1 cargo test -p notes-core
--test snapshots` (глифы в рисунках изменятся) + проверка глазами.
