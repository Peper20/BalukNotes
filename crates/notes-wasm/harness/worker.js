// The E4 prototype in a Web Worker: one message - the notes to build.
import { measure } from "./measure.js";

onmessage = async ({ data: { notes } }) => {
  try {
    postMessage(await measure(notes));
  } catch (e) {
    postMessage({ error: String(e?.stack ?? e) });
  }
};
