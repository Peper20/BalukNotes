// Where the client goes: the base address of the server and the token, one setting.
//
// By default, the same server that served the page, without a token (the
// cookie `notes_token`, if the server has a token, the browser sends
// itself). Otherwise, the object `globalThis.__NOTES_API__ = { base, token }`
// before the client starts (Tauri: the window init script,
// `http://127.0.0.1:<port>` and the token; a VPS: its own domain) or
// `configure(...)`.

export interface ApiConfig {
  /** Server address without a trailing `/`; "" - the same as the page's. */
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
 * The full address of a server path. `withToken` is for addresses the
 * browser opens itself (PDF, events, styles): no header can be added there,
 * so the token goes as a parameter (the server answers with a cookie).
 */
export function apiUrl(path: string, { withToken = false } = {}): string {
  const url = config.base + path;
  if (!withToken || !config.token) return url;
  return `${url}${url.includes("?") ? "&" : "?"}token=${encodeURIComponent(config.token)}`;
}

/** Request headers from code: the token is `Authorization: Bearer`. */
export const authHeaders = (): Record<string, string> => (config.token ? { Authorization: `Bearer ${config.token}` } : {});

/** Styles from the server (fonts, theme colors, `baluk.css`), from the configured address. */
export function rebaseStylesheets(root: ParentNode = document): void {
  if (!config.base) return;
  for (const link of root.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"][href^="/"]')) {
    link.href = apiUrl(link.getAttribute("href")!, { withToken: true });
  }
}
