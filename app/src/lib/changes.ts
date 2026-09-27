// Откуда клиент узнаёт, что файлы заметок могли измениться: события сервера
// (`/api/events`, сервер следит за файлами) или опрос раз в N секунд.
// Проверку (сверку версии) делает state/updates — источнику достаточно
// сказать «проверь».

import type { EventsHello } from "./api/types/EventsHello";

export interface ChangeSource {
  /** Начать слушать; вернуть «остановить». */
  start(onChange: () => void): () => void;
}

/** Опрос раз в `seconds` секунд (0 — никогда); скрытую вкладку не тревожить. */
export function polling(seconds: number, hidden: () => boolean = () => document.hidden): ChangeSource {
  return {
    start(onChange) {
      if (!(seconds > 0)) return () => {};
      const timer = setInterval(() => hidden() || onChange(), seconds * 1000);
      return () => clearInterval(timer);
    },
  };
}

/** Что нужно от `EventSource` (в тестах — подделка). */
export interface EventStream {
  addEventListener(type: string, listener: (e: MessageEvent<string>) => void): void;
  onerror: ((e: Event) => void) | null;
  close(): void;
}

/**
 * События сервера: `change` — проверить. Пока событий нет (сервер не следит
 * за файлами, соединение потеряно, старый сервер без `/api/events`) —
 * `fallback` (опрос). После восстановленного соединения — одна проверка:
 * изменения за время разрыва могли потеряться.
 */
export function serverEvents(url: string, fallback: ChangeSource, connect: (url: string) => EventStream = (u) => new EventSource(u)): ChangeSource {
  return {
    start(onChange) {
      let stopFallback: (() => void) | null = null;
      const useFallback = () => (stopFallback ??= fallback.start(onChange));
      const quitFallback = () => {
        stopFallback?.();
        stopFallback = null;
      };
      let es: EventStream;
      try {
        es = connect(url);
      } catch {
        useFallback();
        return quitFallback;
      }
      let lost = false;
      es.addEventListener("hello", (e) => {
        const hello = JSON.parse(e.data) as EventsHello;
        if (hello.watching) quitFallback();
        else useFallback();
        if (lost) onChange();
        lost = false;
      });
      es.addEventListener("change", () => onChange());
      es.onerror = () => {
        lost = true;
        useFallback();
      };
      return () => {
        es.close();
        quitFallback();
      };
    },
  };
}
