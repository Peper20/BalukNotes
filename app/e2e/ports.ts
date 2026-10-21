// The ports of the e2e: all lie in one range reserved for them, 8600-8649
// (agents' test servers use 8500-8599, `tools/test-env.sh` 8432). No port
// number is written anywhere else in `app/e2e/` or `playwright.config.ts`.

/** The most Playwright workers: each has an app server on its own port. */
export const MAX_WORKERS = 8;
/** The app server of the Playwright worker `parallelIndex`: 8600 + the index. */
export const appPort = (parallelIndex: number): number => 8600 + parallelIndex;
/** The server that global-setup warms (the template of the workers' data). */
export const WARM_PORT = 8644;
/** The server with sign-in of `auth.spec.ts`. */
export const AUTH_PORT = 8640;
/** The storage server (hub) of `sync.spec.ts`. */
export const SYNC_HUB_PORT = 8641;
/** The app server of `sync.spec.ts`. */
export const SYNC_APP_PORT = 8642;
/** The hub with the browser version (`hub.spec.ts`, started by `playwright.config.ts`). */
export const HUB_PORT = 8643;
/** An address nobody listens on (`sync.spec.ts`: a server that does not answer). */
export const SILENT_PORT = 8649;
