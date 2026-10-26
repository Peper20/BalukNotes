import { expect, it, vi } from "vitest";
import type { SettingValues, VaultSettingsResponse } from "../api";

/** What the fake server holds, and the answers it has not sent yet. */
const server = { shared: { "appearance.font_size": 23 } as SettingValues, own: {} as SettingValues };
const held: (() => void)[] = [];
const sent: string[] = [];

/** A request is applied when it arrives and answered when the test releases it, with all the values of that moment. */
function answer<T>(apply: () => T): Promise<T> {
  const value = apply();
  return new Promise((resolve) => held.push(() => resolve(value)));
}

const vaultResponse = (): VaultSettingsResponse => ({
  schema: {
    settings: [{ key: "appearance.font_size", label: "", type: "number", min: 12, max: 32, step: 1, default: 19, shared: true, device: false }],
  } as unknown as VaultSettingsResponse["schema"],
  shared: { ...server.shared },
  own: { ...server.own },
  values: { ...server.shared, ...server.own },
});

vi.mock("../vault", () => ({ vault: () => "v" }));
vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: {
    vaultSettings: async () => vaultResponse(),
    themes: async () => [],
    saveSettings: (patch: SettingValues) => {
      sent.push(`shared ${JSON.stringify(patch)}`);
      return answer(() => Object.assign(server.shared, patch) && { ...server.shared });
    },
    saveVaultSettings: (patch: SettingValues) => {
      sent.push(`own ${JSON.stringify(patch)}`);
      return answer(() => {
        for (const [k, v] of Object.entries(patch)) {
          if (v === null) delete server.own[k];
          else server.own[k] = v;
        }
        return vaultResponse();
      });
    },
  },
}));
vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener: () => {} }));

const { settings } = await import("./settings.svelte");

/** Releases the held answers one by one, the latest first, until every change is done. */
async function answerLatestFirst(done: Promise<unknown>): Promise<void> {
  let finished = false;
  void done.then(() => (finished = true));
  while (!finished) {
    await new Promise((r) => setTimeout(r, 0));
    held.pop()?.();
  }
}

it("changes go to the server one at a time, in order: an older answer does not bring an older value back", async () => {
  await settings.load();
  // The value is the vault's own, then a field sends 25 twice (its change, then on blur) and a button makes it shared again.
  server.own["appearance.font_size"] = 25;
  await settings.load();
  const done = Promise.all([
    settings.save({ "appearance.font_size": 25 }),
    settings.useShared("appearance.font_size"),
  ]);
  // Only the first is sent until it is answered.
  await new Promise((r) => setTimeout(r, 0));
  expect(sent).toEqual(['own {"appearance.font_size":25}']);
  await answerLatestFirst(done);
  expect(sent).toEqual(['own {"appearance.font_size":25}', 'own {"appearance.font_size":null}']);
  expect(server.own).toEqual({});
  expect(settings.values["appearance.font_size"]).toBe(23);
  expect(settings.error).toBeNull();
});

it("where a change goes is decided in its turn: after \"only here\" it is the vault's", async () => {
  sent.length = 0;
  await settings.load();
  const done = Promise.all([settings.onlyHere("appearance.font_size"), settings.save({ "appearance.font_size": 27 })]);
  await answerLatestFirst(done);
  expect(sent).toEqual(['own {"appearance.font_size":23}', 'own {"appearance.font_size":27}']);
  expect(server.shared["appearance.font_size"]).toBe(23);
  expect(settings.values["appearance.font_size"]).toBe(27);
});
