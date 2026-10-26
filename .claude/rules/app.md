---
paths:
  - "app/**"
---

# app - the client

Svelte 5 (runes) + TypeScript + Vite, no SvelteKit. How the client is tied to
the core - `docs/architecture.md`.

```sh
npm run dev     # hot reload: :5173, the API is proxied to :8432 (tools/test-env.sh)
npm run build   # app/dist: a debug notes serve takes it from disk - reloading the page is enough
npm run types   # API types from Rust
npm test        # Vitest; also check, e2e
```

- State - modules in `src/lib/state/` (startup - `start()` in `index.ts`) and
  `ui.svelte.ts` (panels, chapter, contents); new state is its own module, not
  a field in someone else's. Pure logic - `src/lib/*.ts` with Vitest next to it
  (`*.test.ts`); components - `src/components/`.
- The server only through `src/lib/api/` (address - `api/config.ts`,
  errors - `ApiError`; a 401 answer -> the sign-in screen, one place:
  `api/signin.ts`), no `fetch` in components. The source of changes is
  `changes.ts` (long polling `GET .../events?after=`, no polling every N
  seconds); the server connection is `state/connection.svelte.ts` (whether it
  answered - `api.onReach`).
- Where a request goes - `api/transport.ts`: HTTP to the server, or, with
  `__NOTES_API__.core`, a message to the core in a worker (`src/worker/`;
  `host.ts` is its queue, `opfs.ts` writes the vault and settings through to
  OPFS). Stylesheets, fonts and `pdfUrl` stay HTTP.
- **API types come from Rust** (`ts-rs`, feature `ts`): `npm run types`
  exports them to `src/lib/api/types/` (in git). Changed a response structure -
  export and commit; do not edit by hand.
- The vault is in the address (`/v/<name>/...`, `lib/vault.ts`), chosen before
  the state loads (`lib/boot.ts`). Addresses only through `lib/ids.ts`
  (`noteHref`, `homeHref`...), not as a string `"/"`/`"/n/..."`;
  `localStorage` through `lib/storage.ts` (each vault has its own keys).
- The reader sees the title, not the file name: `notes.title(id)`,
  `notes.folderTitle(path)` (`lib/state/notes.svelte.ts`).
- Note HTML is inserted into the DOM directly (`NoteView.svelte`), not by a
  template.
- The document only through `lib/dom.ts`, never `document.<name>` or
  `window.<id>`: a note's `id`/`name` can shadow them (DOM clobbering).
- A view setting = an entry in `notes-core::settings::Schema`
  (`.attr("data-...")` or `.var("--...", "px")`) + a CSS rule; `appearance.ts`
  applies it by the schema.
- Note styles only inside `.k-note { ... }`: blocks in `src/baluk-css/`
  (order - `@import` in `baluk.css`) are joined into `assets/baluk.css` and go
  through lightningcss (prefixes: the app window is WebKit, it needs
  `-webkit-user-select`; browsers - `TARGETS` in `bundle.ts`). The interface -
  `src/app.css`.
- Live blocks (interactive figures, frames, the graph) - the registry
  `src/lib/live/`: a module with `LiveBlock` + a selector in `selectors.ts` + a
  line in `BLOCKS`.
- Icons - `@lucide/svelte` (`import X from "@lucide/svelte/icons/x"`), not
  text glyphs. The frames panel has a "quiet" look: outline icons without
  background or border (user's decision).
- Interface classes are unique by meaning: a shared `.help` of a setting hint
  and the help dialog once stretched the settings past the screen edge.
- The theme on `<html>` is set before the client (`public/assets/theme.js`,
  key `k-theme`); `settings.apply` does not touch `<html>` until settings
  arrive: otherwise the first frame is white.
- Another note: the previous one stays on screen while the new one loads, but
  no longer than `STALE_MS` (`reader.svelte.ts`) - from the cache the switch has
  no empty frame.
- `<html data-state="loading|ready">`: tools and e2e wait for `ready` - a new
  screen or state follows the same.
- The graph draws the core's ready layout (`POST /api/graph/layout`): view -
  `graph-view.ts`, gestures - `graph-gesture.ts`, moves - `graph-motion.ts`,
  physics - `graph-physics.ts` (the core layout is the rest state).
  `RETURN = 0.25` and `DAMPING = 0.3` were tuned by the user (neighbors come back
  a quarter of the way): do not change without them.
- e2e - `e2e/*.spec.ts`: Playwright on the system chromium. Every Playwright
  worker has its own app server on its own copy of `tests/vault`
  (`<DATA_ROOT>/e2e/w<index>`, in RAM when it can be, `e2e/server.ts`; the `app` fixture of `e2e/fixtures.ts`: specs
  import `test` and `expect` from it), started on a copy of the template data
  that `e2e/global-setup.ts` warms once (the binaries are built there, too;
  `e2e/cache-key.ts` mirrors the cache directory name of
  `notes-core::cache::DiskCache::new` and must change with it);
  the files run in parallel, the tests of a file in order, so a file relies only
  on the fixture vault and on what it does itself. The number of workers is
  `PLAYWRIGHT_WORKERS` (`.claude/rules/tools.md`, "CPU cap"; unset: one). A new
  interface feature - a new scenario; phone - `mobile.spec.ts` (400x800: the page is not
  wider than the window, measured after `document.fonts.ready`).
  `e2e/wasm.spec.ts` needs `tests/.data/wasm/` (the module, `fonts.json` with
  `fonts/`, and `files.json`), which the `wasm-module` step of `tools/check.sh` makes
  (`crates/notes-wasm/harness/build.sh` and `node
  crates/notes-wasm/harness/run.mjs --prepare`); `npm run e2e` alone without
  them fails with 404s. `e2e/hub.spec.ts` talks to a second server of
  `playwright.config.ts`, `notes-hub --web tests/.data/web`, over the bundle
  that the `web-bundle` step of `tools/check.sh` makes (`tools/web-bundle.sh`); without
  `tests/.data/web` `npm run e2e` fails at the start of that server. It sets
  `core.syncEvery` (1 s, `CoreSource`) in the bundle's `assets/config.js` by a
  page route, so the sync scenario does not wait for the 30 s round. Ports of
  the e2e: 8600-8649 (`e2e/ports.ts`), written nowhere else.
