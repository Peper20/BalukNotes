<!--
  Живая поверхность z = f(x, y): canvas, грани со светотенью по алгоритму
  художника (как кадр Typst), вращение перетаскиванием (мышь, палец) и
  стрелками, двойной щелчок — начальный вид. Цвета — переменные темы:
  смена темы перерисовывает.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { compile } from "../lib/plot/formula";
  import { faces, mixHex, turn, view, Z, type P3 } from "../lib/plot/geometry";
  import { colorVar, defaults, PT_PER_CM, SMALL_PT, TEXT_PT, type Plot3D } from "../lib/plot/spec";
  import PlotSliders from "./PlotSliders.svelte";

  // Описание рисунка задано раз при монтировании (NoteView) и не меняется.
  let { spec: initial }: { spec: Plot3D } = $props();
  const spec = untrack(() => initial);

  const f = compile(spec.f, ["x", "y", ...spec.params.map((p) => p.name)]);
  let values = $state(defaults(spec.params));
  const deg = Math.PI / 180;
  let th = $state(spec.view[0] * deg);
  let ph = $state(spec.view[1] * deg);
  // Мир холста: коробка с запасом под подписи, в единицах коробки.
  const HALF_W = 1.6;
  const HALF_H = 1.5;
  // Размер как у кадра Typst: единица коробки = size / 2 см.
  const widthEm = (2 * HALF_W * (spec.size / 2) * PT_PER_CM) / TEXT_PT;

  let canvas: HTMLCanvasElement | undefined = $state();
  let cssWidth = $state(0);
  let theme = $state(0);

  const [x0, x1] = spec.x;
  const [y0, y1] = spec.y;
  const [z0, z1] = spec.z;
  const grid = $derived.by(() => {
    const n = spec.n;
    const v = values;
    return Array.from({ length: n + 1 }, (_, i) =>
      Array.from({ length: n + 1 }, (_, j) => f({ ...v, x: x0 + ((x1 - x0) * i) / n, y: y0 + ((y1 - y0) * j) / n })),
    );
  });

  $effect(() => draw(grid, th, ph, cssWidth, theme));

  function draw(grid: (number | null)[][], th: number, ph: number, w: number, _theme: number) {
    const cv = canvas;
    const ctx = cv?.getContext("2d");
    if (!cv || !ctx || w === 0) return;
    const dpr = devicePixelRatio || 1;
    const h = (w * HALF_H) / HALF_W;
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
    const k = w / (2 * HALF_W); // px на единицу коробки
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const css = getComputedStyle(cv);
    const get = (name: string) => css.getPropertyValue(name).trim() || "#888888";
    const axis = get("--k-fig-axis");
    const base = colorVar(spec.color);
    const dark = get(base + "-dark");
    const light = get(base + "-light");
    const edge = get("--k-fig-line");
    const S = ([u, v]: [number, number] | P3): [number, number] => [w / 2 + u * k, h / 2 - v * k];
    const at = (p: P3) => S(view(p, th, ph));

    // пол коробки и вертикальное ребро в дальнем углу
    const corners: [number, number][] = [[-1, -1], [1, -1], [1, 1], [-1, 1]];
    const far = [...corners].sort((p, q) => view([...p, 0], th, ph)[2] - view([...q, 0], th, ph)[2])[0]!;
    ctx.strokeStyle = axis;
    ctx.lineWidth = 0.6;
    ctx.beginPath();
    corners.forEach((c, i) => (i ? ctx.lineTo(...at([...c, -Z])) : ctx.moveTo(...at([...c, -Z]))));
    ctx.closePath();
    ctx.moveTo(...at([...far, -Z]));
    ctx.lineTo(...at([...far, Z]));
    ctx.stroke();

    // грани
    ctx.lineJoin = "round";
    for (const face of faces(grid, z0, z1, th, ph)) {
      ctx.beginPath();
      face.pts.forEach((p, i) => (i ? ctx.lineTo(...S(p)) : ctx.moveTo(...S(p))));
      ctx.closePath();
      if (spec.style === "wire") {
        ctx.strokeStyle = get(base);
        ctx.lineWidth = 0.6;
      } else {
        ctx.fillStyle = mixHex(dark, light, face.light);
        ctx.fill();
        ctx.strokeStyle = mixHex(edge, edge, 0).replace(/[\d.]+\)$/, "0.4)");
        ctx.lineWidth = 0.4;
      }
      ctx.stroke();
    }

    // подписи осей — поверх: x и y у ближних рёбер, z над дальним углом
    const nearer = (p: P3, q: P3) => (view(p, th, ph)[2] > view(q, th, ph)[2] ? p : q);
    const fontPx = (SMALL_PT / TEXT_PT) * parseFloat(css.fontSize);
    ctx.font = `italic ${fontPx}px ${css.getPropertyValue("--k-font-text")}`;
    ctx.fillStyle = axis;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(spec.labels[0], ...at(nearer([0, -1.22, -Z], [0, 1.22, -Z])));
    ctx.fillText(spec.labels[1], ...at(nearer([-1.22, 0, -Z], [1.22, 0, -Z])));
    ctx.fillText(spec.labels[2], ...at([...far, Z + 0.14]));
  }

  // Вращение: перетаскивание по горизонтали — поворот, по вертикали — наклон;
  // ближняя сторона едет за указателем (`turn`). Палец, начавший вертикально,
  // прокручивает страницу (touch-action: pan-y — браузер пришлёт pointercancel):
  // до TOUCH_SLOP пикселей палец не вращает — ждём, куда он пошёл.
  const TOUCH_SLOP = 8;
  let drag: { id: number; x: number; y: number; held: boolean } | null = null;
  function down(e: PointerEvent) {
    drag = { id: e.pointerId, x: e.clientX, y: e.clientY, held: e.pointerType === "touch" };
    canvas?.setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag || drag.id !== e.pointerId) return;
    const [dx, dy] = [e.clientX - drag.x, e.clientY - drag.y];
    if (drag.held) {
      if (Math.hypot(dx, dy) < TOUCH_SLOP) return;
      // Вертикально — это прокрутка страницы, не рисунка.
      if (Math.abs(dy) > Math.abs(dx)) return void (drag = null);
      drag.held = false;
    }
    rotate(dx * 0.012, dy * 0.012);
    drag = { ...drag, x: e.clientX, y: e.clientY };
  }
  function rotate(du: number, dv: number) {
    [th, ph] = turn(th, ph, du, dv);
  }
  function key(e: KeyboardEvent) {
    const step = 5 * deg;
    const d = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
    if (!d) return;
    e.preventDefault();
    rotate(d[0]!, d[1]!);
  }
  function reset() {
    th = spec.view[0] * deg;
    ph = spec.view[1] * deg;
  }

  // Смена темы — те же переменные, другие значения; догрузился шрифт
  // подписей — другие буквы: перерисовать.
  onMount(() => {
    const again = () => theme++;
    const obs = new MutationObserver(again);
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    document.fonts.addEventListener("loadingdone", again);
    void document.fonts.ready.then(again);
    return () => {
      obs.disconnect();
      document.fonts.removeEventListener("loadingdone", again);
    };
  });
</script>

<div class="k-plot-live">
  <canvas
    bind:this={canvas}
    bind:clientWidth={cssWidth}
    class="k-plot-canvas"
    style:width="min(100%, {widthEm}em)"
    style:aspect-ratio="{HALF_W} / {HALF_H}"
    tabindex="0"
    aria-label="поверхность: перетащите, чтобы повернуть"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={() => (drag = null)}
    onpointercancel={() => (drag = null)}
    ondblclick={reset}
    onkeydown={key}
  ></canvas>
  <div class="k-plot-readout"><span class="k-plot-hint">перетащите, чтобы повернуть; двойной щелчок — исходный вид</span></div>
  <PlotSliders params={spec.params} bind:values />
</div>
