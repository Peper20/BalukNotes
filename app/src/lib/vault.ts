// The vault the client shows: from the page address `/v/<name>/...`. One
// browser tab is one vault (like an Obsidian window): another one opens by a
// navigation with a reload, so the name is not reactive - it is set once at
// start (`boot.ts`), before the state loads.

let current: string | null = null;

/** The shown vault; null - not chosen yet (tests, before start). */
export const vault = (): string | null => current;

export function setVault(name: string | null): void {
  current = name;
}

/** The start of vault addresses: `/v/<name>`; without a vault, "". */
export const vaultBase = (name: string | null = current): string => (name == null ? "" : `/v/${encodeURIComponent(name)}`);

/** Home of the vault. */
export const vaultHome = (name: string | null = current): string => `${vaultBase(name)}/`;

/**
 * The vault and the rest of the address: `/v/Учёба/n/A` -> { vault: "Учёба", rest: "/n/A" }.
 * Not a vault address or bad encoding - null.
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
 * An address inside the shown vault: an address without a vault (`/n/A`, as
 * the core puts links in the note HTML, old addresses) gets `/v/<name>`.
 */
export function inVault(url: string): string {
  if (!url.startsWith("/") || current == null || splitVaultPath(url.split(/[?#]/)[0]!)) return url;
  return vaultBase() + url;
}

/**
 * Which vault to open when the address does not name one: the one opened
 * last time if it still exists. There is no default vault, otherwise null (the picker).
 */
export function chooseVault(list: string[], remembered: string | null): string | null {
  return remembered != null && list.includes(remembered) ? remembered : null;
}
