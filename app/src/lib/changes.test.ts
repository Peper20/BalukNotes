import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { EventsResponse } from "./api/types/EventsResponse";
import { changeSource, RETRY_MS, serverEvents, type Poll } from "./changes";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** Подделка сервера: каждый запрос ждёт, пока тест ответит. */
class FakeServer {
  calls: { after: number | null; signal: AbortSignal; resolve: (r: EventsResponse) => void; reject: (e: unknown) => void }[] = [];
  poll: Poll = (after, signal) => new Promise((resolve, reject) => this.calls.push({ after, signal, resolve, reject }));
  get last() {
    return this.calls.at(-1)!;
  }
}

const res = (seq: number, paths: string[] | null = null, watching = true): EventsResponse => ({
  watching,
  seq,
  changes: paths ? [{ seq, paths }] : [],
});
const flush = () => vi.advanceTimersByTimeAsync(0);

it("изменения — проверить; следующий запрос — с номером ответа", async () => {
  const server = new FakeServer();
  const seen = vi.fn();
  const stop = serverEvents(server.poll).start(seen);
  await flush();
  expect(server.last.after).toBeNull();
  server.last.resolve(res(0));
  await flush();
  expect(seen).not.toHaveBeenCalled();
  expect(server.last.after).toBe(0);

  server.last.resolve(res(3, ["a.typ"]));
  await flush();
  expect(seen).toHaveBeenCalledTimes(1);
  expect(server.last.after).toBe(3);
  // Ответ по тайм-ауту — без изменений.
  server.last.resolve(res(3));
  await flush();
  expect(seen).toHaveBeenCalledTimes(1);
  expect(server.calls).toHaveLength(4);

  stop();
  expect(server.last.signal.aborted).toBe(true);
});

it("сервер не ответил — повтор через паузу; ответил — одна проверка", async () => {
  const server = new FakeServer();
  const seen = vi.fn();
  serverEvents(server.poll).start(seen);
  await flush();
  server.last.resolve(res(2));
  await flush();
  server.last.reject(new Error("offline"));
  await flush();
  expect(server.calls).toHaveLength(2);
  await vi.advanceTimersByTimeAsync(RETRY_MS - 1);
  expect(server.calls).toHaveLength(2);
  await vi.advanceTimersByTimeAsync(1);
  expect(server.calls).toHaveLength(3);
  expect(server.last.after).toBe(2);
  server.last.resolve(res(2));
  await flush();
  expect(seen).toHaveBeenCalledTimes(1);
});

it("сервер не следит за файлами — запросов больше нет, только по кнопке", async () => {
  const server = new FakeServer();
  const seen = vi.fn();
  serverEvents(server.poll).start(seen);
  await flush();
  server.last.resolve(res(0, null, false));
  await vi.advanceTimersByTimeAsync(60_000);
  expect(server.calls).toHaveLength(1);
  expect(seen).not.toHaveBeenCalled();
});

it("настройка: автоматически — события; по кнопке — ни одного запроса", async () => {
  const server = new FakeServer();
  changeSource("manual", server.poll).start(vi.fn());
  await flush();
  expect(server.calls).toHaveLength(0);

  const stop = changeSource("auto", server.poll).start(vi.fn());
  await flush();
  expect(server.calls).toHaveLength(1);
  stop();
  expect(server.last.signal.aborted).toBe(true);
});
