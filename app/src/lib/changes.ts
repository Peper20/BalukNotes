// Откуда клиент узнаёт, что файлы заметок могли измениться: события сервера
// (`GET …/events?after=<seq>` хранилища - долгий опрос: сервер отвечает на
// изменении или через ~25 с пустым ответом, клиент сразу спрашивает снова).
// Опроса раз в N секунд нет (решение пользователя): без событий (сервер не
// следит, режим «только по кнопке») изменения — по кнопке «Обновить».
// Проверку (сверку версии) делает state/updates — источнику достаточно
// сказать «проверь». Связь с сервером видит api (`onReach`) по этим же запросам.

import type { EventsResponse } from "./api/types/EventsResponse";

export interface ChangeSource {
  /** Начать слушать; вернуть «остановить». */
  start(onChange: () => void): () => void;
}

/** Настройка `refresh.mode`: «автоматически» или «только по кнопке». */
export type RefreshMode = "auto" | "manual";

/** Один запрос событий (`api.events`): изменения после `after`. */
export type Poll = (after: number | null, signal: AbortSignal) => Promise<EventsResponse>;

/** Пауза перед новым запросом, если сервер не ответил. */
export const RETRY_MS = 3000;

/** Ничего не слушать: изменения — только по кнопке. */
const none: ChangeSource = { start: () => () => {} };

/** Источник изменений по настройке: автоматически — события сервера, по кнопке — никакого. */
export function changeSource(mode: RefreshMode, poll: Poll): ChangeSource {
  return mode === "manual" ? none : serverEvents(poll);
}

/**
 * События сервера: в ответе есть изменения — проверить. Сервер не ответил —
 * новый запрос через `RETRY_MS`; ответил снова — одна проверка: изменения за
 * время разрыва могли потеряться. `watching: false` — сервер не следит за
 * файлами, ждать нечего.
 */
export function serverEvents(poll: Poll): ChangeSource {
  return {
    start(onChange) {
      const abort = new AbortController();
      void (async () => {
        let after: number | null = null;
        let lost = false;
        while (!abort.signal.aborted) {
          let res: EventsResponse;
          try {
            res = await poll(after, abort.signal);
          } catch {
            if (abort.signal.aborted) return;
            lost = true;
            await sleep(RETRY_MS, abort.signal);
            continue;
          }
          if (lost || res.changes.length > 0) onChange();
          lost = false;
          after = res.seq;
          if (!res.watching) return;
        }
      })();
      return () => abort.abort();
    },
  };
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    signal.addEventListener("abort", () => (clearTimeout(timer), resolve()), { once: true });
  });
}
