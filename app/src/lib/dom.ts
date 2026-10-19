// The document as the client reads it. A note's HTML goes into the page, and an
// `<img name="...">` in it shadows `document.<name>` for the whole page (DOM
// clobbering): with `name="body"` the `document.body` is the image, and the
// same goes for `getElementById`, `title`, `documentElement`. An element `id`
// shadows only `window.<id>`. So the client takes the document's nodes and
// methods once, here, when it loads (before any note is in the page), keeps
// them in module variables, and looks elements up inside its own container.
// Nothing else reads `document.<name>`.

const doc = document;

/** The root element: its attributes are the theme and the state (`data-state`). */
export const htmlEl = doc.documentElement;

/** The body: the client mounts into it, and every lookup of the client is inside it. */
export const bodyEl = doc.body;

/** The fonts of the page (`document.fonts`). */
export const fontSet = doc.fonts;

/** `document.createElement`, taken once. */
export const createElement: Document["createElement"] = doc.createElement.bind(doc);

/** `document.elementFromPoint`, taken once. */
export const elementAt = doc.elementFromPoint.bind(doc);

const titleEl = doc.head.querySelector("title") ?? doc.head.appendChild(createElement("title"));

/** The page title (`document.title`). */
export function setTitle(text: string): void {
  titleEl.textContent = text;
}

/** An element of the client by a selector, inside its body (not the whole document: a note can repeat an id). */
export const inApp = <T extends Element = HTMLElement>(selector: string): T | null => bodyEl.querySelector<T>(selector);

/** The focused element, if it is in the client (`document.activeElement`). */
export const focusedEl = (): Element | null => bodyEl.querySelector(":focus");
