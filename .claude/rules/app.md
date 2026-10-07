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
- The server only through `src/lib/api/` (address and token - `api/config.ts`,
  errors - `ApiError`), no `fetch` in components. The source of changes is
  `changes.ts` (long polling `GET .../events?after=`, no polling every N
  seconds); the server connection is `state/connection.svelte.ts` (whether it
  answered - `api.onReach`).
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
- e2e - `e2e/*.spec.ts`: Playwright on the system chromium, a server on a copy
  of `tests/vault` in `tests/.data/e2e`, warmed up beforehand. A new interface
  feature - a new scenario; phone - `mobile.spec.ts` (400x800: the page is not
  wider than the window, measured after `document.fonts.ready`).
