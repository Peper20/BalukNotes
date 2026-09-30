// Удобства одного читателя в localStorage: вкладки, недавние, места чтения.
// Хранилище может быть недоступно (приватное окно, запрет сайта) — тогда
// всё работает, просто не запоминается.
//
// Ключи — свои у каждого хранилища заметок (`k-tabs@Учёба`): вкладки и
// места чтения одного не видны в другом. Общие для всех — `loadShared`.

import { vault, vaultBase } from "./vault";

/** Ключ в хранилище заметок, которое показано. */
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
    // не запомнили — не страшно
  }
}

export const load = <T>(key: string, fallback: T): T => read(scoped(key), fallback);
export const save = (key: string, value: unknown): void => write(scoped(key), value);

/** Общее для всех хранилищ заметок (какое открыто последним). */
export const loadShared = read;
export const saveShared = write;

/**
 * Хранилище переименовали (`to`) или удалили (`to` = null): его ключи —
 * под новым именем, адреса в них (`/v/<имя>/…`, вкладки) — тоже; удалённого
 * — забыть. Общий ключ `last` (открытое последним) — вслед за ним.
 */
export function moveVault(from: string, to: string | null, last: string): void {
  try {
    // Ключ — `<имя ключа>@<хранилище>`; в имени ключа `@` нет, в имени хранилища — может быть.
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
    // не перенесли — вкладки хранилища начнутся заново
  }
}
