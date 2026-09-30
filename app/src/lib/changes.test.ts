import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { changeSource, serverEvents, type EventStream } from "./changes";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** Подделка EventSource: события — вручную. */
class FakeStream implements EventStream {
  listeners = new Map<string, (e: MessageEvent<string>) => void>();
  onerror: ((e: Event) => void) | null = null;
  closed = false;
  addEventListener(type: string, fn: (e: MessageEvent<string>) => void) {
    this.listeners.set(type, fn);
  }
  emit(type: string, data = "{}") {
    this.listeners.get(type)?.(new MessageEvent(type, { data }));
  }
  close() {
    this.closed = true;
  }
}

it("события сервера: change — проверить; разрыв — нет связи, возврат — одна проверка", () => {
  const stream = new FakeStream();
  const seen = vi.fn();
  const reach = vi.fn();
  const stop = serverEvents("/api/events", reach, () => stream).start(seen);
  stream.emit("hello", '{"watching":true}');
  expect(reach).toHaveBeenLastCalledWith(true);
  stream.emit("change", '{"seq":1,"paths":["a.typ"]}');
  expect(seen).toHaveBeenCalledTimes(1);

  stream.onerror!(new Event("error"));
  expect(reach).toHaveBeenLastCalledWith(false);
  // Опроса нет: пока связи нет, проверок нет.
  vi.advanceTimersByTime(60_000);
  expect(seen).toHaveBeenCalledTimes(1);
  stream.emit("hello", '{"watching":true}');
  expect(reach).toHaveBeenLastCalledWith(true);
  expect(seen).toHaveBeenCalledTimes(2);

  stop();
  expect(stream.closed).toBe(true);
});

it("сервер не следит за файлами — проверок нет, только по кнопке", () => {
  const stream = new FakeStream();
  const seen = vi.fn();
  serverEvents("/api/events", undefined, () => stream).start(seen);
  stream.emit("hello", '{"watching":false}');
  vi.advanceTimersByTime(60_000);
  expect(seen).not.toHaveBeenCalled();
});

it("настройка: автоматически — события; по кнопке — ничего", () => {
  const streams: FakeStream[] = [];
  const connect = () => {
    const s = new FakeStream();
    streams.push(s);
    return s;
  };
  const manual = vi.fn();
  changeSource("manual", "/api/events", undefined, connect).start(manual);
  expect(streams).toHaveLength(0);

  const auto = vi.fn();
  const stop = changeSource("auto", "/api/events", undefined, connect).start(auto);
  streams[0]!.emit("hello", JSON.stringify({ watching: true }));
  streams[0]!.emit("change");
  expect(auto).toHaveBeenCalledTimes(1);
  stop();
  expect(streams[0]!.closed).toBe(true);
});
