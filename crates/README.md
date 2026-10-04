# crates - ядро, сервер, CLI

Как устроено и почему - `docs/architecture.md`; здесь - правила кода.

| Крейт | Что |
|---|---|
| `notes-core` | ядро без async и HTTP: хранилище, компиляция Typst, обработка HTML, кэш, прогрев, ссылки, граф, поиск, настройки |
| `notes-server` | HTTP API (axum) и раздача клиента |
| `notes-assets` | клиент `app/dist` для сервера |
| `notes` | тонкая команда `notes`: вызывает части из своей папки, без зависимостей (architecture §1) |
| `notes-typst` | часть со сборкой Typst: все команды, кроме команд других частей |
| `notes-app` | окно (Tauri 2) без Typst: схема `notes://` -> ядро через сокет Unix; нужен WebKitGTK |

Логика - в ядре, остальные крейты - тонкие обёртки. Новая часть приложения -
бинарник `notes-<имя>` и строка в `notes::PARTS` (её команды); общие флаги
`notes-typst` со значением - `notes::VALUE_FLAGS` (сверяет тест).

Значок приложения - один источник `app/public/assets/icon.svg` (favicon сайта,
меню - `tools/install.sh`); окну Tauri нужен PNG, после правки SVG:
`rsvg-convert -w 256 -h 256 app/public/assets/icon.svg -o
crates/notes-app/icons/icon.png`.

Общие правила Rust - навык Claude Code `rust-best-practices`
(`.claude/skills/`, справочник `apollographql/skills`, MIT, с исправленными
примерами; навык общий, о проекте в нём ничего нет). Правила этого файла
важнее навыка. Без загруженного навыка (после сжатия контекста - загруженного заново)
правку `.rs` (Edit, Write и запись из Bash) запрещает хук `.claude/hooks/require-rust-skill.py`
(`.claude/settings.json`).

Где проект расходится с навыком:

- **Паника**: как в навыке - `unwrap`/`expect` вне тестов нет (линты
  `clippy::unwrap_used`/`expect_used`); `expect` на инвариант, который код
  гарантирует сам, - под `#[expect(clippy::expect_used, reason = "...")]`.
  В тестах можно: `#[test]` и `#[cfg(test)]` - `clippy.toml`, помощники
  сквозных тестов - `#![allow(...)]` на весь тестовый крейт (правило
  крейта, а не заглушка). Паника Typst при сборке - ошибка сборки заметки
  (`world.rs`, `panic_error`).
- **Производительность**: главы о ней - советы, правило - `CLAUDE.md`
  (мелочи не оптимизировать, слабые места - в `docs/tech-debt.md`). Замеры -
  фича `measure`, `RUST_LOG=notes_core=debug`, отчёты - `docs/research/`.
- **Тесты**: тест - одно поведение, но несколько случаев подряд (сборка
  Typst медленная, подготовка общая), имя короткое (`filters`); `rstest` и
  `insta` не подключены - снимки в `tests/snapshots/`.
- **Комментарии**: длинные обоснования - в `docs/architecture.md`, в коде -
  ссылка (`architecture §1`); `TODO` в коде нет - долг в `docs/tech-debt.md`;
  `missing_docs` выключен (крейты внутренние).
- **Язык**: проект переходит на английский (roadmap, "Приоритетное"); файл,
  переписанный по навыку, переводится целиком (комментарии, документация,
  сообщения) вместе с тестами текстов сообщений. Тексты интерфейса в ядре
  (подписи настроек в `settings.rs`, имя `new_note::UNTITLED`) - русские.
- **Потоки**: в ядре - `parking_lot`; у `std::sync::Mutex` отравление -
  явно (`unwrap_or_else(PoisonError::into_inner)`); в сервере блокирующее
  (Typst, диск) - в `spawn_blocking` (`notes-server/src/error.rs`).
- **Проверка**: `tools/check.sh`; clippy без `--all-features` (фичи `ts` и
  `measure` - для выгрузки типов и замеров, `notes-app` требует WebKitGTK).
  `rust-version = "1.92"` - новее в std не брать.

- **Сервер** - модуль на область API (`vaults`, `notes`, `graph`, `search`,
  `settings`, `assets`, `fonts`, `events`) со своими `routes()`; общее -
  `AppState` (`lib.rs`), `error.rs`, токен - `auth.rs`. Новая область - модуль
  + `merge`. API хранилища - под `/api/vaults/{vault}/...` (`s.vault(name)` -
  открытое хранилище); общее для всех (настройки, темы, шрифты) - без
  хранилища. Ответ - именованная структура (`notes-server/src/api.rs` или тип
  ядра), не `json!`: из неё выгружаются типы клиента (`app/README.md`).
- **Typst закреплён `=0.15.1`** (HTML-экспорт экспериментальный, API `html.*`
  меняется). Обновление - отдельной задачей: тесты, проверка глазами, копии
  крейтов в `vendor/` перенести или убрать (`vendor/README.md`).
- Ошибки ядра - `notes_core::Error` (thiserror); ошибки компиляции заметок -
  не ошибки, а `Diagnostic` в `NotePage`.
- Файлы хранилища - только через `trait Storage`. Слои ядра - architecture §7;
  тесты слоёв - без Typst (`pages::tests::Setup`: подделка сборки +
  `MemStorage`).
- Настройки производительности - настройки устройства (`device.*`,
  `SettingDef::device`, значения по умолчанию по `settings::Platform`); ядро
  применяет их на ходу (`Notes::apply_device`), не константами.
- **Кэш на диске** годен, пока не менялся код отрисовки: метка - хэш исходников
  ядра (`crates/notes-core/build.rs`). Модуль, работающий после кэша, можно
  вписать в `AFTER_CACHE` там же; влияющий на сырую отрисовку - никогда.
  Поменял `Record` или `Rendered` - подними `cache::FORMAT`.
- Версии и ключи кэша - только `version::StableHasher`, не `DefaultHasher`.
- Пути заметок - только через `NoteId` (проверяет `..`, служебные `_`/`.`).
- **Страница заметки** - цепочка проходов: `render.rs` + `passes/` (по дереву
  typst-html и по тексту) -> кэш -> `finish.rs` (под настройки: `figures.rs`,
  `frames.rs`). Новый проход - модуль и строка в списке (`passes::TREE`/`TEXT`,
  `finish::FINISH`; после кэша - ещё `AFTER_CACHE`). Глава книги - `book.rs`.
  Время проходов и вес - `RUST_LOG=notes_core=debug` ("проход", "рисунки").
- Данные хранилища для заметок `/_vault/<префикс>/...` - поставщик в реестре
  `vault_data.rs`.
- Шрифты оформления встроены (`fonts/README.md`); браузеру - шрифты тем
  (основные и запасные) частями WOFF2 (`webfonts.rs`). Правка кодирования частей или обновление
  `fontcull` - подними `webfonts::ENCODER`.
- Линты - `[workspace.lints]` (clippy pedantic); заглушить - точечно,
  `#[expect(..., reason = "...")]`.
- Тесты: модульные - рядом с кодом, сквозные - `crates/*/tests/it/` (один
  бинарник на крейт: общее ядро на фикстуру - `common.rs`; новый файл -
  модуль в `main.rs`, не отдельный бинарник; тяжёлые замеры - фича
  `measure`) на `tests/vault`.
