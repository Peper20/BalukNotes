import { expect, it } from "vitest";
import { problemText } from "./settings-problem";

it("problemText: the file and the place of the error", () => {
  const text = problemText({ path: "/v/.baluk/settings.json", message: "x", line: 3, column: 7 });
  expect(text).toContain("строка 3, столбец 7");
  expect(text).toContain("/v/.baluk/settings.json");
  expect(text).toContain("не сохраняются");
  expect(problemText({ path: "p", message: "x" })).not.toContain("строка");
});
