// Client start, before the state loads: which vault to show. Tabs and
// reading places are per vault (localStorage), so the name is needed before
// the state modules read them.

import { api, type VaultsResponse } from "./api";
import { loadShared, moveVault, saveShared } from "./storage";
import { chooseVault, setVault, splitVaultPath, vault, vaultBase } from "./vault";

/** localStorage key: the vault opened last. */
const LAST = "k-vault";

/**
 * The vault comes from the address `/v/<name>/...`. An address without a
 * vault (`/`, old `/n/...`) gets the one opened last in this browser (the
 * page address gets `/v/<name>` without a new history entry). There is no
 * default vault: no last one - the vault list for the picker screen.
 */
export async function pickVault(): Promise<VaultsResponse | null> {
  const split = splitVaultPath(location.pathname);
  if (split) {
    setVault(split.vault);
    return null;
  }
  const list = await api.vaults();
  const name = chooseVault(list.vaults, loadShared<string | null>(LAST, null));
  if (name == null) return list;
  setVault(name);
  history.replaceState(null, "", vaultHref(name));
  return null;
}

/** Page address in the vault `name`: an old address without a vault is kept. */
export const vaultHref = (name: string): string => vaultBase(name) + location.pathname + location.search + location.hash;

/** A vault opened: next time an address without a vault opens it. */
export function rememberVault(): void {
  const name = vault();
  if (name != null) saveShared(LAST, name);
}

/** A vault was renamed (`to`) or deleted (null): its tabs and reading places follow it. */
export const vaultMoved = (from: string, to: string | null): void => moveVault(from, to, LAST);
