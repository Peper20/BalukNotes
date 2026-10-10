// Where another vault opens: in this window (an ordinary navigation) or in a
// new one (`window.open`; the app window opens a window of its own or focuses
// the one that already shows the vault, a browser opens a tab). The decision
// is pure, the vault menu only carries it out.

export type VaultOpen = "this" | "new";

/** The part of a mouse event the decision depends on. */
export interface Gesture {
  button: number;
  ctrlKey: boolean;
  metaKey: boolean;
}

/**
 * The window for a vault: an explicit choice (the context menu) wins, then a
 * "new window" gesture (Ctrl, Cmd or middle click), then the setting
 * `vaults.open`; an unknown setting value means "this".
 */
export function vaultOpenMode(setting: unknown, gesture?: Gesture, explicit?: VaultOpen): VaultOpen {
  if (explicit) return explicit;
  if (gesture && (gesture.ctrlKey || gesture.metaKey || gesture.button === 1)) return "new";
  return setting === "new" ? "new" : "this";
}

export function openVault(href: string, mode: VaultOpen): void {
  if (mode === "new") window.open(href, "_blank");
  else location.assign(href);
}
