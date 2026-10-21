// Where the client goes: the base address of the server, one setting.
//
// By default, the same server that served the page. Otherwise, the object
// `globalThis.__NOTES_API__ = { base }` before the client starts (Tauri: the
// window init script, `http://127.0.0.1:<port>`; a VPS: its own domain) or
// `configure(...)`; the hub's browser version sets `core` in `assets/config.js`
// (`tools/web-bundle.sh`; the file in the repository sets nothing). Sign-in is a cookie the browser sends itself (requests
// carry credentials, `api/index.ts`); no token is kept or put into addresses.
//
// With `core` set the `/api/...` requests go to the core in a Web Worker
// instead of the server (`transport.ts`); `base` still serves everything else
// (stylesheets, fonts, the PDF).

import type { CoreSource } from "../../worker/protocol";

export type { CoreSource };

export interface ApiConfig {
  /** Server address without a trailing `/`; "" - the same as the page's. */
  base: string;
  /** The core in a worker: the module, the packages, the vault name and files (all but the module optional). Unset: the server answers. */
  core?: CoreSource;
}

declare global {
  var __NOTES_API__: Partial<ApiConfig> | undefined;
}

const normalize = (c: Partial<ApiConfig> = {}): ApiConfig => ({
  base: (c.base ?? "").replace(/\/+$/, ""),
  ...(c.core && { core: c.core }),
});

// Read as an own property: an element with the same id would be the global by name (named access on window).
let config = normalize(Object.getOwnPropertyDescriptor(globalThis, "__NOTES_API__")?.value as Partial<ApiConfig> | undefined);

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
