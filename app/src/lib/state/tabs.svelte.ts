// Client tabs (remembered). Navigation between them is in router.

import { homeHref } from "../ids";
import { load, save } from "../storage";
import { clampActive, closeAt, dropTabs, openAfter, openBehind, type Tab, type TabList } from "../tabs";

class Tabs {
  list = $state<Tab[]>(load<{ tabs: Tab[] }>("k-tabs", { tabs: [] }).tabs);
  active = $state(load<{ active: number }>("k-tabs", { active: 0 }).active);
  /** How many background tabs were opened from the active one (`from`) since its last navigation. */
  #behind = { from: -1, url: "", count: 0 };

  /** After loading: at least one tab, the active one within limits. */
  restore(): void {
    if (!this.list.length) this.list = [{ url: homeHref() }];
    this.active = clampActive(this.active, this.list.length);
  }

  /** Address of the active tab, the page address. */
  setUrl(url: string): void {
    const tab = this.list[this.active];
    if (tab) tab.url = url;
    this.save();
  }

  openAfter(url: string): void {
    this.#set(openAfter(this.#state(), url));
  }

  /** A new background tab; a navigation in the active one or another active one - new ones go right after it. */
  openBehind(url: string): void {
    const from = this.list[this.active]?.url ?? "";
    if (this.#behind.from !== this.active || this.#behind.url !== from) this.#behind = { from: this.active, url: from, count: 0 };
    this.#set(openBehind(this.#state(), url, this.#behind.count++));
    this.save();
  }

  /** Closes it; null - the last tab (cannot be closed). */
  close(i: number): { wasActive: boolean } | null {
    const next = closeAt(this.#state(), i);
    if (next) this.#set(next);
    return next;
  }

  /** Removes the tabs for which `drop` is true; `true` - the active one was removed. */
  drop(drop: (tab: Tab) => boolean): boolean {
    const next = dropTabs(this.#state(), drop, homeHref());
    this.#set(next);
    this.save();
    return next.activeDropped;
  }

  /** New tab addresses (`url` -> a new one or null - the same). */
  move(url: (url: string) => string | null): void {
    for (const tab of this.list) tab.url = url(tab.url) ?? tab.url;
    this.save();
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
