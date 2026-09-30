import { afterEach, expect, it } from "vitest";
import { load, moveVault, save } from "./storage";
import { setVault, vaultBase } from "./vault";

afterEach(() => {
  setVault(null);
  localStorage.clear();
});

it("переименованное хранилище уносит свои ключи и адреса вкладок", () => {
  setVault("Учёба");
  save("k-tabs", [{ url: `${vaultBase("Учёба")}/n/A` }]);
  save("k-places", { A: 1 });
  setVault("a@Учёба");
  save("k-tabs", [{ url: "чужое" }]);
  localStorage.setItem("k-vault", JSON.stringify("Учёба"));

  moveVault("Учёба", "Учёба 2026", "k-vault");
  setVault("Учёба 2026");
  expect(load("k-tabs", [])).toEqual([{ url: `${vaultBase("Учёба 2026")}/n/A` }]);
  expect(load("k-places", {})).toEqual({ A: 1 });
  expect(localStorage.getItem("k-tabs@Учёба")).toBeNull();
  expect(localStorage.getItem("k-tabs@a@Учёба")).not.toBeNull();
  expect(JSON.parse(localStorage.getItem("k-vault")!)).toBe("Учёба 2026");

  moveVault("Учёба 2026", null, "k-vault");
  expect(load("k-tabs", null)).toBeNull();
  expect(localStorage.getItem("k-vault")).toBeNull();
});
