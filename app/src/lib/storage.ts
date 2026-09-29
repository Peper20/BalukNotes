// Удобства одного читателя в localStorage: вкладки, недавние, места чтения.
// Хранилище может быть недоступно (приватное окно, запрет сайта) — тогда
// всё работает, просто не запоминается.
//
// Ключи — свои у каждого хранилища заметок (`k-tabs@Учёба`): вкладки и
// места чтения одного не видны в другом. Общие для всех — `loadShared`.

import { vault } from "./vault";

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
