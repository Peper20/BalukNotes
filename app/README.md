# app - клиент

Svelte 5 (руны) + TypeScript + Vite, без SvelteKit. Как клиент связан с ядром -
`docs/architecture.md`.

```sh
npm run dev     # горячая замена: :5173, API - прокси на :8432 (tools/test-env.sh)
npm run build   # app/dist: отладочный notes serve берёт его с диска - достаточно перезагрузить страницу
npm run types   # типы API из Rust
npm test        # Vitest; ещё check, e2e
```

- Состояние - модули `src/lib/state/` (запуск - `start()` в `index.ts`) и
  `ui.svelte.ts` (панели, глава, оглавление); новое состояние - свой модуль, не
  поле в чужом. Чистая логика - `src/lib/*.ts` с Vitest рядом (`*.test.ts`);
  компоненты - `src/components/`.
- Сервер - только через `src/lib/api/` (адрес и токен - `api/config.ts`, ошибки
  - `ApiError`), без `fetch` в компонентах. Источник изменений - `changes.ts`
  (события `GET .../events`, опроса нет); связь с сервером -
  `state/connection.svelte.ts` (ответил ли он - `api.onReach`).
- **Типы API - из Rust** (`ts-rs`, фича `ts`): `npm run types` выгружает их в
  `src/lib/api/types/` (в git). Поменял структуру ответа - выгрузи и закоммить;
  руками не править.
- Хранилище - в адресе (`/v/<имя>/...`, `lib/vault.ts`), выбирается до загрузки
  состояния (`lib/boot.ts`). Адреса - только через `lib/ids.ts` (`noteHref`,
  `homeHref`...), не строкой `"/"`/`"/n/..."`; `localStorage` - через
  `lib/storage.ts` (ключи свои у каждого хранилища).
- Читателю - название, а не имя файла: `notes.title(id)`,
  `notes.folderTitle(path)` (`lib/state/notes.svelte.ts`).
- HTML заметки вставляется в DOM напрямую (`NoteView.svelte`), не шаблоном.
- Настройка вида = запись в `notes-core::settings::Schema` (`.attr("data-...")`
  или `.var("--...", "px")`) + правило CSS; `appearance.ts` применяет её по
  схеме.
- Стили заметки - только внутри `.k-note { ... }`: блоки `src/baluk-css/`
  (порядок - `@import` в `baluk.css`), склеиваются в `assets/baluk.css`.
  Интерфейс - `src/app.css`.
- Живые блоки (интерактивные рисунки, кадры, граф) - реестр `src/lib/live/`:
  модуль с `LiveBlock` + селектор в `selectors.ts` + строка в `BLOCKS`.
- Значки - `@lucide/svelte` (`import X from "@lucide/svelte/icons/x"`), не
  текстовые глифы. Панель кадров - "тихий" вид: контурные значки без фона и
  рамки (решение пользователя).
- Классы интерфейса - уникальные по смыслу: общий `.help` у подсказки настройки
  и диалога справки однажды растянул настройки за край экрана.
- Тема на `<html>` - до клиента (`public/assets/theme.js`, ключ `k-theme`);
  `settings.apply` не трогает `<html>`, пока настройки не пришли: иначе
  первый кадр - белый.
- Другая заметка: прежняя стоит на экране, пока грузится новая, но не дольше
  `STALE_MS` (`reader.svelte.ts`) - из кэша переход без пустого кадра.
- `<html data-state="loading|ready">`: инструменты и e2e ждут `ready` - новый
  экран или состояние соблюдает то же.
- Граф рисует готовую раскладку ядра (`POST /api/graph/layout`): вид -
  `graph-view.ts`, жесты - `graph-gesture.ts`, переезды - `graph-motion.ts`,
  физика - `graph-physics.ts` (раскладка ядра - покой). `RETURN = 0.25` и
  `DAMPING = 0.3` подобрал пользователь (соседи возвращаются на четверть пути):
  не менять без него.
- e2e - `e2e/*.spec.ts`: Playwright на системном chromium, сервер на копии
  `tests/vault` в `tests/.data/e2e`, прогретый заранее. Новая возможность
  интерфейса - новый сценарий; телефон - `mobile.spec.ts` (400x800: страница не
  шире окна, замер после `document.fonts.ready`).
