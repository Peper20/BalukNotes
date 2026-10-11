// Sign-in state of the client: the server (with `notes serve --auth`) answers
// 401 to everything but the sign-in screen's own files, and the client then
// shows the sign-in form instead of the app. One place turns a 401 into this
// state: `request()` in `index.ts` calls `requireSignIn`; the app root
// (`main.ts`) listens and swaps the screen. A 401 is an answer, not a lost
// connection, and the loops that repeat requests stop on it (`isSignIn`).

let required = false;
let listener: (() => void) | undefined;

/** Whether a request was refused for the lack of a session. */
export const signInRequired = (): boolean => required;

/** Calls `fn` once the first time a session is needed (now, if it already is). */
export function onSignInRequired(fn: () => void): void {
  listener = fn;
  if (required) fn();
}

/** A request answered 401: from now on the sign-in screen is the client. */
export function requireSignIn(): void {
  if (required) return;
  required = true;
  listener?.();
}

/** Tests: back to "signed in". */
export function resetSignIn(): void {
  required = false;
  listener = undefined;
}

/** An error that means "sign-in needed" (an `ApiError` with 401, checked by its shape). */
export const isSignIn = (e: unknown): boolean => (e as { status?: unknown } | null)?.status === 401;

/** What the sign-in form shows under the fields for a failed attempt (an `ApiError`-like value). */
export function signInMessage(e: unknown): string {
  const err = e as { status?: number; retryAfter?: number | null; message?: string } | null;
  switch (err?.status) {
    case 401:
      return "Неверный логин или пароль";
    case 429:
      return err.retryAfter ? `Слишком много попыток, подождите ${err.retryAfter} с` : "Слишком много попыток, подождите немного";
    case 0:
      return "Сервер не отвечает";
    default:
      return err?.message || "Не удалось войти";
  }
}
