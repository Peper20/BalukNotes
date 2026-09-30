// Состояние клиента — модули-хранилища с узкими связями:
//
// - settings — настройки и темы;
// - notes — список заметок;
// - tabs, places — вкладки, места чтения и недавние (localStorage);
// - router — адрес страницы → что показать, переходы;
// - reader — показанная заметка: загрузка, статус, прокрутка;
// - updates — проверка изменений;
// - connection — связь с сервером («нет связи» и возврат).
//
// Чистая логика — в ../tabs.ts, ../places.ts, ../reading.ts (с Vitest).
// Интерфейс вокруг заметки (панели, глава, оглавление) — ../ui.svelte.ts.

import { api, rebaseStylesheets } from "../api";
import { rememberVault } from "../boot";
import { parseRoute } from "../ids";
import { connection } from "./connection.svelte";
import { notes } from "./notes.svelte";
import { places } from "./places.svelte";
import { reader } from "./reader.svelte";
import { router } from "./router.svelte";
import { settings } from "./settings.svelte";
import { tabs } from "./tabs.svelte";
import { updates } from "./updates.svelte";

export { connection, notes, places, reader, router, settings, tabs, updates };

/** Запуск клиента: настройки и список заметок с сервера, маршрут, прогрев. */
export async function start(): Promise<void> {
  rebaseStylesheets();
  connection.start();
  // Список заметок — после настроек: вид (кегль) меняет ширину названий
  // вкладок, а полоса вкладок прокручивается к активной по списку заметок.
  await Promise.all([settings.load(), notes.load()]);
  rememberVault();
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
