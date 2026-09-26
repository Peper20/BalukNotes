# baluk notes

Заметки на Typst с красотой печатных конспектов и удобством Obsidian:
ссылки, граф, быстрый переход, поиск — и интерактив, которого нет в PDF.
Браузер, десктоп и Android из одной кодовой базы (Rust + Tauri 2).

Сейчас сделаны M0, M1 и M3: ядро на Rust компилирует хранилище `.typ` в HTML
с двумя темами и в PDF; локальный сервер и клиент в браузере — дерево,
вкладки, поиск, оглавление, обратные ссылки, граф заметок, интерактивные
рисунки и кадры; статический сайт. Дальше — оболочка Tauri (M2) и Android (M4).

## Запуск

Нужны Rust ≥ 1.92, Node.js ≥ 22 и пакет `@preview/cetz:0.4.2` (скачается
сам, если нет в кэше). Клиент собирается один раз (и после его правок):

```sh
npm --prefix app ci && npm --prefix app run build   # клиент → app/dist
cargo run -p notes-cli -- serve             # http://127.0.0.1:8421, хранилище data/vault
cargo run -p notes-cli -- check             # ошибки компиляции и битые ссылки
cargo run -p notes-cli -- build site/       # статический сайт
cargo run -p notes-cli -- pdf Конспекты/Матан --theme night   # PDF заметки или книги
tools/test-env.sh                                     # тестовое окружение (tests/vault, :8432)
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
| `app/` | клиент: Svelte 5 + TypeScript + Vite; `public/assets/baluk.css` — вид заметок |
| `baluk/` | библиотека оформления Typst: блоки, рисунки, шаблоны `note`/`book`, ссылки `see` — HTML- и PDF-ветка |
| `tests/vault/` | хранилище-фикстура: каждый случай отрисовки, каталог — его `README.md` |
| `tests/snapshots/` | эталонные снимки HTML фикстур (`UPDATE_SNAPSHOTS=1` — обновить) |
| `tools/check.sh` | полная проверка одной командой: Rust, `notes check` на фикстуре, типы API, клиент, e2e |
| `tools/test-env.sh` | тестовое окружение: сервер на `tests/vault`, данные в `tests/.data/` |
| `tools/visual.mjs` | альбом снимков всех фикстур: светлая, тёмная, узкий экран |
| `tools/shot.mjs` | снимок одной страницы headless Chromium |

## Проверка

```sh
tools/check.sh          # всё: cargo test/clippy/fmt, notes check, типы API, клиент, e2e
tools/check.sh --fast   # без сборки клиента и e2e; ещё есть --rust и --app
```

В конце — таблица «шаг → ок/упал, время», код возврата ненулевой при любой
ошибке; логи шагов — `tests/.data/check/`.
