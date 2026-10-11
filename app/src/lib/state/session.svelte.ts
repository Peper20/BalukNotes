// Who is signed in: only on a server with sign-in (`notes serve --auth`); the
// window and a server on localhost have none (`login` stays null, the
// interface shows no sign-out). The sign-in screen itself is
// components/SignIn.svelte, shown by main.ts when a request answers 401
// (lib/api/signin.ts).

import { api, ApiError } from "../api";

class Session {
  /** The login of this browser's session; null - no sign-in on this server. */
  login = $state<string | null>(null);

  /** Ends the session and reloads: the page then shows the sign-in screen. Returns an error text, if it failed. */
  async signOut(): Promise<string | null> {
    try {
      await api.logout();
    } catch (e) {
      return e instanceof ApiError ? e.message : String(e);
    }
    location.reload();
    return null;
  }
}

export const session = new Session();
