import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ApiError, type SyncStatus, type SyncVault } from "../api";

const calls = { status: 0, confirm: 0, restore: 0 };
let answer: () => Promise<SyncStatus>;
vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: {
    syncStatus: () => {
      calls.status++;
      return answer();
    },
    syncLink: async () => ({}),
    syncUnlink: async () => {},
    syncConfirm: async () => {
      calls.confirm++;
      return {};
    },
    syncRestore: async () => {
      calls.restore++;
      return {};
    },
  },
}));

const { sync } = await import("./sync.svelte");

const vault = (state: SyncVault["state"] = "idle"): SyncVault => ({
  name: "v",
  local: true,
  remote: true,
  linked: true,
  state,
  last_sync: null,
  error: null,
  report: null,
  held: null,
});
const status = (vaults: SyncVault[]): SyncStatus => ({ server: "https://s", login: "me", signed_in: true, server_error: null, session_ended: false, vaults });

beforeEach(() => {
  vi.useFakeTimers();
  calls.status = calls.confirm = calls.restore = 0;
});
afterEach(() => {
  sync.watch(false);
  vi.useRealTimers();
});

it("asks once on open and does not poll while nothing syncs", async () => {
  answer = async () => status([vault()]);
  sync.watch(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(calls.status).toBe(1);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(calls.status).toBe(1);
  expect(sync.loaded).toBe(true);
});

it("polls every few seconds while a vault is syncing, stops when it is done, and not after close", async () => {
  let n = 0;
  answer = async () => status([vault(++n < 3 ? "syncing" : "idle")]);
  sync.watch(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(calls.status).toBe(1);
  await vi.advanceTimersByTimeAsync(2000);
  expect(calls.status).toBe(2);
  await vi.advanceTimersByTimeAsync(2000);
  expect(calls.status).toBe(3);
  await vi.advanceTimersByTimeAsync(20_000);
  expect(calls.status).toBe(3);
  // Closed while syncing: no more asks.
  answer = async () => status([vault("syncing")]);
  sync.watch(false);
  sync.watch(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(calls.status).toBe(4);
  sync.watch(false);
  await vi.advanceTimersByTimeAsync(20_000);
  expect(calls.status).toBe(4);
});

it("after an action: the status again, then a couple more asks, then quiet", async () => {
  answer = async () => status([vault()]);
  sync.watch(true);
  await vi.advanceTimersByTimeAsync(0);
  await sync.setLinked("v", false);
  expect(calls.status).toBe(2);
  await vi.advanceTimersByTimeAsync(2000);
  await vi.advanceTimersByTimeAsync(2000);
  expect(calls.status).toBe(4);
  await vi.advanceTimersByTimeAsync(30_000);
  expect(calls.status).toBe(4);
});

it("confirming or restoring held deletions is a row action that asks the status again", async () => {
  answer = async () => status([{ ...vault("held"), held: { side: "server", count: 12, total: 12 } }]);
  sync.watch(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(sync.status?.vaults[0]?.held?.count).toBe(12);
  await sync.restoreFiles("v");
  await sync.confirmDeletion("v");
  expect([calls.restore, calls.confirm]).toEqual([1, 1]);
  expect(calls.status).toBe(3);
  expect(sync.busy).toEqual({});
});

it("a server without sync (404): unavailable, no polling", async () => {
  answer = async () => {
    throw new ApiError("this server runs without vault sync", 404);
  };
  sync.watch(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(sync.unavailable).toBe(true);
  await vi.advanceTimersByTimeAsync(10_000);
  expect(calls.status).toBe(1);
});
