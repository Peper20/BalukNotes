import { expect, it } from "vitest";
import { movedId } from "./rename";

it("переименование: сам путь и всё внутри, соседи с тем же началом — нет", () => {
  expect(movedId("Сеть", "Сеть", "Сети")).toBe("Сети");
  expect(movedId("Сеть/SSH", "Сеть", "Сети")).toBe("Сети/SSH");
  expect(movedId("Сеть 2/SSH", "Сеть", "Сети")).toBeNull();
  expect(movedId("Другое", "Сеть", "Сети")).toBeNull();
});
