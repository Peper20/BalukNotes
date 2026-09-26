// Интерактивные рисунки konspekt: ползунки, координаты, вращение, тема.
import { expect, test } from "@playwright/test";
import { open } from "./helpers";

test("график: живой вместо кадра, ползунок меняет кривую, координаты под указателем", async ({ page }) => {
  await open(page, "Рисунки/Интерактив");
  const plot = page.locator(".k-plot").first();
  await expect(plot).toHaveAttribute("data-live", "");
  await expect(plot.locator(".k-frame")).toBeHidden();
  const curve = plot.locator(".k-plot-curve");
  const before = await curve.getAttribute("d");
  const slider = plot.locator("input[type=range]").first();
  await slider.fill("2.5");
  await expect(plot.locator("output").first()).toHaveText("2.5");
  await expect.poll(() => curve.getAttribute("d")).not.toBe(before);

  const svg = plot.locator(".k-plot-svg");
  const box = (await svg.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.6, box.y + box.height / 2);
  await expect(plot.locator(".k-plot-readout")).toContainText("x =");
  await expect(plot.locator(".k-plot-guide")).toHaveCount(1);
  await page.mouse.move(0, 0);
  await expect(plot.locator(".k-plot-guide")).toHaveCount(0);
});

test("поверхность: перетаскивание поворачивает, тема перекрашивает", async ({ page }) => {
  await open(page, "Рисунки/Интерактив");
  const canvas = page.locator(".k-plot-canvas");
  await canvas.scrollIntoViewIfNeeded();
  await page.evaluate(async () => void (await document.fonts.ready));
  const frame = () => canvas.evaluate((c: HTMLCanvasElement) => c.toDataURL());
  await expect.poll(async () => (await frame()).length).toBeGreaterThan(5000);
  // кадр устоялся (подписи перерисованы после загрузки шрифтов)
  let first = "";
  await expect.poll(async () => first === (first = await frame())).toBe(true);

  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2 + 20, { steps: 5 });
  await page.mouse.up();
  await expect.poll(frame).not.toBe(first);

  // двойной щелчок — исходный вид
  await canvas.dblclick();
  await expect.poll(frame).toBe(first);

  // тема: кривая графика и поверхность в цветах «ночи»
  const stroke = () => page.locator(".k-plot-curve").first().evaluate((p) => getComputedStyle(p).stroke);
  const light = await stroke();
  await page.locator("#theme").click();
  await page.locator("#theme").click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "ночь");
  await expect.poll(stroke).not.toBe(light);
  await expect.poll(frame).not.toBe(first);
});
