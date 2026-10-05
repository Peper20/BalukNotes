// The graph page: zoom, pan, moving a node, filters, search, the neighbours of a note.
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

test("graph: the icon at the bottom of the sidebar opens the vault graph", async ({ page }) => {
  await open(page, "демо/компоненты");
  await page.locator("#open-graph").click();
  await expect(page).toHaveURL(vaultUrl("/graph"));
  await expect(page.locator(".graph-node")).not.toHaveCount(0);
});

test("graph: buttons and the wheel zoom, a node moves, a click opens the note, Ctrl+click - in the background", async ({ page }) => {
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

  // dragging a node moves it, the note does not open
  const ssh = page.locator('.graph-node[data-id="Сеть/SSH"] circle');
  const before = await nodeAt(page, "Сеть/SSH");
  const c = (await ssh.boundingBox())!;
  await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
  await page.mouse.down();
  await page.mouse.move(c.x + 60, c.y + 40, { steps: 5 });
  await page.mouse.up();
  expect(await nodeAt(page, "Сеть/SSH")).not.toBe(before);
  await expect(page).toHaveURL(/\/graph$/);

  // Ctrl+click: the note in a background tab, the graph stays
  await ssh.click({ modifiers: ["Control"] });
  await expect(page.locator(".tabbar .tab")).toHaveCount(2);
  await expect(page).toHaveURL(/\/graph$/);

  await ssh.click();
  await ready(page);
  await expect(title(page)).toHaveText("SSH");
});

test("graph: folders, tag, search; filters are remembered", async ({ page }) => {
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

test("graph: books as chapters - the root and chapters, a click on a chapter opens the book at it; the switch is remembered", async ({ page }) => {
  await page.goto(vaultUrl("/graph"));
  await ready(page);
  await expect(page.locator('.graph-node[data-id="Книга/.2"]')).toHaveCount(0);
  await page.getByLabel("книги главами").check();
  const chapters = page.locator('.graph-node.chapter[data-id^="Книга/"]');
  await expect(chapters).toHaveCount(3);
  // The link "book - chapter" for every chapter (one more book of the fixture is English).
  await expect(page.locator(".graph-edge.chapter")).toHaveCount(await page.locator(".graph-node.chapter").count());
  await expect(page.locator('.graph-node.book[data-id="Книга"]')).toHaveCount(1);
  await page.reload();
  await ready(page);
  await expect(chapters).toHaveCount(3);
  await page.locator('.graph-node[data-id="Книга/.2"] circle').click();
  await ready(page);
  await expect.poll(() => decodeURIComponent(new URL(page.url()).pathname + new URL(page.url()).hash)).toBe("/v/vault/n/Книга#Продолжение");
});

test("graph: the neighbours of a note from \"Ссылаются сюда\", the depth in the address", async ({ page }) => {
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
  // back to the note by the link in the heading of the neighbours page
  await page.goBack();
  await ready(page);
  await page.locator(".graph-title").getByRole("link", { name: "SSH" }).click();
  await ready(page);
  await expect(page).toHaveURL(noteUrl("Сеть/SSH"));
});

test("the vault graph in a note: live over the picture, a click opens the note", async ({ page }) => {
  await open(page, "Рисунки/Граф хранилища");
  const graph = page.locator(".k-graph").first();
  await expect(graph).toHaveAttribute("data-live", "");
  await expect(graph.locator(".k-frame")).toBeHidden();
  await expect(graph.locator(".graph-node.center")).toHaveAttribute("data-id", "Сеть/SSH");
  await graph.locator('.graph-node[data-id="Сеть/UFW"]').click();
  await expect(page).toHaveURL(noteUrl("Сеть/UFW"));
});

test("the graph in a note: a node pulls its neighbours, does not leave the figure frame and does not select the text behind it", async ({ page }) => {
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
  await expect.poll(() => at(neighbour)).not.toBe(before); // the neighbour followed
  await inside();
  expect(await page.evaluate(() => getSelection()?.toString() ?? "")).toBe(""); // the pointer is outside the frame
  await page.mouse.up();
  await page.waitForTimeout(1500); // settled
  await inside();
  await expect(page).toHaveURL(noteUrl("Рисунки/Граф хранилища")); // a drag is not a click
});

test.describe("without motion (prefers-reduced-motion)", () => {
  test.use({ reducedMotion: "reduce" });

  test("the graph does not animate, a dragged node does not pull its neighbours", async ({ page }) => {
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
    // a filter change applies at once, without fading: the gone node vanished together with the new count
    const count = await page.locator(".graph-count").textContent();
    await page.locator(".graph-legend button", { hasText: "Сеть" }).click();
    await expect(page.locator(".graph-count")).not.toHaveText(count!);
    expect(await page.locator('.graph-node[data-id="Сеть/SSH"]').count()).toBe(0);
  });
});

test("graph: \"вернуть раскладку\" - nodes return to their places smoothly, without overshoot", async ({ page }) => {
  await page.goto(vaultUrl("/graph"));
  await ready(page);
  const restore = page.getByRole("button", { name: "вернуть раскладку" });
  await expect(restore).toBeDisabled();
  const xy = (id: string) =>
    page.locator(`.graph-node[data-id="${id}"]`).evaluate((g) => (g.getAttribute("transform") ?? "").match(/[-\d.]+/g)!.map(Number));
  const ids = await page.locator(".graph-node").evaluateAll((gs) => gs.map((g) => (g as SVGElement).dataset.id!));
  const home = new Map(await Promise.all(ids.map(async (id) => [id, await xy(id)] as const)));

  // Dragging a node: the neighbours follow it, after the release they return only a quarter of the way.
  const ssh = page.locator('.graph-node[data-id="Сеть/SSH"] circle');
  const c = (await ssh.boundingBox())!;
  await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
  await page.mouse.down();
  await page.mouse.move(c.x + 160, c.y + 90, { steps: 10 });
  await page.mouse.up();
  await expect(restore).toBeEnabled();
  await page.waitForTimeout(1500); // the physics settled
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

  // Restore: the distance of the SSH node to its home only decreases, at the end it is exactly home.
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

test("graph forces: a sliding side panel, \"Папки\" changes the layout, the value is remembered, \"По умолчанию\" brings it back", async ({ page }) => {
  try {
    await page.goto(vaultUrl("/graph"));
    await ready(page);
    const before = await nodeAt(page, "Сеть/SSH");
    const panel = page.locator(".graph-forces");
    const toggle = page.getByRole("button", { name: "силы графа", exact: true });
    await expect(panel).toBeHidden(); // hidden by default
    await toggle.click();
    await expect(panel).toBeVisible();
    await page.getByRole("button", { name: "спрятать силы графа" }).click();
    await expect(panel).toBeHidden();
    await toggle.click();
    await expect(panel).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    await toggle.click();
    await expect.poll(async () => (await panel.boundingBox())?.width).toBeGreaterThan(200); // slid out entirely
    // Beside, not over the graph: the canvas shrinks while the panel slides out - wait for the end.
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
    await expect(panel).toBeHidden(); // hidden again after a reload
    await toggle.click();
    await expect(clusters).toHaveValue("300");
    await page.getByRole("button", { name: "По умолчанию" }).click();
    await expect(clusters).toHaveValue("5");
    await expect.poll(() => nodeAt(page, "Сеть/SSH")).toBe(before);
  } finally {
    await page.request.put("/api/settings", { data: { "graph.clusters": 5 } });
  }
});
