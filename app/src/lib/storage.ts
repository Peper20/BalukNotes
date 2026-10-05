// One reader's conveniences in localStorage: tabs, recent notes, reading
// places. The storage may be unavailable (a private window, the site is
// blocked) - then everything works, just without remembering.
//
// Keys are per note vault (`k-tabs@Учёба`): the tabs and reading places of
// one are not seen in another. Shared by all - `loadShared`.

import { vault, vaultBase } from "./vault";

/** The key in the shown note vault. */
const scoped = (key: string): string => {
  const name = vault();
  return name == null ? key : `${key}@${name}`;
};

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw == null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

function write(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // not remembered: no harm
  }
}

export const load = <T>(key: string, fallback: T): T => read(scoped(key), fallback);
export const save = (key: string, value: unknown): void => write(scoped(key), value);

/** Shared by all note vaults (which one was opened last). */
export const loadShared = read;
export const saveShared = write;

/**
 * A vault was renamed (`to`) or deleted (`to` = null): its keys move under
 * the new name, the addresses in them (`/v/<name>/...`, tabs) too; a deleted
 * one's are forgotten. The shared key `last` (opened last) follows it.
 */
export function moveVault(from: string, to: string | null, last: string): void {
  try {
    // A key is `<key name>@<vault>`; the key name has no `@`, a vault name may.
    const base = (key: string | null) => (key != null && key.slice(key.indexOf("@") + 1) === from && key.includes("@") ? key.slice(0, key.indexOf("@")) : null);
    const keys = Array.from({ length: localStorage.length }, (_, i) => localStorage.key(i)).filter((k) => base(k) != null) as string[];
    const [old, now] = [`${vaultBase(from)}/`, to == null ? "" : `${vaultBase(to)}/`];
    for (const key of keys) {
      const raw = localStorage.getItem(key);
      localStorage.removeItem(key);
      if (to == null || raw == null) continue;
      localStorage.setItem(`${base(key)}@${to}`, raw.split(old).join(now));
    }
    if (read<string | null>(last, null) === from) {
      if (to == null) localStorage.removeItem(last);
      else write(last, to);
    }
  } catch {
    // not moved: the vault's tabs start anew
  }
}
