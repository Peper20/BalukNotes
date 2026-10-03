# tools - проверка, снимки, установка

| Файл | Что |
|---|---|
| `check.sh` | полная проверка |
| `install.sh` | `notes` в `~/.local/bin` и навык в `~/.claude/skills` одной версией, служба `notes service` - перезапуск; после изменений - снова |
| `test-env.sh` | сервер на `tests/vault` (данные `tests/.data`, порт 8432) |
| `visual.mjs`, `shot.mjs` | снимки фикстур и страниц (браузер - `lib/browser.mjs`) |

## Проверка

Тесты не трогают данные пользователя: хранилище-фикстура `tests/vault/`
(каталог случаев - его README), данные тестов - `tests/.data/` (не в git,
можно удалять).

```sh
tools/check.sh          # полная, перед коммитом и PR (~4 мин)
tools/check.sh --fast   # без сборки клиента и e2e; ещё --rust, --app
```

`check.sh` и `install.sh` идут с приоритетом ниже обычного
(`nice` 2 и 1, `ionice` 5): компьютер не подлагивает. Шаги: cargo test, clippy (`-D warnings`), fmt; `notes check` на `tests/vault`
(итог сверяется со строкой "Итог" в его README); заготовки `notes new` и
навык (`skills/README.md`); выгруженные типы API совпадают с закоммиченными
(незакоммиченные - тоже "упал"); клиент - check, Vitest, build, e2e. В конце -
таблица шагов со временем, код возврата ненулевой при любой ошибке, логи -
`tests/.data/check/`. Для e2e нужен системный Chromium (`/usr/bin/chromium`,
другой - `CHROMIUM=...`). Перед шагами: `target/` больше 30 ГБ
(`TARGET_LIMIT_GB`) - `cargo clean`, cargo сам старые сборки не удаляет; при
своём `CARGO_TARGET_DIR` (общий для worktree) - не чистит.

**Эталонные снимки** `tests/snapshots/`: HTML каждой фикстуры (`*.snap`),
публичные имена библиотеки, эталон поиска. Тест упал - прочитать
`git diff tests/snapshots`; изменение задумано -
`UPDATE_SNAPSHOTS=1 cargo test -p notes-core`.

## Посмотреть глазами

После правок библиотеки, CSS или клиента - обе темы и узкий экран. Снимки - в
scratchpad или `tests/.data/`, не в репозиторий; PNG открывать Read.

```sh
tools/test-env.sh [--fresh] &          # сервер на tests/vault, порт 8432
tools/visual.mjs [--only Книга]         # фикстуры x (светлая, тёмная, узкий) -> tests/.data/visual/index.html
tools/shot.mjs "http://127.0.0.1:8432/n/демо/компоненты" out.png [--dark] [--size 400x800] [--full]
tools/shot.mjs URL out.png --print 'scrollY'     # замер в странице
```

- `chromium --screenshot` не использовать: он рисует страницу с нуля без
  прокрутки - якоря и липкие панели на снимке ломаются. `shot.mjs` не
  эмулирует телефон (с эмуляцией замеры врут): узкий экран - шириной.
- Отладочная сборка берёт `baluk/` и `app/dist` с диска; правки Rust - после
  перезапуска `notes serve`.
- Остановить сервер - `kill $(pgrep -x notes)`, не `pkill -f ...`, и не ждать
  процесс через `pgrep -f ...`: шаблон совпадает с командной строкой самой
  оболочки (убивает её или ждёт вечно).

## Облачное окружение

- Chromium - обёртка `/usr/local/bin/chromium` с `--no-sandbox` (ставит setup
  script окружения); шрифты встроены; `packages.typst.org` разрешён (CeTZ).
  Хранилища пользователя нет - работать на `tests/vault`.
- Диск мал: собирать с `CARGO_PROFILE_DEV_DEBUG=line-tables-only` (полная
  отладочная информация - десятки ГБ); кончилось место - `cargo clean`.
- Время: сборка Rust с нуля ~8 мин, `cargo test` ~1 мин, e2e ~1,5 мин.
- Не для облака (нужны пользователь или его машина): приложения Tauri и
  Android, авторизация, замена файлов шрифтов, обновление Typst.
