// Interactive baluk figures: sliders, coordinates, rotation, theme.
import { expect, test } from "@playwright/test";
import { open, resetTheme } from "./helpers";

test("plot: live instead of the frame, a slider changes the curve, coordinates under the pointer", async ({ page }) => {
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

test("surface: dragging rotates, a theme recolors", async ({ page }) => {
  await open(page, "Рисунки/Интерактив");
  const canvas = page.locator(".k-plot-canvas");
  await canvas.scrollIntoViewIfNeeded();
  await page.evaluate(async () => void (await document.fonts.ready));
  const frame = () => canvas.evaluate((c: HTMLCanvasElement) => c.toDataURL());
  await expect.poll(async () => (await frame()).length).toBeGreaterThan(5000);
  // the frame settled (labels redrawn after the fonts loaded)
  let first = "";
  await expect.poll(async () => first === (first = await frame())).toBe(true);

  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2 + 20, { steps: 5 });
  await page.mouse.up();
  await expect.poll(frame).not.toBe(first);

  // a double click restores the initial view
  await canvas.dblclick();
  await expect.poll(frame).toBe(first);

  // theme: the plot curve and the surface in the "night" colors
  const stroke = () => page.locator(".k-plot-curve").first().evaluate((p) => getComputedStyle(p).stroke);
  const light = await stroke();
  await page.locator("#theme").click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "night");
  await expect.poll(stroke).not.toBe(light);
  await expect.poll(frame).not.toBe(first);
  await resetTheme(page);
});
