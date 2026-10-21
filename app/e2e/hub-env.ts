// The hub the browser-version spec talks to: shared by playwright.config.ts (it starts the server) and hub.spec.ts.
export const HUB_PORT = 8463;
export const HUB_URL = `http://127.0.0.1:${HUB_PORT}`;
/** The data directory of the hub, from the repository root. */
export const HUB_DATA = "tests/.data/e2e-hub";
export const HUB_LOGIN = "e2e";
export const HUB_PASSWORD = "e2e-password";
/** The hub vault made before the server starts, with one note. */
export const HUB_VAULT = "Учёба";
