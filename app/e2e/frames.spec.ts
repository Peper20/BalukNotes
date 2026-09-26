// Кадры baluk: ползунок, кнопки, проигрывание, тема.
import { expect, test } from "@playwright/test";
import { open, resetTheme } from "./helpers";

test("кадры: виден кадр по умолчанию, ползунок и ‹ › переключают, место не прыгает", async ({ page }) => {
  await open(page, "Рисунки/Кадры");
  const frames = page.locator(".k-frames").first();
  await expect(frames).toHaveAttribute("data-live", "");
  const current = frames.locator(".k-frames-item[data-current]");
  await expect(current).toHaveCount(1);
  await expect(current.locator(".k-frames-label")).toHaveText("n = 4");
  await expect(frames.locator(".k-frames-count")).toHaveText("4 / 12");
  const height = await frames.evaluate((el) => el.getBoundingClientRect().height);

  await frames.locator("input[type=range]").fill("9");
  await expect(current.locator(".k-frames-label")).toHaveText("n = 10");
  await frames.locator(".k-frames-next").click();
  await expect(current.locator(".k-frames-label")).toHaveText("n = 11");
  await frames.locator(".k-frames-prev").click();
  await frames.locator(".k-frames-prev").click();
  await expect(frames.locator(".k-frames-count")).toHaveText("9 / 12");
  // все кадры в одной клетке: высота рисунка не меняется
  expect(await frames.evaluate((el) => el.getBoundingClientRect().height)).toBe(height);
  // видимый кадр — ровно один
  const visible = await frames.locator(".k-frames-item").evaluateAll((els) => els.filter((e) => getComputedStyle(e).visibility === "visible").length);
  expect(visible).toBe(1);
});

test("кадры: «▶» проигрывает до конца и останавливается, с конца — сначала", async ({ page }) => {
  await open(page, "Рисунки/Кадры");
  const frames = page.locator(".k-frames").nth(2); // шаги алгоритма: 4 кадра, 2 в секунду
  await frames.scrollIntoViewIfNeeded();
  const count = frames.locator(".k-frames-count");
  await expect(count).toHaveText("1 / 4");
  await frames.locator(".k-frames-play").click();
  await expect(frames.locator(".k-frames-play")).toHaveAttribute("aria-label", "пауза");
  await expect(count).toHaveText("4 / 4", { timeout: 5000 });
  await expect(frames.locator(".k-frames-play")).toHaveAttribute("aria-label", "проиграть");
  await expect(frames.locator(".k-frames-item[data-current] .k-frames-label")).toHaveText("проход 3");
  await frames.locator(".k-frames-play").click();
  await expect(count).toHaveText(/^[12] \/ 4$/);
  await frames.locator(".k-frames-play").click(); // пауза
  await expect(frames.locator(".k-frames-play")).toHaveAttribute("aria-label", "проиграть");
});

test("кадры: тема перекрашивает кадры, ползунок работает с клавиатуры", async ({ page }) => {
  await open(page, "Рисунки/Кадры");
  const frames = page.locator(".k-frames").first();
  const slider = frames.locator("input[type=range]");
  await slider.focus();
  await page.keyboard.press("ArrowRight");
  await expect(frames.locator(".k-frames-count")).toHaveText("5 / 12");
  await expect(slider).toHaveAttribute("aria-valuetext", "n = 5");

  // линия кадра: цвет — переменная темы в общем SVG
  const shown = () =>
    frames.locator(".k-frames-item[data-current] svg").first().evaluate((svg) => {
      const lines = [...svg.querySelectorAll("path")].map((p) => getComputedStyle(p).stroke).filter((c) => c !== "none");
      return lines.join(" ");
    });
  const light = await shown();
  await page.locator("#theme").click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "night");
  await expect.poll(shown).not.toBe(light);
  await resetTheme(page);
});
