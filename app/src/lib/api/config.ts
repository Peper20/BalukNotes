// Куда ходит клиент: базовый адрес сервера и токен — одна настройка.
//
// По умолчанию — тот же сервер, что отдал страницу, без токена (cookie
// `notes_token`, если сервер с токеном, браузер шлёт сам). Иначе — объект
// `globalThis.__NOTES_API__ = { base, token }` до запуска клиента (Tauri:
// скрипт инициализации окна, `http://127.0.0.1:<порт>` и токен; VPS — свой
// домен) или `configure(…)`.

export interface ApiConfig {
  /** Адрес сервера без `/` в конце; "" — тот же, что у страницы. */
  base: string;
  token: string | null;
}

declare global {
  var __NOTES_API__: Partial<ApiConfig> | undefined;
}

const normalize = (c: Partial<ApiConfig> = {}): ApiConfig => ({ base: (c.base ?? "").replace(/\/+$/, ""), token: c.token || null });

let config = normalize(globalThis.__NOTES_API__);

export const apiConfig = (): ApiConfig => config;

export function configure(c: Partial<ApiConfig>): void {
  config = normalize({ ...config, ...c });
}

/**
 * Полный адрес пути сервера. `withToken` — для адресов, которые открывает
 * сам браузер (PDF, события, стили): заголовок туда не добавить, поэтому
 * токен — параметром (сервер ответит cookie).
 */
export function apiUrl(path: string, { withToken = false } = {}): string {
  const url = config.base + path;
  if (!withToken || !config.token) return url;
  return `${url}${url.includes("?") ? "&" : "?"}token=${encodeURIComponent(config.token)}`;
}

/** Заголовки запроса из кода: токен — `Authorization: Bearer`. */
export const authHeaders = (): Record<string, string> => (config.token ? { Authorization: `Bearer ${config.token}` } : {});

/** Стили с сервера (шрифты, цвета тем, `baluk.css`) — с настроенного адреса. */
export function rebaseStylesheets(root: ParentNode = document): void {
  if (!config.base) return;
  for (const link of root.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"][href^="/"]')) {
    link.href = apiUrl(link.getAttribute("href")!, { withToken: true });
  }
}
