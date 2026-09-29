// Вкладки клиента (запоминаются). Переходы между ними — router.

import { homeHref } from "../ids";
import { load, save } from "../storage";
import { clampActive, closeAt, dropTabs, openAfter, type Tab, type TabList } from "../tabs";

class Tabs {
  list = $state<Tab[]>(load<{ tabs: Tab[] }>("k-tabs", { tabs: [] }).tabs);
  active = $state(load<{ active: number }>("k-tabs", { active: 0 }).active);

  /** После загрузки: хотя бы одна вкладка, активная — в пределах. */
  restore(): void {
    if (!this.list.length) this.list = [{ url: homeHref() }];
    this.active = clampActive(this.active, this.list.length);
  }

  /** Адрес активной вкладки — адрес страницы. */
  setUrl(url: string): void {
    const tab = this.list[this.active];
    if (tab) tab.url = url;
    this.save();
  }

  openAfter(url: string): void {
    this.#set(openAfter(this.#state(), url));
  }

  /** Закрыть; null — последняя вкладка (закрыть нельзя). */
  close(i: number): { wasActive: boolean } | null {
    const next = closeAt(this.#state(), i);
    if (next) this.#set(next);
    return next;
  }

  /** Убрать вкладки, для которых `drop` — истина; `true` — убрана активная. */
  drop(drop: (tab: Tab) => boolean): boolean {
    const next = dropTabs(this.#state(), drop, homeHref());
    this.#set(next);
    this.save();
    return next.activeDropped;
  }

  save(): void {
    save("k-tabs", { tabs: this.list, active: this.active });
  }

  #state(): TabList {
    return { tabs: this.list, active: this.active };
  }

  #set({ tabs, active }: TabList): void {
    this.list = tabs;
    this.active = active;
  }
}

export const tabs = new Tabs();
