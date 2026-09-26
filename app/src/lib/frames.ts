// Кадры (`baluk/frames.typ`): описание из `data-k-frames` и переходы.

export interface FramesSpec {
  /** Имя параметра — подпись ползунка. */
  name: string;
  count: number;
  /** Номер кадра по умолчанию (с нуля). */
  default: number;
  /** Кадров в секунду при проигрывании. */
  fps: number;
  /** Проигрывать по кругу. */
  loop: boolean;
}

/** Описание кадров из JSON; непонятное — null (остаётся кадр по умолчанию). Пределы fps — как в `frames.typ`. */
export function parseFrames(json: string | undefined): FramesSpec | null {
  if (!json) return null;
  try {
    const s = JSON.parse(json) as Partial<FramesSpec>;
    const count = Number(s.count);
    const def = Number(s.default);
    const fps = Number(s.fps);
    if (!Number.isInteger(count) || count < 1 || !Number.isInteger(def) || def < 0 || def >= count) return null;
    return {
      name: typeof s.name === "string" ? s.name : "",
      count,
      default: def,
      fps: fps > 0 && fps <= 60 ? fps : 2,
      loop: s.loop === true,
    };
  } catch {
    return null;
  }
}

/** Шаг кнопками ‹ ›: по кругу, если `loop`, иначе упор в край. */
export function stepFrame(i: number, delta: number, spec: FramesSpec): number {
  const j = i + delta;
  if (spec.loop) return ((j % spec.count) + spec.count) % spec.count;
  return Math.min(spec.count - 1, Math.max(0, j));
}

/** Следующий кадр при проигрывании; null — конец (без `loop`). */
export function nextPlaying(i: number, spec: FramesSpec): number | null {
  if (i + 1 < spec.count) return i + 1;
  return spec.loop ? 0 : null;
}

/** С какого кадра начинать «▶»: с последнего без `loop` — сначала. */
export function playStart(i: number, spec: FramesSpec): number {
  return !spec.loop && i >= spec.count - 1 ? 0 : i;
}
