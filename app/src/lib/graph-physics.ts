// Живой граф: физика, пока с графом что-то делают. Раскладка ядра
// (`notes-core::vault_graph`) — состояние покоя: каждый узел держится за
// своё место («дом») слабой пружиной, рёбра — пружины длиной как в
// раскладке, наехавшие прямоугольники узлов с подписями раздвигаются (как
// в ядре). В покое все силы — ноль: нетронутый граф не шевелится, PDF,
// сайт и приложение показывают одну картинку.
//
// Протянули узел — соседи тянутся за ним по рёбрам, их соседи — слабее;
// затухание сильное — без раскачки. Отпустили — узел остаётся, где его
// бросили, а соседи плавно проходят немного назад (`RETURN` пути к прежним
// местам) и встают: полный возврат пружинами «встряхивал» граф, а совсем
// без движения граф замирал мёртво. Рамка (`frame`) — узлы вместе с
// подписью не выходят за неё.

export type Point = [number, number];
/** Прямоугольник `[x0, y0, x1, y1]`. */
export type Rect = [number, number, number, number];

/** Размер узла вокруг его центра: полуширина (кружок или подпись), верх и низ (с подписью). */
export interface Extent {
  half: number;
  top: number;
  bottom: number;
}

export interface PhysicsNode {
  x: number;
  y: number;
  /** Размер для раздвигания — как в ядре (подпись кеглем раскладки), иначе покой не был бы покоем. */
  extent: Extent;
}

/** Жёсткость пружины к дому и рёбер, затухание скорости за шаг. */
const HOME = 0.05;
const SPRING = 0.07;
const DAMPING = 0.3;
/** Какую долю пути к прежнему месту соседи проходят после отпускания. */
const RETURN = 0.25;
/** Сдвиг (единиц раскладки за шаг), ниже которого граф считается осевшим. */
const REST = 0.02;

export class Physics {
  readonly n: number;
  readonly pos: Float64Array;
  private readonly vel: Float64Array;
  private readonly home: Float64Array;
  private extents: Extent[];
  private readonly links: [number, number, number][];
  private readonly lo: Float64Array;
  private readonly hi: Float64Array;
  private frame: Rect | null = null;
  private frameExtents: Extent[];
  /** Узел под указателем. */
  private held = -1;
  /** Брошенный узел: стоит, пока соседи оседают (иначе их пружины его качнут). */
  private dropped = -1;

  constructor(nodes: PhysicsNode[], edges: [number, number][], frame: Rect | null = null) {
    this.n = nodes.length;
    this.pos = new Float64Array(nodes.flatMap((p) => [p.x, p.y]));
    this.home = this.pos.slice();
    this.vel = new Float64Array(2 * this.n);
    this.extents = nodes.map((p) => p.extent);
    this.frameExtents = this.extents;
    this.links = edges.filter(([a, b]) => a !== b).map(([a, b]) => [a, b, this.span(a, b)]);
    this.lo = new Float64Array(2 * this.n);
    this.hi = new Float64Array(2 * this.n);
    this.setFrame(frame);
  }

  /**
   * Размеры для раздвигания — по подписям на экране (они бывают крупнее,
   * чем считало ядро), но ужатые ровно настолько, чтобы дома не наезжали:
   * покой остаётся покоем. Ужимаются только узлы, которые наезжают дома.
   */
  setExtents(extents: Extent[]) {
    const s = new Array<number>(this.n).fill(1);
    for (let i = 0; i < this.n; i++) {
      const ei = extents[i]!;
      for (let j = i + 1; j < this.n; j++) {
        const ej = extents[j]!;
        const dx = Math.abs(this.home[2 * j]! - this.home[2 * i]!);
        const dy = this.home[2 * j + 1]! - this.home[2 * i + 1]!;
        const w = ei.half + ej.half;
        const h = dy >= 0 ? ei.bottom + ej.top : ei.top + ej.bottom;
        if (w <= dx || h <= Math.abs(dy)) continue;
        const f = Math.max(dx / w, Math.abs(dy) / h);
        s[i] = Math.min(s[i]!, f);
        s[j] = Math.min(s[j]!, f);
      }
    }
    this.extents = extents.map((e, i) => ({ half: e.half * s[i]!, top: e.top * s[i]!, bottom: e.bottom * s[i]! }));
  }

  /** Расстояние между домами узлов. */
  private span(a: number, b: number): number {
    return Math.hypot(this.home[2 * a]! - this.home[2 * b]!, this.home[2 * a + 1]! - this.home[2 * b + 1]!);
  }

  /**
   * Рамка, за которую узлы (с подписью размером `extents`, по умолчанию —
   * как при раздвигании) не выходят; `null` — без рамки. Узел, чей дом
   * уже за рамкой (подпись на экране крупнее, чем считало ядро), может
   * стоять дома — рамка для него расширяется до дома.
   */
  setFrame(frame: Rect | null, extents: Extent[] = this.extents) {
    this.frame = frame;
    this.frameExtents = extents;
    for (let i = 0; i < this.n; i++) {
      if (!frame) {
        this.lo.fill(-Infinity, 2 * i, 2 * i + 2);
        this.hi.fill(Infinity, 2 * i, 2 * i + 2);
        continue;
      }
      const e = extents[i]!;
      const [hx, hy] = [this.home[2 * i]!, this.home[2 * i + 1]!];
      this.lo[2 * i] = Math.min(frame[0] + e.half, hx);
      this.hi[2 * i] = Math.max(frame[2] - e.half, hx);
      this.lo[2 * i + 1] = Math.min(frame[1] + e.top, hy);
      this.hi[2 * i + 1] = Math.max(frame[3] - e.bottom, hy);
    }
  }

  at(i: number): Point {
    return [this.pos[2 * i]!, this.pos[2 * i + 1]!];
  }

  /** Держать узел `i` в точке (с учётом рамки). */
  drag(i: number, x: number, y: number) {
    this.held = i;
    this.dropped = -1;
    this.pos[2 * i] = clamp(x, this.lo[2 * i]!, this.hi[2 * i]!);
    this.pos[2 * i + 1] = clamp(y, this.lo[2 * i + 1]!, this.hi[2 * i + 1]!);
    this.vel[2 * i] = this.vel[2 * i + 1] = 0;
  }

  /** Отпустить узел: он остаётся, остальные немного отходят к прежним местам (новый покой). */
  release() {
    const i = this.held;
    if (i < 0) return;
    this.held = -1;
    this.dropped = i;
    for (let k = 0; k < 2 * this.n; k++) {
      const back = k >> 1 === i ? 0 : RETURN;
      this.home[k] = this.pos[k]! + (this.home[k]! - this.pos[k]!) * back;
    }
    for (const l of this.links) l[2] = this.span(l[0], l[1]);
    this.setFrame(this.frame, this.frameExtents);
  }

  get holding(): boolean {
    return this.held >= 0;
  }

  private fixed(i: number): boolean {
    return i === this.held || i === this.dropped;
  }


  /** Один шаг; возвращает наибольший сдвиг узла за шаг — для [`settled`]. */
  step(): number {
    const { n, pos, vel, home, extents } = this;
    const before = pos.slice();
    const force = new Float64Array(2 * n);
    for (let i = 0; i < 2 * n; i++) force[i] = (home[i]! - pos[i]!) * HOME;
    for (const [a, b, len] of this.links) {
      const dx = pos[2 * b]! - pos[2 * a]!;
      const dy = pos[2 * b + 1]! - pos[2 * a + 1]!;
      const d = Math.max(Math.hypot(dx, dy), 1e-6);
      const f = ((d - len) / d) * SPRING;
      force[2 * a]! += dx * f;
      force[2 * a + 1]! += dy * f;
      force[2 * b]! -= dx * f;
      force[2 * b + 1]! -= dy * f;
    }
    for (let i = 0; i < n; i++) {
      if (this.fixed(i)) continue;
      for (const k of [2 * i, 2 * i + 1]) {
        vel[k] = (vel[k]! + force[k]!) * DAMPING;
        pos[k]! += vel[k]!;
      }
    }
    // Наехавшие прямоугольники — врозь по оси с меньшим перекрытием, сразу
    // (не силой: иначе пружины вдавливают узлы друг в друга).
    for (let i = 0; i < n; i++) {
      const ei = extents[i]!;
      for (let j = i + 1; j < n; j++) {
        const ej = extents[j]!;
        const dx = pos[2 * j]! - pos[2 * i]!;
        const dy = pos[2 * j + 1]! - pos[2 * i + 1]!;
        const ox = ei.half + ej.half - Math.abs(dx);
        if (ox <= 0) continue;
        const oy = (dy >= 0 ? ei.bottom + ej.top : ei.top + ej.bottom) - Math.abs(dy);
        if (oy <= 0) continue;
        const [fi, fj] = [this.fixed(i), this.fixed(j)];
        // По оси с меньшим перекрытием; упёрлись там в рамку — по другой.
        const axes: [number, number, number][] = [
          [0, dx, ox],
          [1, dy, oy],
        ];
        if (oy < ox) axes.reverse();
        for (const [k, d, o] of axes) {
          const dir = d < 0 ? -1 : 1;
          const [si, sj] = fi ? [0, o] : fj ? [o, 0] : [o / 2, o / 2];
          const mi = this.room(i, k, -dir * si);
          const mj = this.room(j, k, dir * sj);
          if (Math.abs(mi) + Math.abs(mj) < o / 2 && k === axes[0]![0]) continue;
          pos[2 * i + k]! += mi;
          pos[2 * j + k]! += mj;
          break;
        }
      }
    }
    let fastest = 0;
    for (let i = 0; i < n; i++) {
      if (this.fixed(i)) continue;
      for (const k of [2 * i, 2 * i + 1]) {
        pos[k] = clamp(pos[k]!, this.lo[k]!, this.hi[k]!);
        vel[k] = pos[k]! - before[k]!;
        fastest = Math.max(fastest, Math.abs(vel[k]!));
      }
    }
    return fastest;
  }

  /** Сколько узел `i` может сдвинуться по оси `k` на `delta` (держимый — нисколько, рамка — до края). */
  private room(i: number, k: number, delta: number): number {
    if (this.fixed(i) || delta === 0) return 0;
    const at = this.pos[2 * i + k]!;
    return clamp(at + delta, this.lo[2 * i + k]!, this.hi[2 * i + k]!) - at;
  }

  /** Осел: никого не держат и все почти стоят. */
  settled(fastest: number): boolean {
    const done = this.held < 0 && fastest < REST;
    if (done) this.dropped = -1;
    return done;
  }
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Ширина подписи — по числу знаков, как в ядре (`vault_graph::label_width`). */
export const labelWidth = (text: string, size: number) => [...text].length * size * 0.58;

/** Размер узла с подписью под кружком (`vault_graph::box_rect`). */
export function extentOf(r: number, name: string, size: number, gap: number): Extent {
  return { half: Math.max(r, labelWidth(name, size) / 2), top: r, bottom: r + gap + size * 1.2 };
}
