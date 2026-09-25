# baluk notes

Заметки на Typst с красотой конспектов konspekt-style и удобством Obsidian:
ссылки, граф, быстрый переход, поиск — и интерактив, которого нет в PDF.
Браузер, десктоп и Android из одной кодовой базы (Rust + Tauri 2).

Сейчас — веха M0: библиотека оформления `konspekt/` умеет HTML, а прототип
сборщика превращает хранилище в статический сайт с двумя темами.

```sh
tools/build.py examples/vault build     # нужен typst 0.15.1 и пакет cetz 0.4.2 в кэше
xdg-open build/index.html
```

| Где | Что |
|---|---|
| `docs/architecture.md` | решения: стек, хранилище, отрисовка, темы, интерактив, синхронизация |
| `docs/roadmap.md` | вехи и что сделано |
| `konspekt/` | библиотека оформления (форк `lib/` из konspekt-style + HTML-ветка) |
| `web/konspekt.css` | вид HTML-заметок; цвета — переменные из тем `konspekt/theme.typ` |
| `tools/build.py` | прототип сборщика, будет заменён `notes-core` на Rust |
| `examples/vault/` | тестовое хранилище: витрина блоков и рисунков, глава «Ассессмента» |
