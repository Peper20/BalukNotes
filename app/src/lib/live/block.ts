// Живой блок заметки: селектор разметки библиотеки и оживление одного элемента.

/** Открыть заметку по щелчку на узле графа (без перезагрузки страницы). */
export type OpenNote = (id: string, newTab: boolean) => void;

export interface LiveContext {
  open: OpenNote;
}

export interface LiveBlock {
  /** Имя для сообщений об ошибках. */
  name: string;
  /** Элементы блока (`selectors.ts`). */
  selector: string;
  /**
   * Оживить элемент: вернуть уборку или `null` — разметка не подошла, остаётся
   * запасной вид. Исключение — то же, что `null` (с предупреждением).
   */
  mount(el: HTMLElement, ctx: LiveContext): (() => void) | null;
}
