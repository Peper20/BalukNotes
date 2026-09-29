// Хранилище, которое показывает клиент: из адреса страницы `/v/<имя>/…`.
// Одна вкладка браузера — одно хранилище (как окно Obsidian): другое
// открывается переходом с перезагрузкой, поэтому имя не реактивное — его
// задают один раз при запуске (`boot.ts`), до загрузки состояния.

let current: string | null = null;

/** Показанное хранилище; null — ещё не выбрано (тесты, до запуска). */
export const vault = (): string | null => current;

export function setVault(name: string | null): void {
  current = name;
}

/** Начало адресов хранилища: `/v/<имя>`; без хранилища — "". */
export const vaultBase = (name: string | null = current): string => (name == null ? "" : `/v/${encodeURIComponent(name)}`);

/** Главная хранилища. */
export const vaultHome = (name: string | null = current): string => `${vaultBase(name)}/`;

/**
 * Хранилище и остаток адреса: `/v/Учёба/n/A` → { vault: "Учёба", rest: "/n/A" }.
 * Не адрес хранилища или битое кодирование — null.
 */
export function splitVaultPath(pathname: string): { vault: string; rest: string } | null {
  const m = /^\/v\/([^/]+)(\/.*)?$/.exec(pathname);
  if (!m) return null;
  try {
    return { vault: decodeURIComponent(m[1]!), rest: m[2] ?? "/" };
  } catch {
    return null;
  }
}

/**
 * Адрес внутри показанного хранилища: адрес без хранилища (`/n/A` — так
 * ссылки ставит ядро в HTML заметки, прежние адреса) получает `/v/<имя>`.
 */
export function inVault(url: string): string {
  if (!url.startsWith("/") || current == null || splitVaultPath(url.split(/[?#]/)[0]!)) return url;
  return vaultBase() + url;
}

/**
 * Какое хранилище открыть, когда адрес его не называет: открытое в прошлый
 * раз, если оно ещё есть. Хранилища по умолчанию нет — иначе null (выбор).
 */
export function chooseVault(list: string[], remembered: string | null): string | null {
  return remembered != null && list.includes(remembered) ? remembered : null;
}
