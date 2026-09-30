// Откуда клиент узнаёт, что файлы заметок могли измениться: события сервера
// (`…/events` хранилища, сервер следит за файлами). Опроса нет (решение
// пользователя): без событий (сервер не следит, режим «только по кнопке»)
// изменения — по кнопке «Обновить». Проверку (сверку версии) делает
// state/updates — источнику достаточно сказать «проверь».

export interface ChangeSource {
  /** Начать слушать; вернуть «остановить». */
  start(onChange: () => void): () => void;
}

/** Настройка `refresh.mode`: «автоматически» или «только по кнопке». */
export type RefreshMode = "auto" | "manual";

/** Ничего не слушать: изменения — только по кнопке. */
const none: ChangeSource = { start: () => () => {} };

/** Связь с сервером по потоку событий: пропала (`false`) или есть (`true`). */
export type Reach = (reachable: boolean) => void;

/** Источник изменений по настройке: автоматически — события сервера, по кнопке — никакого. */
export function changeSource(mode: RefreshMode, eventsUrl: string, reach?: Reach, connect?: (url: string) => EventStream): ChangeSource {
  return mode === "manual" ? none : serverEvents(eventsUrl, reach, connect);
}

/** Что нужно от `EventSource` (в тестах — подделка). */
export interface EventStream {
  addEventListener(type: string, listener: (e: MessageEvent<string>) => void): void;
  onerror: ((e: Event) => void) | null;
  close(): void;
}

/**
 * События сервера: `change` — проверить. Разрыв — `reach(false)` (браузер
 * переподключается сам); снова `hello` — `reach(true)` и одна проверка:
 * изменения за время разрыва могли потеряться.
 */
export function serverEvents(url: string, reach: Reach = () => {}, connect: (url: string) => EventStream = (u) => new EventSource(u)): ChangeSource {
  return {
    start(onChange) {
      let es: EventStream;
      try {
        es = connect(url);
      } catch {
        return () => {};
      }
      let lost = false;
      // `hello` говорит и, следит ли сервер за файлами: не следит — изменения по кнопке.
      es.addEventListener("hello", () => {
        reach(true);
        if (lost) onChange();
        lost = false;
      });
      es.addEventListener("change", () => onChange());
      es.onerror = () => {
        lost = true;
        reach(false);
      };
      return () => es.close();
    },
  };
}
