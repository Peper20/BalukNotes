// A live block of a note: a selector of the library markup and the activation of one element.

/** Opens a note on a click on a graph node (without reloading the page). */
export type OpenNote = (id: string, background: boolean) => void;

export interface LiveContext {
  open: OpenNote;
}

export interface LiveBlock {
  /** Name for error messages (console). */
  name: string;
  /** The block's elements (`selectors.ts`). */
  selector: string;
  /**
   * Activates an element: returns the cleanup or `null` - the markup did not
   * fit, the fallback look stays. An exception is the same as `null` (with a warning).
   */
  mount(el: HTMLElement, ctx: LiveContext): (() => void) | null;
}
