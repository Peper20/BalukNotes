// How the client learns that note files may have changed: server events
// (`GET .../events?after=<seq>` of a vault - long polling: the server
// answers on a change or after ~25 s with an empty response, the client asks
// again at once). There is no polling every N seconds (user's decision):
// without events (the server does not watch, the "by button only" mode)
// changes come by the "Обновить" button. The check (comparing versions) is
// done by state/updates - a source just says "check". The api sees the
// connection to the server (`onReach`) by the same requests.

import { isSignIn } from "./api/signin";
import type { EventsResponse } from "./api/types/EventsResponse";

export interface ChangeSource {
  /** Starts listening; returns "stop". */
  start(onChange: () => void): () => void;
}

/** The setting `refresh.mode`: "automatically" or "by button only". */
export type RefreshMode = "auto" | "manual";

/** One events request (`api.events`): changes after `after`. */
export type Poll = (after: number | null, signal: AbortSignal) => Promise<EventsResponse>;

/** Pause before a new request if the server did not answer. */
export const RETRY_MS = 3000;

/** Listen to nothing: changes only by the button. */
const none: ChangeSource = { start: () => () => {} };

/** The change source by the setting: automatic - server events, by button - none. */
export function changeSource(mode: RefreshMode, poll: Poll): ChangeSource {
  return mode === "manual" ? none : serverEvents(poll);
}

/**
 * Server events: the response has changes - check. The server did not
 * answer - a new request after `RETRY_MS`; it answered again - one check:
 * changes during the break may have been lost. `watching: false` - the
 * server does not watch the files, nothing to wait for. A refusal for the
 * lack of a session (401) ends the listening: the sign-in screen takes over,
 * and a new start comes with the page.
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
          } catch (e) {
            if (abort.signal.aborted || isSignIn(e)) return;
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
