// Удобства одного читателя в localStorage: вкладки, недавние, места чтения.
// Хранилище может быть недоступно (приватное окно, запрет сайта) — тогда
// всё работает, просто не запоминается.

export function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw == null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function save(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // не запомнили — не страшно
  }
}
