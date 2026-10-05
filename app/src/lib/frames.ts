// Frames (`baluk/frames.typ`): the description from `data-k-frames` and transitions.

export interface FramesSpec {
  /** Parameter name, the slider label. */
  name: string;
  count: number;
  /** Default frame number (from zero). */
  default: number;
  /** Frames per second when playing. */
  fps: number;
  /** Play in a loop. */
  loop: boolean;
}

/** The frame description from JSON; unclear - null (the default frame stays). fps limits as in `frames.typ`. */
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

/** A step by the ‹ › buttons: in a circle if `loop`, otherwise stops at the edge. */
export function stepFrame(i: number, delta: number, spec: FramesSpec): number {
  const j = i + delta;
  if (spec.loop) return ((j % spec.count) + spec.count) % spec.count;
  return Math.min(spec.count - 1, Math.max(0, j));
}

/** The next frame when playing; null - the end (without `loop`). */
export function nextPlaying(i: number, spec: FramesSpec): number | null {
  if (i + 1 < spec.count) return i + 1;
  return spec.loop ? 0 : null;
}

/** Which frame "▶" starts from: from the last one without `loop`, from the start. */
export function playStart(i: number, spec: FramesSpec): number {
  return !spec.loop && i >= spec.count - 1 ? 0 : i;
}
