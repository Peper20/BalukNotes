import { afterEach, expect, it } from "vitest";
import { chooseVault, inVault, setVault, splitVaultPath, vaultBase, vaultHome } from "./vault";

afterEach(() => setVault(null));

it("адрес хранилища: имя кодируется, читается обратно", () => {
  expect(vaultBase("Учёба 2026")).toBe(`/v/${encodeURIComponent("Учёба 2026")}`);
  expect(vaultHome(null)).toBe("/");
  const path = new URL(`${vaultBase("C++ и C#")}/n/A`, "http://x").pathname;
  expect(splitVaultPath(path)).toEqual({ vault: "C++ и C#", rest: "/n/A" });
  expect(splitVaultPath("/v/x")).toEqual({ vault: "x", rest: "/" });
  expect(splitVaultPath("/v/x/")).toEqual({ vault: "x", rest: "/" });
  expect(splitVaultPath("/n/A")).toBeNull();
  expect(splitVaultPath("/v/%E0%A4%A/")).toBeNull();
});

it("адреса без хранилища — в показанное", () => {
  expect(inVault("/n/A")).toBe("/n/A");
  setVault("Учёба");
  const base = vaultBase();
  expect(inVault("/n/A#x")).toBe(`${base}/n/A#x`);
  expect(inVault("/graph?around=A")).toBe(`${base}/graph?around=A`);
  expect(inVault(`${base}/n/A`)).toBe(`${base}/n/A`);
  expect(inVault("/v/Другое/n/A")).toBe("/v/Другое/n/A");
  expect(inVault("#якорь")).toBe("#якорь");
});

it("какое хранилище открыть без имени в адресе: только открытое последним", () => {
  expect(chooseVault(["А", "Б"], "Б")).toBe("Б");
  expect(chooseVault(["А", "Б"], "удалённое")).toBeNull();
  expect(chooseVault(["А"], null)).toBeNull();
  expect(chooseVault([], null)).toBeNull();
});
