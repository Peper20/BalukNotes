// Граф заметок: узлы — заметки и книги, рёбра — ссылки #см (из /api/graph).
//
// Раскладка — простая силовая модель без библиотек: все узлы отталкиваются,
// рёбра — пружины, слабое притяжение к центру. Узлов — десятки–сотни,
// O(n²) на шаг — миллисекунды; считаем заранее и рисуем уже готовый граф.
// Цвет узла — по папке верхнего уровня, цвета — переменные темы, поэтому
// граф перекрашивается вместе с темой. Временный клиент M1; в M3 — граф во
// весь экран с перетаскиванием и фильтрами.

const SVG = "http://www.w3.org/2000/svg";
const PALETTE = ["--k-accent", "--k-box-example", "--k-second", "--k-box-idea", "--k-box-algo", "--k-box-pitfall", "--k-box-thm"];

const el = (tag, attrs = {}) => {
  const e = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  return e;
};

/** Координаты узлов: {id → [x, y]}. Детерминированно: одинаковый граф — одинаковая картинка. */
function layout(nodes, edges) {
  const n = nodes.length;
  const index = new Map(nodes.map((node, i) => [node.id, i]));
  const pos = nodes.map((_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [Math.cos(a) * 100, Math.sin(a) * 100];
  });
  const links = edges.map((e) => [index.get(e.from), index.get(e.to)]).filter(([a, b]) => a != null && b != null);
  const k = 70; // желаемая длина ребра
  for (let step = 0, t = 30; step < 400; step++, t *= 0.985) {
    const force = pos.map(() => [0, 0]);
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const dx = pos[i][0] - pos[j][0], dy = pos[i][1] - pos[j][1];
        const d2 = Math.max(dx * dx + dy * dy, 1);
        const f = (k * k) / d2;
        force[i][0] += dx * f; force[i][1] += dy * f;
        force[j][0] -= dx * f; force[j][1] -= dy * f;
      }
    }
    for (const [a, b] of links) {
      const dx = pos[a][0] - pos[b][0], dy = pos[a][1] - pos[b][1];
      const d = Math.max(Math.hypot(dx, dy), 1);
      const f = (d - k) / d / 2;
      force[a][0] -= dx * f; force[a][1] -= dy * f;
      force[b][0] += dx * f; force[b][1] += dy * f;
    }
    for (let i = 0; i < n; i++) {
      force[i][0] -= pos[i][0] * 0.02;
      force[i][1] -= pos[i][1] * 0.02;
      const len = Math.hypot(...force[i]);
      if (len > 0) {
        const m = Math.min(len, t) / len;
        pos[i][0] += force[i][0] * m;
        pos[i][1] += force[i][1] * m;
      }
    }
  }
  return new Map(nodes.map((node, i) => [node.id, pos[i]]));
}

/**
 * Рисует граф в `container`. `onOpen(id)` — клик по заметке.
 * Наведение подсвечивает узел и его соседей.
 */
export function renderGraph(container, graph, { onOpen }) {
  const { nodes, edges } = graph;
  if (!nodes.length) return;
  const pos = layout(nodes, edges);
  const degree = new Map(nodes.map((n) => [n.id, 0]));
  for (const e of edges) {
    degree.set(e.from, (degree.get(e.from) ?? 0) + 1);
    degree.set(e.to, (degree.get(e.to) ?? 0) + 1);
  }
  const folders = [...new Set(nodes.map((n) => n.id.split("/")[0]))];
  const color = (id) => `var(${PALETTE[folders.indexOf(id.split("/")[0]) % PALETTE.length]})`;
  const radius = (n) => (n.kind === "book" ? 11 : 5.5) + Math.min(degree.get(n.id), 8) * 0.7;

  const xs = [...pos.values()].map((p) => p[0]), ys = [...pos.values()].map((p) => p[1]);
  // По горизонтали — место под подписи (до ~20 знаков), по вертикали — под нижнюю подпись.
  const [padX, padY] = [120, 40];
  const [x0, y0] = [Math.min(...xs) - padX, Math.min(...ys) - padY];
  const [w, h] = [Math.max(...xs) - x0 + padX, Math.max(...ys) - y0 + padY + 15];
  const svg = el("svg", { viewBox: `${x0} ${y0} ${w} ${h}`, class: "graph-svg", role: "img", "aria-label": "Граф заметок" });

  const neighbours = new Map(nodes.map((n) => [n.id, new Set([n.id])]));
  const lines = edges.map((e) => {
    neighbours.get(e.from)?.add(e.to);
    neighbours.get(e.to)?.add(e.from);
    const [a, b] = [pos.get(e.from), pos.get(e.to)];
    const line = el("line", { x1: a[0], y1: a[1], x2: b[0], y2: b[1], class: "graph-edge", "stroke-width": Math.min(1 + e.count * 0.4, 3) });
    line.dataset.from = e.from;
    line.dataset.to = e.to;
    svg.append(line);
    return line;
  });

  const groups = nodes.map((n) => {
    const [x, y] = pos.get(n.id);
    const g = el("g", { class: `graph-node${n.kind ? "" : " missing"}${n.kind === "book" ? " book" : ""}`, transform: `translate(${x} ${y})` });
    g.dataset.id = n.id;
    const c = el("circle", { r: radius(n) });
    c.style.fill = n.kind ? color(n.id) : "none";
    c.style.stroke = color(n.id);
    const label = el("text", { y: radius(n) + 13, "text-anchor": "middle" });
    label.textContent = n.id.split("/").at(-1);
    const title = el("title");
    title.textContent = n.kind ? `${n.id}${n.kind === "book" ? " (книга)" : ""} · связей: ${degree.get(n.id)}` : `${n.id} — такой заметки нет`;
    g.append(c, label, title);
    if (n.kind) g.addEventListener("click", () => onOpen(n.id));
    g.addEventListener("pointerenter", () => focus(n.id));
    g.addEventListener("pointerleave", () => focus(null));
    svg.append(g);
    return g;
  });

  function focus(id) {
    const near = id ? neighbours.get(id) : null;
    svg.classList.toggle("focused", Boolean(id));
    for (const g of groups) g.classList.toggle("near", Boolean(near?.has(g.dataset.id)));
    for (const l of lines) l.classList.toggle("near", Boolean(id) && (l.dataset.from === id || l.dataset.to === id));
  }

  const legend = document.createElement("div");
  legend.className = "graph-legend";
  for (const f of folders) {
    const item = document.createElement("span");
    const dot = document.createElement("i");
    dot.style.background = color(f);
    item.append(dot, f);
    legend.append(item);
  }
  container.replaceChildren(svg, legend);
}
