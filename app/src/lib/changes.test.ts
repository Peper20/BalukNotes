import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { changeSource, POLL_SECONDS, polling, serverEvents, type EventStream } from "./changes";

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

it("события сервера: change — проверить; опрос — только пока событий нет", () => {
  const stream = new FakeStream();
  const seen = vi.fn();
  const stop = serverEvents("/api/events", polling(5, () => false), () => stream).start(seen);
  stream.emit("hello", '{"watching":true}');
  vi.advanceTimersByTime(20_000);
  expect(seen).not.toHaveBeenCalled();
  stream.emit("change", '{"seq":1,"paths":["a.typ"]}');
  expect(seen).toHaveBeenCalledTimes(1);

  // Разрыв — опрос; соединение вернулось — одна проверка, опрос выключен.
  stream.onerror!(new Event("error"));
  vi.advanceTimersByTime(5_000);
  expect(seen).toHaveBeenCalledTimes(2);
  stream.emit("hello", '{"watching":true}');
  expect(seen).toHaveBeenCalledTimes(3);
  vi.advanceTimersByTime(20_000);
  expect(seen).toHaveBeenCalledTimes(3);

  stop();
  expect(stream.closed).toBe(true);
});

it("сервер не следит за файлами — опрос", () => {
  const stream = new FakeStream();
  const seen = vi.fn();
  serverEvents("/api/events", polling(5, () => false), () => stream).start(seen);
  stream.emit("hello", '{"watching":false}');
  vi.advanceTimersByTime(10_000);
  expect(seen).toHaveBeenCalledTimes(2);
});

it("наблюдатель сервера сломался — поток закрыт: одна проверка и опрос", () => {
  const stream = new FakeStream();
  const seen = vi.fn();
  serverEvents("/api/events", polling(5, () => false), () => stream).start(seen);
  stream.emit("hello", '{"watching":true}');
  stream.onerror!(new Event("error"));
  stream.emit("hello", '{"watching":false}');
  expect(seen).toHaveBeenCalledTimes(1);
  vi.advanceTimersByTime(10_000);
  expect(seen).toHaveBeenCalledTimes(3);
});

it("настройка: автоматически — события, без них опрос; по кнопке — ничего", () => {
  const streams: FakeStream[] = [];
  const connect = () => {
    const s = new FakeStream();
    streams.push(s);
    return s;
  };
  const manual = vi.fn();
  changeSource("manual", "/api/events", connect).start(manual);
  vi.advanceTimersByTime(POLL_SECONDS * 10_000);
  expect(manual).not.toHaveBeenCalled();
  expect(streams).toHaveLength(0);

  const auto = vi.fn();
  const stop = changeSource("auto", "/api/events", connect).start(auto);
  streams[0]!.emit("hello", JSON.stringify({ watching: false }));
  vi.advanceTimersByTime(POLL_SECONDS * 1000);
  expect(auto).toHaveBeenCalledTimes(1);
  streams[0]!.emit("hello", JSON.stringify({ watching: true }));
  vi.advanceTimersByTime(POLL_SECONDS * 10_000);
  expect(auto).toHaveBeenCalledTimes(1);
  streams[0]!.emit("change");
  expect(auto).toHaveBeenCalledTimes(2);
  stop();
  expect(streams[0]!.closed).toBe(true);
});
