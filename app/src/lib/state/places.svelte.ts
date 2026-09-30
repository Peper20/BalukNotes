// Где читали каждую заметку и недавно открытые — удобства одного читателя
// (localStorage).

import { load, save } from "../storage";
import { pushRecent, rememberPlace, type Place } from "../places";

const MAX_RECENT = 30;
const MAX_PLACES = 200;

class Places {
  /** Недавно открытые заметки, свежие первыми. */
  recent = $state<string[]>(load<string[]>("k-recent", []));
  #places = load<Record<string, Place>>("k-places", {});

  get(id: string): Place | null {
    return this.#places[id] ?? null;
  }

  remember(id: string, place: Place): void {
    this.#places = rememberPlace(this.#places, id, place, MAX_PLACES);
    save("k-places", this.#places);
  }

  /** Заметку удалили: не помнить ни места, ни в недавних. */
  forget(id: string): void {
    const { [id]: _, ...rest } = this.#places;
    this.#places = rest;
    this.recent = this.recent.filter((r) => r !== id);
    save("k-places", this.#places);
    save("k-recent", this.recent);
  }

  /** Заметку (папку) переименовали: места и недавние — под новыми путями. */
  move(moved: (id: string) => string | null): void {
    this.#places = Object.fromEntries(Object.entries(this.#places).map(([id, place]) => [moved(id) ?? id, place]));
    this.recent = this.recent.map((id) => moved(id) ?? id);
    save("k-places", this.#places);
    save("k-recent", this.recent);
  }

  visited(id: string): void {
    this.recent = pushRecent(this.recent, id, MAX_RECENT);
    save("k-recent", this.recent);
  }
}

export const places = new Places();
