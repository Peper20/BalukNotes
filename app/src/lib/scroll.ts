// Not the document scrolls but the page column under the top bar (`#page`):
// the sidebar, the top bar and the outline stay in place. In the window
// (WebKitGTK) scrolling the document re-lays the fixed layers on every frame,
// and with fractional screen scaling they jitter by a pixel.

/** The column that scrolls; before it appears (the vault picker screen), the document. */
export function scroller(): HTMLElement {
  return document.getElementById("page") ?? document.documentElement;
}

/** Column scroll from the top, px. */
export function scrollTop(): number {
  return scroller().scrollTop;
}

/** Scrolls the column to `y` (instantly). */
export function scrollToTop(y: number): void {
  scroller().scrollTo(0, y);
}

/** Scrolled to the end (the last sections will not reach the top anymore). */
export function atBottom(): boolean {
  const s = scroller();
  return s.clientHeight + s.scrollTop >= s.scrollHeight - 2;
}
