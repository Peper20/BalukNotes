// Маршрут: адрес страницы → что показать (заметка, главная, граф, теги).
// Переходы внутри клиента, вкладки, «назад» к месту чтения.

import { hashAnchor, homeHref, noteHref, parseRoute, type Route } from "../ids";
import { inVault } from "../vault";
import { placeFromHistory } from "../places";
import { notes } from "./notes.svelte";
import { places } from "./places.svelte";
import { reader } from "./reader.svelte";
import { tabs } from "./tabs.svelte";

class Router {
  route = $state.raw<Route>({ kind: "home" });
  /** Якорь адреса; `anchorSeq` растёт при переходе по якорю в той же заметке. */
  anchor = $state<string | null>(null);
  anchorSeq = $state(0);

  currentId = $derived(this.route.kind === "note" ? this.route.id : null);
  currentNote = $derived(notes.byId(this.currentId));

  /**
   * Перейти внутри клиента (как по ссылке); `newTab` — в новой вкладке.
   * Адрес без хранилища (`/n/…` из HTML заметки) — в показанном хранилище.
   */
  go(url: string | URL, { replace = false, newTab = false } = {}): void {
    url = inVault(String(url));
    reader.remember();
    if (newTab) tabs.openAfter(String(url));
    if (replace) history.replaceState(null, "", url);
    else history.pushState(null, "", url);
    this.sync();
  }

  open(id: string, anchor?: string | null, { newTab = false } = {}): void {
    this.go(noteHref(id, anchor), { newTab });
  }

  switchTab(i: number): void {
    const tab = tabs.list[i];
    if (!tab || i === tabs.active) return;
    reader.remember();
    tabs.active = i;
    history.pushState(null, "", tab.url);
    this.sync();
  }

  closeTab(i: number): void {
    if (i < 0 || i >= tabs.list.length) return;
    if (tabs.list.length === 1) {
      this.go(homeHref());
      return;
    }
    if (tabs.close(i)?.wasActive) {
      history.replaceState(null, "", tabs.list[tabs.active]!.url);
      this.sync();
    }
    tabs.save();
  }

  /** Маршрут по адресу страницы: та же заметка — только якорь. */
  sync({ pop = false } = {}): void {
    const route = parseRoute(location.pathname, location.search);
    this.anchor = hashAnchor(location.hash);
    tabs.setUrl(location.pathname + location.search + location.hash);
    // «Назад» — туда, где были в этой записи истории; иначе — где читали
    // эту заметку в прошлый раз (если адрес без якоря).
    const fromHistory = pop ? placeFromHistory(history.state) : null;
    if (route.kind === "note" && route.id === this.currentId && !reader.pending) {
      reader.restore = this.anchor ? null : fromHistory;
      this.anchorSeq++;
      return;
    }
    this.route = route;
    if (route.kind === "note") reader.show(route.id, fromHistory ?? (this.anchor ? null : places.get(route.id)));
    else reader.clear();
  }
}

export const router = new Router();
