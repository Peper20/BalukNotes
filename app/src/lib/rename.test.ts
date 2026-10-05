import { expect, it } from "vitest";
import { movedId } from "./rename";

it("renaming: the path itself and everything inside, not neighbours with the same start", () => {
  expect(movedId("Сеть", "Сеть", "Сети")).toBe("Сети");
  expect(movedId("Сеть/SSH", "Сеть", "Сети")).toBe("Сети/SSH");
  expect(movedId("Сеть 2/SSH", "Сеть", "Сети")).toBeNull();
  expect(movedId("Другое", "Сеть", "Сети")).toBeNull();
});
