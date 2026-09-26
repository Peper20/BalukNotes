import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { polling } from "./changes";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

it("опрос: раз в N секунд, кроме скрытой вкладки; остановка", () => {
  let hidden = false;
  const seen = vi.fn();
  const stop = polling(5, () => hidden).start(seen);
  vi.advanceTimersByTime(10_000);
  expect(seen).toHaveBeenCalledTimes(2);
  hidden = true;
  vi.advanceTimersByTime(5_000);
  expect(seen).toHaveBeenCalledTimes(2);
  stop();
  hidden = false;
  vi.advanceTimersByTime(20_000);
  expect(seen).toHaveBeenCalledTimes(2);
});

it("опрос выключен при 0", () => {
  const seen = vi.fn();
  polling(0).start(seen);
  vi.advanceTimersByTime(60_000);
  expect(seen).not.toHaveBeenCalled();
});
