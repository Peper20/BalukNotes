# baluk notes

Заметки на Typst с красотой конспектов konspekt-style и удобством Obsidian:
ссылки, граф, быстрый переход, поиск — и интерактив, которого нет в PDF.
Браузер, десктоп и Android из одной кодовой базы (Rust + Tauri 2).

Сейчас — веха M1: ядро на Rust компилирует хранилище `.typ` в HTML с двумя
темами, локальный сервер показывает заметки с оглавлением и обратными
ссылками и обновляет их по запросу.

## Запуск

Нужны Rust ≥ 1.92 и пакет `@preview/cetz:0.4.2` (скачается сам, если нет в кэше).

```sh
cargo run -p notes-cli -- serve             # http://127.0.0.1:8421, хранилище data/vault
cargo run -p notes-cli -- check             # ошибки компиляции и битые ссылки
cargo run -p notes-cli -- build site/       # статический сайт
cargo run -p notes-cli -- pdf Конспекты/Матан --theme ночь   # PDF заметки или книги
cargo run -p notes-cli -- --vault examples/vault serve   # тестовое хранилище
```

Заметки — файлы `.typ` в `data/vault/` (каталог в git не входит). Пишет их
кто угодно (обычно Claude Code), клиент подхватывает изменения по кнопке ⟳
или раз в несколько секунд (настройка).

## Устройство

| Где | Что |
|---|---|
| `docs/architecture.md` | решения: стек, хранилище, отрисовка, темы, настройки, обновление |
| `docs/roadmap.md` | вехи и что сделано |
| `docs/tech-debt.md` | что отложено и почему |
| `crates/notes-core/` | ядро: хранилище, компилятор Typst, склейка тем, рисунки, индекс ссылок, настройки, проверка |
| `crates/notes-server/` | HTTP API и раздача клиента (axum) |
| `crates/notes-cli/` | команда `notes`: serve, check, build, pdf |
| `web/` | клиент: `app.js` + `app.css` (интерфейс), `konspekt.css` (вид заметок) |
| `konspekt/` | библиотека оформления: форк konspekt-style + HTML-ветка, `заметка`, `см` |
| `examples/vault/` | тестовое хранилище (фикстуры тестов и проверки глазами) |
| `tools/shot.mjs` | снимок страницы headless Chromium — проверять вид глазами |

## Проверка

```sh
cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --check
```
