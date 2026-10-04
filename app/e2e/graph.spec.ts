// Страница графа: масштаб, сдвиг, перестановка узла, фильтры, поиск, соседи заметки.
import { expect, test, type Page } from "@playwright/test";
import { noteUrl, open, ready, title, vaultUrl } from "./helpers";

const scale = (page: Page) =>
  page.locator(".graph-svg > g").evaluate((g) => Number(/scale\(([\d.]+)\)/.exec(g.getAttribute("transform") ?? "")?.[1]));
const nodeAt = (page: Page, id: string) =>
  page.locator(`.graph-node[data-id="${id}"]`).evaluate((g) => g.getAttribute("transform"));

test.beforeEach(async ({ page }) => {
  await page.goto(vaultUrl("/"));
  await page.evaluate(() => localStorage.removeItem("k-graph@vault"));
});

test("граф: значок внизу боковой панели открывает граф хранилища", async ({ page }) => {
  await open(page, "демо/компоненты");
  await page.locator("#open-graph").click();
  await expect(page).toHaveURL(vaultUrl("/graph"));
  await expect(page.locator(".graph-node")).not.toHaveCount(0);
});

test("граф: кнопки и колесо масштабируют, узел переставляется, щелчок открывает заметку, Ctrl+щелчок - в фоне", async ({ page }) => {
  await page.goto(vaultUrl("/graph"));
  await ready(page);
  await expect(page.locator(".graph-node")).not.toHaveCount(0);
  const k0 = await scale(page);
  await page.getByRole("button", { name: "крупнее" }).click();
  expect(await scale(page)).toBeCloseTo(k0 * 1.3, 3);
  await page.getByRole("button", { name: "вписать" }).click();
  expect(await scale(page)).toBeCloseTo(k0, 3);

  const box = (await page.locator(".graph-svg").boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, -300);
  await expect.poll(() => scale(page)).toBeGreaterThan(k0);
  await page.getByRole("button", { name: "вписать" }).click();

  // протянуть узел — переставить, заметка не открывается
  const ssh = page.locator('.graph-node[data-id="Сеть/SSH"] circle');
  const before = await nodeAt(page, "Сеть/SSH");
  const c = (await ssh.boundingBox())!;
  await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
  await page.mouse.down();
  await page.mouse.move(c.x + 60, c.y + 40, { steps: 5 });
  await page.mouse.up();
  expect(await nodeAt(page, "Сеть/SSH")).not.toBe(before);
  await expect(page).toHaveURL(/\/graph$/);

  // Ctrl+щелчок — заметка в фоновой вкладке, граф остаётся
  await ssh.click({ modifiers: ["Control"] });
  await expect(page.locator(".tabbar .tab")).toHaveCount(2);
  await expect(page).toHaveURL(/\/graph$/);

  await ssh.click();
  await ready(page);
  await expect(title(page)).toHaveText("SSH");
});

test("граф: папки, тег, поиск; фильтры запоминаются", async ({ page }) => {
  await page.goto(vaultUrl("/graph"));
  await ready(page);
  const nodes = page.locator(".graph-node");
  const all = await nodes.count();

  await page.locator(".graph-legend button", { hasText: "Сеть" }).click();
  await expect(page.locator('.graph-node[data-id="Сеть/SSH"]')).toHaveCount(0);
  await expect(nodes).not.toHaveCount(all);
  await page.locator(".graph-legend button", { hasText: "Сеть" }).click();
  await expect(nodes).toHaveCount(all);

  await page.getByLabel("Тег", { exact: true }).selectOption("безопасность");
  await expect(nodes).toHaveCount(2);
  await page.reload();
  await ready(page);
  await expect(nodes).toHaveCount(2);
  await page.getByLabel("Тег", { exact: true }).selectOption({ label: "все теги" });
  await expect(nodes).toHaveCount(all);

  await page.getByLabel("Найти на графе").fill("ufw");
  await expect(page.locator(".graph-node.hit")).toHaveCount(1);
  await expect(page.locator(".graph-count")).toContainText("найдено 1");
});

test("граф: книги главами — корень и главы, щелчок по главе открывает книгу на ней; переключатель запоминается", async ({ page }) => {
  await page.goto(vaultUrl("/graph"));
  await ready(page);
  await expect(page.locator('.graph-node[data-id="Книга/.2"]')).toHaveCount(0);
  await page.getByLabel("книги главами").check();
  const chapters = page.locator('.graph-node.chapter[data-id^="Книга/"]');
  await expect(chapters).toHaveCount(3);
  // Связь «книга - глава» — у каждой главы (ещё одна книга фикстуры — English).
  await expect(page.locator(".graph-edge.chapter")).toHaveCount(await page.locator(".graph-node.chapter").count());
  await expect(page.locator('.graph-node.book[data-id="Книга"]')).toHaveCount(1);
  await page.reload();
  await ready(page);
  await expect(chapters).toHaveCount(3);
  await page.locator('.graph-node[data-id="Книга/.2"] circle').click();
  await ready(page);
  await expect.poll(() => decodeURIComponent(new URL(page.url()).pathname + new URL(page.url()).hash)).toBe("/v/vault/n/Книга#Продолжение");
});

test("граф: соседи заметки — из «Ссылаются сюда», глубина в адресе", async ({ page }) => {
  await open(page, "Сеть/SSH");
  await page.locator("#backlinks").getByRole("link", { name: "на графе" }).click();
  await ready(page);
  await expect(page).toHaveURL(/\/graph\?around=/);
  await expect(page.locator(".graph-node.center")).toHaveAttribute("data-id", "Сеть/SSH");
  const near = await page.locator(".graph-node").count();
  await page.getByLabel("Глубина").selectOption("2");
  await expect(page).toHaveURL(/depth=2/);
  expect(await page.locator(".graph-node").count()).toBeGreaterThanOrEqual(near);
  await page.locator(".graph-title").getByRole("link", { name: "весь граф" }).click();
  await ready(page);
  await expect(page.locator(".graph-node.center")).toHaveCount(0);
  await expect(page).toHaveURL(/\/graph$/);
  // вернуться к заметке — по ссылке в заголовке страницы соседей
  await page.goBack();
  await ready(page);
  await page.locator(".graph-title").getByRole("link", { name: "SSH" }).click();
  await ready(page);
  await expect(page).toHaveURL(noteUrl("Сеть/SSH"));
});

test("граф хранилища в заметке: живой поверх картинки, щелчок открывает заметку", async ({ page }) => {
  await open(page, "Рисунки/Граф хранилища");
  const graph = page.locator(".k-graph").first();
  await expect(graph).toHaveAttribute("data-live", "");
  await expect(graph.locator(".k-frame")).toBeHidden();
  await expect(graph.locator(".graph-node.center")).toHaveAttribute("data-id", "Сеть/SSH");
  await graph.locator('.graph-node[data-id="Сеть/UFW"]').click();
  await expect(page).toHaveURL(noteUrl("Сеть/UFW"));
});

test("граф в заметке: узел тянет соседей, не уходит за рамку рисунка и не выделяет текст за ней", async ({ page }) => {
  await open(page, "Рисунки/Граф хранилища");
  const svg = page.locator(".k-graph .graph-svg").first();
  await expect(svg).toBeVisible();
  const inside = async () => {
    const frame = (await svg.boundingBox())!;
    const boxes = await svg.locator(".graph-node").evaluateAll((gs) => gs.map((g) => g.getBoundingClientRect().toJSON() as DOMRect));
    for (const b of boxes) {
      expect(b.left).toBeGreaterThanOrEqual(frame.x - 1);
      expect(b.right).toBeLessThanOrEqual(frame.x + frame.width + 1);
      expect(b.top).toBeGreaterThanOrEqual(frame.y - 1);
      expect(b.bottom).toBeLessThanOrEqual(frame.y + frame.height + 1);
    }
  };
  const at = (id: string) => svg.locator(`.graph-node[data-id="${id}"]`).evaluate((g) => g.getAttribute("transform"));
  const neighbour = "Сеть/UFW";
  const before = await at(neighbour);

  const c = (await svg.locator('.graph-node[data-id="Сеть/SSH"] circle').boundingBox())!;
  await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
  await page.mouse.down();
  await page.mouse.move(c.x + 2000, c.y + 1500, { steps: 20 });
  await expect.poll(() => at(neighbour)).not.toBe(before); // сосед потянулся
  await inside();
  expect(await page.evaluate(() => getSelection()?.toString() ?? "")).toBe(""); // указатель за рамкой
  await page.mouse.up();
  await page.waitForTimeout(1500); // осели
  await inside();
  await expect(page).toHaveURL(noteUrl("Рисунки/Граф хранилища")); // протянуть — не щелчок
});

test.describe("без движения (prefers-reduced-motion)", () => {
  test.use({ reducedMotion: "reduce" });

  test("граф не анимируется, протянутый узел соседей не тянет", async ({ page }) => {
    await page.goto(vaultUrl("/graph"));
    await ready(page);
    const circle = page.locator('.graph-node[data-id="Сеть/SSH"] circle');
    expect(await circle.evaluate((c) => getComputedStyle(c).animationName)).toBe("none");
    const neighbour = await nodeAt(page, "Сеть/UFW");
    const c = (await circle.boundingBox())!;
    await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
    await page.mouse.down();
    await page.mouse.move(c.x + 120, c.y + 80, { steps: 5 });
    await page.waitForTimeout(200);
    expect(await nodeAt(page, "Сеть/UFW")).toBe(neighbour);
    await page.mouse.up();
    // смена фильтра — сразу, без угасания: ушедший узел пропал вместе с новым счётчиком
    const count = await page.locator(".graph-count").textContent();
    await page.locator(".graph-legend button", { hasText: "Сеть" }).click();
    await expect(page.locator(".graph-count")).not.toHaveText(count!);
    expect(await page.locator('.graph-node[data-id="Сеть/SSH"]').count()).toBe(0);
  });
});

test("граф: «вернуть раскладку» — узлы плавно, без перелёта, возвращаются на места", async ({ page }) => {
  await page.goto(vaultUrl("/graph"));
  await ready(page);
  const restore = page.getByRole("button", { name: "вернуть раскладку" });
  await expect(restore).toBeDisabled();
  const xy = (id: string) =>
    page.locator(`.graph-node[data-id="${id}"]`).evaluate((g) => (g.getAttribute("transform") ?? "").match(/[-\d.]+/g)!.map(Number));
  const ids = await page.locator(".graph-node").evaluateAll((gs) => gs.map((g) => (g as SVGElement).dataset.id!));
  const home = new Map(await Promise.all(ids.map(async (id) => [id, await xy(id)] as const)));

  // Протянуть узел — соседи тянутся за ним, после отпускания возвращаются лишь на четверть.
  const ssh = page.locator('.graph-node[data-id="Сеть/SSH"] circle');
  const c = (await ssh.boundingBox())!;
  await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
  await page.mouse.down();
  await page.mouse.move(c.x + 160, c.y + 90, { steps: 10 });
  await page.mouse.up();
  await expect(restore).toBeEnabled();
  await page.waitForTimeout(1500); // физика осела
  const away = async () => {
    let far = 0;
    for (const id of ids) {
      const [x, y] = await xy(id);
      const [hx, hy] = home.get(id)!;
      far = Math.max(far, Math.hypot(x! - hx!, y! - hy!));
    }
    return far;
  };
  expect(await away()).toBeGreaterThan(20);

  // Вернуть: расстояние до дома у узла SSH только убывает, в конце — ровно дома.
  const track = page.locator('.graph-node[data-id="Сеть/SSH"]').evaluate(
    (g, [hx, hy]) =>
      new Promise<number[]>((done) => {
        const seen: number[] = [];
        const start = performance.now();
        const tick = () => {
          const [x, y] = (g.getAttribute("transform") ?? "").match(/[-\d.]+/g)!.map(Number);
          seen.push(Math.hypot(x! - hx!, y! - hy!));
          if (performance.now() - start < 900) requestAnimationFrame(tick);
          else done(seen);
        };
        requestAnimationFrame(tick);
      }),
    home.get("Сеть/SSH")!,
  );
  await restore.click();
  const dist = await track;
  for (let i = 1; i < dist.length; i++) expect(dist[i]!).toBeLessThanOrEqual(dist[i - 1]! + 1e-6);
  expect(dist.at(-1)!).toBeLessThan(1e-6);
  expect(await away()).toBeLessThan(1e-6);
  await expect(restore).toBeDisabled();
});

test("силы графа: выдвижная панель сбоку, «Папки» меняют раскладку, значение запоминается, «По умолчанию» - обратно", async ({ page }) => {
  try {
    await page.goto(vaultUrl("/graph"));
    await ready(page);
    const before = await nodeAt(page, "Сеть/SSH");
    const panel = page.locator(".graph-forces");
    const toggle = page.getByRole("button", { name: "силы графа", exact: true });
    await expect(panel).toBeHidden(); // по умолчанию спрятана
    await toggle.click();
    await expect(panel).toBeVisible();
    await page.getByRole("button", { name: "спрятать силы графа" }).click();
    await expect(panel).toBeHidden();
    await toggle.click();
    await expect(panel).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    await toggle.click();
    await expect.poll(async () => (await panel.boundingBox())?.width).toBeGreaterThan(200); // выехала целиком
    // Сбоку, а не поверх графа: холст сжимается вместе с выездом панели - ждать конца.
    await expect
      .poll(async () => {
        const [canvas, side] = await Promise.all([page.locator(".graph-canvas").boundingBox(), panel.boundingBox()]);
        return side!.x - (canvas!.x + canvas!.width);
      })
      .toBeGreaterThanOrEqual(-1);
    const clusters = page.locator('input[data-key="graph.clusters"]');
    await clusters.fill("300");
    await expect.poll(() => nodeAt(page, "Сеть/SSH")).not.toBe(before);
    await expect(panel.locator("output").first()).toHaveText("300 %");

    await page.reload();
    await ready(page);
    await expect(panel).toBeHidden(); // после перезагрузки снова спрятана
    await toggle.click();
    await expect(clusters).toHaveValue("300");
    await page.getByRole("button", { name: "По умолчанию" }).click();
    await expect(clusters).toHaveValue("5");
    await expect.poll(() => nodeAt(page, "Сеть/SSH")).toBe(before);
  } finally {
    await page.request.put("/api/settings", { data: { "graph.clusters": 5 } });
  }
});
