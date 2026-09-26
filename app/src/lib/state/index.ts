// Состояние клиента — модули-хранилища с узкими связями:
//
// - settings — настройки и темы;
// - notes — список заметок;
// - tabs, places — вкладки, места чтения и недавние (localStorage);
// - router — адрес страницы → что показать, переходы;
// - reader — показанная заметка: загрузка, статус, прокрутка;
// - updates — проверка изменений.
//
// Чистая логика — в ../tabs.ts, ../places.ts, ../reading.ts (с Vitest).
// Интерфейс вокруг заметки (панели, глава, оглавление) — ../ui.svelte.ts.

import { api, rebaseStylesheets } from "../api";
import { parseRoute } from "../ids";
import { notes } from "./notes.svelte";
import { places } from "./places.svelte";
import { reader } from "./reader.svelte";
import { router } from "./router.svelte";
import { settings } from "./settings.svelte";
import { tabs } from "./tabs.svelte";
import { updates } from "./updates.svelte";

export { notes, places, reader, router, settings, tabs, updates };

/** Запуск клиента: настройки и список заметок с сервера, маршрут, прогрев. */
export async function start(): Promise<void> {
  rebaseStylesheets();
  // Список заметок — после настроек: вид (кегль) меняет ширину названий
  // вкладок, а полоса вкладок прокручивается к активной по списку заметок.
  const [, list] = await Promise.all([settings.load(), api.notes()]);
  notes.all = list;
  updates.start();
  history.scrollRestoration = "manual";
  tabs.restore();
  router.sync();
  // Сервер собирает все заметки заранее — сначала те, что во вкладках и недавние.
  const tabIds = tabs.list.map((t) => parseRoute(new URL(t.url, location.href).pathname)).flatMap((r) => (r.kind === "note" ? [r.id] : []));
  void api.warm({ ids: [...new Set([...tabIds, ...places.recent])] }).catch(() => {});
  addEventListener("popstate", () => router.sync({ pop: true }));
  addEventListener("pagehide", () => reader.remember());
}
