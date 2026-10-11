// Where the client goes: the base address of the server, one setting.
//
// By default, the same server that served the page. Otherwise, the object
// `globalThis.__NOTES_API__ = { base }` before the client starts (Tauri: the
// window init script, `http://127.0.0.1:<port>`; a VPS: its own domain) or
// `configure(...)`. Sign-in is a cookie the browser sends itself (requests
// carry credentials, `api/index.ts`); no token is kept or put into addresses.

export interface ApiConfig {
  /** Server address without a trailing `/`; "" - the same as the page's. */
  base: string;
}

declare global {
  var __NOTES_API__: Partial<ApiConfig> | undefined;
}

const normalize = (c: Partial<ApiConfig> = {}): ApiConfig => ({ base: (c.base ?? "").replace(/\/+$/, "") });

let config = normalize(globalThis.__NOTES_API__);

export const apiConfig = (): ApiConfig => config;

export function configure(c: Partial<ApiConfig>): void {
  config = normalize({ ...config, ...c });
}

/** The full address of a server path. */
export const apiUrl = (path: string): string => config.base + path;

/** Styles from the server (fonts, theme colors, `baluk.css`), from the configured address. */
export function rebaseStylesheets(root: ParentNode = document): void {
  if (!config.base) return;
  for (const link of root.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"][href^="/"]')) {
    link.href = apiUrl(link.getAttribute("href")!);
  }
}
