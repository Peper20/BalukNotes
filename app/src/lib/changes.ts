// Откуда клиент узнаёт, что файлы заметок могли измениться: опрос раз в N
// секунд или события сервера. Проверку (сверку версии) делает
// state/updates — источнику достаточно сказать «проверь».

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
