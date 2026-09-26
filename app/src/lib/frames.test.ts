import { describe, expect, it } from "vitest";
import { nextPlaying, parseFrames, playStart, stepFrame, type FramesSpec } from "./frames";

const spec = (over: Partial<FramesSpec> = {}): FramesSpec => ({ name: "n", count: 4, default: 1, fps: 2, loop: false, ...over });

describe("кадры", () => {
  it("описание из JSON Typst", () => {
    expect(parseFrames('{"name":"n","count":12,"default":3,"fps":2,"loop":false}')).toEqual(spec({ count: 12, default: 3 }));
    expect(parseFrames('{"name":"t","count":9,"default":2,"fps":4,"loop":true}')?.loop).toBe(true);
  });

  it("непонятное — null, странная скорость — по умолчанию", () => {
    expect(parseFrames(undefined)).toBeNull();
    expect(parseFrames("{")).toBeNull();
    expect(parseFrames('{"count":3,"default":3}')).toBeNull();
    expect(parseFrames('{"count":0,"default":0}')).toBeNull();
    expect(parseFrames('{"count":2,"default":0,"fps":-1}')?.fps).toBe(2);
  });

  it("шаг кнопками: упор в край или по кругу", () => {
    expect(stepFrame(0, -1, spec())).toBe(0);
    expect(stepFrame(3, 1, spec())).toBe(3);
    expect(stepFrame(3, 1, spec({ loop: true }))).toBe(0);
    expect(stepFrame(0, -1, spec({ loop: true }))).toBe(3);
  });

  it("проигрывание: до конца или по кругу; с конца — сначала", () => {
    expect(nextPlaying(2, spec())).toBe(3);
    expect(nextPlaying(3, spec())).toBeNull();
    expect(nextPlaying(3, spec({ loop: true }))).toBe(0);
    expect(playStart(3, spec())).toBe(0);
    expect(playStart(2, spec())).toBe(2);
    expect(playStart(3, spec({ loop: true }))).toBe(3);
  });
});
