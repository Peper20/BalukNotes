// Запуск клиента, до загрузки состояния: какое хранилище показать. Вкладки
// и места чтения у каждого хранилища свои (localStorage), поэтому имя
// нужно раньше, чем их прочитают модули состояния.

import { api, type VaultsResponse } from "./api";
import { loadShared, saveShared } from "./storage";
import { chooseVault, setVault, splitVaultPath, vault, vaultBase } from "./vault";

/** Ключ localStorage: хранилище, открытое последним. */
const LAST = "k-vault";

/**
 * Хранилище — из адреса `/v/<имя>/…`. Адрес без хранилища (`/`, прежние
 * `/n/…`) — открытое в этом браузере последним (адрес страницы получает
 * `/v/<имя>`, без новой записи в истории). Хранилища по умолчанию нет:
 * последнего нет — список хранилищ для экрана выбора.
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

/** Адрес страницы в хранилище `name`: прежний адрес без хранилища сохраняется. */
export const vaultHref = (name: string): string => vaultBase(name) + location.pathname + location.search + location.hash;

/** Хранилище открылось: адрес без хранилища в следующий раз откроет его. */
export function rememberVault(): void {
  const name = vault();
  if (name != null) saveShared(LAST, name);
}
