# Architecture

Notes on Typst in the style of printed lecture notes, the best of Obsidian
(links, a graph, quick switching, search) and interactivity a PDF does not
have. Platforms: browser, desktop, Android (iOS - if there is time left).

## Priorities

1. **Rendering** of Typst notes in HTML as beautiful as in PDF.
2. Navigation: tree, tabs, quick switching, search, backlinks, graph.
3. Interactivity: a 2D/3D graphing calculator, animations, sliders.
4. A server online - storage and sync (HTTPS, sign-in); the core on devices:
   desktop and Android (Tauri), browser (WASM).
5. Some day: a built-in editor, version history, backups, encryption.

Notes are written mostly by Claude Code, the app **shows** them: files on disk
are the source of truth.

## Decisions

### 1. Typst compiles on the device, the server is storage

User's decision: the VPS is weak (1 core, 2 GB), Typst cannot build there.

- **The core (`notes-core`: compiling, cache, link index, graph, search) runs
  on the device**, next to the `app/` interface, and answers it over an HTTP
  API (`notes-server`); the layer `app/src/lib/api/` knows only the address.
  - Desktop (the main way on a computer) - a Tauri 2 window without Typst
    (`notes-app`): the core is a `notes-typst serve` process the window
    connects to over a Unix socket (`0600` in `$XDG_RUNTIME_DIR`), or starts
    itself and stops on exit. The interface uses its own Tauri address scheme,
    the window passes a request to the socket. No port: browser pages and
    other users cannot reach the core, no sign-in is needed. Typst is on disk
    once.
  - **One app, several windows.** `notes app [--vault <name> [<note>]]` (and
    the menu entry) opens a page; a second launch hands its page to the
    running app over a second socket (`app.sock` next to the core socket) and
    exits. A window shows one vault. Another vault opens in the same window
    or a new one (the setting `vaults.open`, a right click or Ctrl+click in
    the vault menu); a vault already shown by a window is never opened twice -
    that window gets the focus (the app cancels the navigation). The client
    asks for a new window with a plain `window.open`, so in a browser the
    same code opens a tab.
  - Browser on a computer - `notes serve` (self-hosting, checks; in the
    background - a systemd user service, `notes service`, the CLI writes the
    unit, no root). The window does not need either.
  - Android - the same window, the core inside the app (there is no console):
    the scheme is served by the same `Router` in-process
    (`docs/research/E7.md`).
  - There is no separate API over Tauri IPC. A scheme response is only whole,
    so events use long polling, not SSE.
  - A browser without installation - the core in WASM (a Web Worker), the API
    transport is messages to the worker. The hardest parts (Typst speed and
    memory on a phone, fonts and packages in the browser) - research E4 first.
- **The server online** - vault files, their versions, change events,
  sessions; without Typst.
- **Self-hosted and web version** (user's decision): `notes serve` with Typst
  - on one's own machine or home server; on a weak VPS - a server without
  Typst, the device or WASM in the browser builds notes. The web version is the
  same client, served by the server.
- **Parts are installed separately** (user's decision). `notes` is thin: help,
  light commands and calling the parts in its folder (`notes app` ->
  `notes-app`), with a version check; a missing part - a message which one.
  Parts: `notes-typst` (building, `check`, `pdf`, `serve`, `sync`),
  `notes-app` (the window, without Typst), `notes-hub` (the storage server and
  `notes users`, without Typst). Typst only in `notes-typst`. What the parts
  share without Typst (the data directory rules, vault names, accounts,
  sessions, the sync engine) is the light crate `notes-store`.
- **Every device has a copy of the vault** with sync (§9): notes are read
  without a network, Claude Code writes to the copy on the computer.
- Typst is a Rust library in the core with a pinned version: the HTML export
  does not break with an update of the system `typst`.
- One interface for all platforms - Svelte 5 + TypeScript + Vite: a shell
  around ready note HTML, the smallest runtime (Android WebView), style
  isolation. API types come from Rust.
- **The vault is in the address**: the client - `/v/<vault>/n/<path>`, the
  API - `/api/vaults/<vault>/...`; themes and fonts are shared, without a
  vault (settings - shared and per vault, §6). One browser tab - one vault (as
  an Obsidian window): another opens by navigating with a reload, and is
  chosen before the state loads, so tabs and reading positions in
  `localStorage` are separate for each. An address without a vault (`/`, the
  former `/n/...`) - the last one opened in this browser, otherwise the choice
  screen (`VaultPicker`: list and creation). The core sets `#see` links
  without a vault (`/n/...`, one HTML for all addresses), the client adds
  `/v/<vault>`. The server opens a vault on the first request (each has its
  own `Notes` core; fonts and themes are shared with the vault-less core, the
  library is one) and warms only the active one - the last opened or the one
  that hinted the warm-up. An inactive vault without requests or event
  streams for 10 minutes is closed by the server, the next request opens it
  again. The vault-less core serves themes and fonts, so the server works
  without vaults too. The vault switcher is at the bottom of the sidebar:
  switch, new, rename and delete the open one (the folder - to the system
  trash). The open vault is closed before that (`Notes::close`: warm-up and
  watcher stop); tabs and reading positions in this browser move with it
  (`storage.ts: moveVault`).
- The core address is one client setting (`app/src/lib/api/config.ts`:
  `globalThis.__NOTES_API__` or `configure`), the server that served the page
  by default. Who may use the server - the session cookie (§9).

```
   self-hosting          desktop / Android (Tauri)     browser (plan)
   browser: app/         WebView: app/                 app/
      | HTTP                | own address scheme          | messages
   notes serve           socket -> notes-typst serve   core in WASM (worker)
                         (Android - core in-process)
   notes-core            notes-core                    notes-core
   vault copy            vault copy                    vault copy
      +--------------------------+-----------------------------+
                                 | HTTPS: sign-in, files, versions, events
                       +---------+----------+
                       | server (VPS)       |  storage, sync, sessions
                       | without Typst      |  1 core, 2 GB
                       +--------------------+
```

### 2. A vault is a folder of `.typ` files, one Typst project

- **The data directory** is outside the repository: `--data` / `NOTES_DATA`,
  otherwise `data` from `~/.config/baluk-notes/config.toml`, otherwise
  `~/.local/share/baluk-notes` (`notes info` - where). It holds
  `vaults/<name>/` - vaults, `settings.json`, `cache/` (can be deleted).
  `notes` works from any folder: the library, client and fonts are embedded.
  Code reaches data only through `notes-core`, the core reaches files through
  `trait Storage` (`DirStorage`, `MemStorage` in tests): replacing it with a
  database touches one place.
- **There are several vaults**, as in Obsidian: the name is the folder name
  in `vaults/` (`notes-core::vaults::VaultName`: no `/`, characters forbidden
  on Windows, or internal `_`/`.`). Each has its own notes, links, graph,
  cache, tabs and reading positions. **There is no default vault** (user's
  decision): even the first one is created and named by the user (in the app
  or `notes vaults new`); the program never creates or moves vaults itself.
  Note commands always take `--vault <name>` (without it - an error with the
  list); a folder outside the data directory - by path (`--vault
  tests/vault`). Vault settings live in `<vault>/.baluk/` (they move with the
  folder); device settings - in the data directory.
- **Deleting a note or folder** - only from the interface, with confirmation,
  to the system trash (`Storage::trash`, the `trash` crate; a book - as a
  folder, a folder - with everything in it).
- **Renaming** - from the interface, by a plan in a dialog
  (`notes-core::rename`): the title in the file (the template's `title:`, for
  a folder - `_folder.toml`), the file name from it (as in `notes new`),
  literal `#see` to it in all notes.
- **Folders in the tree** - with notes and empty ones (`Vault::folders`); a
  directory with only non-note files (`code/`, `img/` next to a note) is not a
  folder. Tests and e2e pass their own trash (`--trash`, `NotesConfig::trash`).
- The vault root is the Typst project root; notes lie in folders by area.
- The library is attached **virtually**: the core serves `/_baluk/...` from
  the app's `baluk/`, in release - embedded in the binary; its version always
  equals the app version. A plain `typst compile` cannot build the vault - the
  app makes PDF (`notes pdf`, a button in the client) and its pages as PNG
  (`notes png`, `typst-render`). The theme comes from
  `sys.inputs.theme`.
- **A note** is one `.typ`, built separately: fast, and a broken one does not
  break the others. **A book** (large notes) is a folder with `main.typ` and
  chapters, so chapters do not count as notes; its `=` is a numbered chapter.
- **Book properties**: `main.typ` is the root (user's decision: a separate
  file, not the first chapter); `book.with` - shared by all chapters
  (language, words, tags), a chapter inherits it. A chapter's own -
  `chapter.with` (title, tags, label): it sets the chapter heading and
  `ul.k-tags.k-chapter-tags`. A book is built whole, so its chapters share the
  language and theme; chapter tags are in the source index (`Section::tags`: a
  chapter is a first-level section with its own tags), the page has only the
  root tags (`passes/tags.rs`). A book tag in the graph filter and `notes tags`
  belongs to the root or any chapter; the note list returns chapters with
  their own tags (`search::TaggedChapter`, the anchor is the heading `id`),
  the tag page shows them as separate rows.
- **Title and path differ** (user's decision). The title is the template's
  `title:` (for a book - in `main.typ`), any text; the path is only an
  address: links, checks, PDF, URL use it. The interface shows titles
  (without `title:` - the file name). The source index takes the title without
  compiling (`graph::Snapshot::title`). A folder title is `_folder.toml` in it
  (`title = "..."`, `notes-core::folders`; an unknown key is a `notes check`
  error). `notes new --title` makes the file name from the title
  (`new_note::file_name`: without characters forbidden on Windows, internal
  `_`/`.` at the start, device names and `main`; a taken one, ignoring case,
  gets a number). A `#see` without text takes the target title from
  `/_vault/title/<path>` (`graph::TitleData`), so the version of the linking
  note includes the target title.
- **Links** - `#see("Сеть/SSH", anchor: "Туннели")`, in HTML
  `<a class="k-link" data-k-target data-k-anchor>`, the core sets the address.
  The anchor is a heading text or a label; the core gives an `id` to a heading
  without a label: spaces and common punctuation -> `-`, other characters stay
  (`C++`, `C#`, `%23` in the address). `notes check` checks links against the
  built pages (for a failed one - against the index). Backlinks and the graph
  come from the **source index** (`notes-core::graph`: the Typst parser finds
  `#see` with a literal string, a file is parsed again only if it changed)
  plus the links of the last successful build (computed paths).
- **The graph is a core module** (`notes-core::vault_graph`): a filter
  (folders, tag, unwritten, unlinked, neighbors of a note) and a deterministic
  layout (a force model: Barnes-Hut repulsion, attraction to the center for a
  constant area per node, the start - nodes in path order along a Hilbert
  curve; then spreading the "node + label" rectangles; the `Layouts` cache) -
  the same for everyone. The layout forces (`Forces`: folders, repulsion,
  center, links) are in the filter: the graph page passes the `graph.*`
  settings, a graph in a note uses the defaults (the PDF does not depend on
  the device). The client draws the ready graph (`POST .../graph/layout`) on
  the home page and on `/graph`; a note inserts it via `#vault-graph(...)`,
  which reads `/_vault/graph/<filter>.json` (the vault data registry
  `vault_data`), draws with CeTZ (PDF, HTML without JS), and the client brings
  it to life at the same coordinates. The version of such a note includes a
  hash of the graph response: an edit that did not change the graph does not
  rebuild it. The core layout is the rest state: motion (dragging with
  neighbors, moves, appearing) happens only in the browser, an untouched graph
  matches the PDF picture, "restore the layout" on `/graph` - a smooth move to
  it.
- **Books as chapters** (`GraphFilter::chapters`, a checkbox on `/graph`): a
  book is the root and chapter vertices (`<book>/.<number>`, first-level
  sections) with "book - chapter" edges - shorter and stiffer than links, so a
  chapter stays near the root. A link from a chapter starts at its vertex
  (section links come from the outline, `outline::Section::links`), a link
  into a book section (`anchor`) goes to the chapter of that section; without
  an anchor and before the first chapter - the root. A click on a chapter -
  the book at its heading. A chapter is a hollow circle smaller than any note
  (`CHAPTER_R`), the "book - chapter" link is a pale line.
- **The note language** is the template's `lang:`: layout words from the
  dictionary `baluk/i18n.typ` (own ones - `words:`), in HTML - `lang` on
  `<article>`. The library exports the dictionary languages in `css.typ`
  (`<k-langs>`), and `notes check` warns about a `lang:` without a dictionary
  and without `words:`.

### 3. Rendering - the Typst HTML export

- Text is real HTML: wrapping to the screen width, search, selection,
  anchors. Formulas - MathML. CeTZ figures - `html.frame`, i.e. SVG as in the
  PDF.
- In HTML mode the library emits elements with classes (`<div class="k-box
  k-def">`), CSS sets the look (`app/src/baluk-css/`); what the HTML export
  drops and how the template covers it - `.claude/rules/baluk.md`.
- The PDF is built from the same source (printing, an exam).

### 4. Themes: at least two, switching without a reload

- Colors of text, boxes and headings are CSS variables, `[data-theme=...]`;
  a theme also has `color-scheme` (scrollbars, inputs). The first frame of a
  page is already in the theme: `app/public/assets/theme.js` sets it before
  styles and the client (from the memory of the last run), a theme not yet
  remembered - window colors follow the system.
- **Figures** (SVG) contain baked-in colors, and derived ones (`lighten`,
  `transparentize`) cannot be replaced with variables. So a note is compiled
  once per theme and the core merges them: text from the first compilation,
  each figure has a variant per theme (the figure order is the same).
- **HTML processing is a chain of passes**: over the `typst-html` tree
  (themes, anchors, links, formula brackets, tags), over the text of the raw
  page (code colors) and after the cache for the reader's settings (figures,
  shared frame parts); how to add one - `.claude/rules/crates.md`.
- **Figure weight** (`notes-core::figures`, over the ready SVG): variants that
  differ only in colors are merged into one SVG with `--kfN` variables per
  theme (the page `<style>`, the tag `data-k-figs`); label glyphs - one shared
  `<svg class="k-glyphs">`; coordinates are rounded (the setting "Рисунки ->
  точность", 0.01 pt by default) without accumulating error. Not merged
  (different geometry, gradients) - the variants stay, CSS shows the right
  one. "Матан": 3.7 MB -> 1.66 MB, 0.21 MB over the network with brotli.
- Figure processing depends on settings, so the raw rendering is cached, and
  memory holds one processed copy: changing the precision does not recompile
  the note, settings are part of the page version.
- Code highlighting: in HTML mode the highlighting theme uses "reference"
  colors, the core replaces them with CSS variables.

### 5. Interactivity - web components

- **Interactive figures** (`baluk/plots.typ`) emit `div.k-plot` with JSON in
  `data-k-plot` (formulas as strings, ranges, parameters) and a plain `canvas`
  inside - the PDF and a browser without JS see it. The client mounts the
  `Plot` component on `.k-plot` (SVG for 2D, canvas for 3D, sliders) and
  hides the frame. Both Typst and the client evaluate the formula - our own
  parser of one subset (`eval` would fail on division by zero). Colors are
  the theme variables `--k-fig-*`.
- **Frames** (`baluk/frames.typ`): a figure with a parameter is compiled for N
  values (up to 60) into `div.k-frames` with N `div.k-frames-item`, each a
  plain `canvas`. Frames sit in one cell of a CSS grid (the space is that of
  the largest); without JS the default frame is visible. Repeated SVG parts
  (axes, frame, labels) are moved by the core into a hidden group
  `svg.k-frames-defs` (`notes-core::frames`, `<use>` in their place). The
  client (`Frames.svelte`) adds a slider, buttons and playback and switches
  `data-current` without redrawing. Typst computes everything - the library
  style is kept. In PDF - the default frame or a row (`pdf: (1, 4, 8)`).
- Later (some day): algorithm animation and running code.

### 6. Settings - in the client, the schema - in the core

- The schema (`notes-core::settings`: key, group, label, type, bounds,
  default, **how to apply** - a `data-...` attribute or a CSS variable on
  `<html>`); the client draws the form and applies the look by the schema.
- Values live in `settings.json` of the data directory, the server checks
  them against the schema.
- **Vault settings** (user's decision, like project settings in VS Code): any
  setting except device ones can be set only for a vault -
  `<vault>/.baluk/settings.json` (moves with the folder); not set - the shared
  one. A change from the interface applies to the open vault
  (`/api/vaults/<vault>/settings`, `null` - shared again), for theme and font
  size (`SettingDef::shared`) - to all, if the vault has no own value. A
  setting has "for all vaults" (write it as shared), theme and font size -
  "only for this vault"; the settings window shows where a value comes from
  (only this vault, changed for all, default). The server takes the result for
  a vault (figures: `figures.*`). A file with a JSON error is not
  overwritten: the vault goes without its settings, changes stay in memory,
  and the response carries `problem` (path, line, column) - the settings
  window warns.
- **Device settings** (group `device`): warm-up, simultaneous builds, Typst
  memory, cache limits - each device has its own, defaults by platform
  (computer or phone), not carried over by sync; the look is shared. The core
  applies them on the fly (at server start and after `PUT /api/settings`).
- View settings (header, numbering, chapter style, font size, column width,
  theme) are CSS attributes on `<html>`, without recompiling. So the library
  emits markup with a margin (numbers in `span.k-num` always), and the client
  hides them.

### 7. Opening speed

- The themes of one note are built in parallel (a thread per theme, a shared
  file cache); different notes too (a build has its own file cache from a
  pool, the number is a device setting, 2 by default): a note can be opened
  while a book is building. A warm-up build does themes in turn in one thread
  with lowered priority.
- The core is layers (`notes-core`): `storage` -> `pipeline` (compiling per
  theme -> rendering -> figures; behind `trait Pipeline`) -> `page_cache`
  (memory + disk `cache.rs`) -> `pages` (a page on request: one build per
  note, an error over the previous rendering) -> `warm` (warm-up) -> `notes`
  (the facade for the server and CLI). Each layer is tested without Typst.
- The memory cache is pages in an LRU limited by bytes (64 MB) plus small
  records for every note (version, files, errors, build time): the version and
  "is it built" without reading pages.
- **The disk cache** (`<data>/cache/pages/<vault>/`, `notes-core::cache`): a
  note record (version and files, errors, build time) and the raw rendering
  in a separate file. Valid with the same rendering code (the tag: format
  version, the hash of sources from `build.rs`, the embedded library, the font
  set) and the same files: after a restart a book opens in tens of
  milliseconds. Build errors are cached too. Cleaning happens when the warm-up
  starts: deleted notes, foreign records older than 14 days, a 512 MB limit.
- **A book by chapters** (a setting): it is built whole (counters, links
  between chapters), and `GET .../notes/{id}?chapter=N` (or `?anchor=`)
  returns one chapter - a cut of the ready HTML (`notes-core::book`,
  milliseconds) with the chapter list and an "anchor -> chapter" map. The
  client fetches neighboring chapters ahead of time (`chapters.ts`; it checks
  the book version before switching). A "Матан" chapter over the network is
  ~0.2 MB instead of 1.7 MB, and a phone does not lay out the whole book.
  Ctrl+F in a note is the palette search (`lib/find.ts`): in the note or the
  whole book (`.../search?note=`, the chapter number from the anchor map), in
  the shown chapter (the same results, filtered by the anchor map) or in the
  vault; a second Ctrl+F is the browser search. What the book shares (title,
  root tags) is at the top of the contents in any chapter (`BookInfo.svelte`,
  user's choice).

### 8. Updates on request; the file watcher is only a speedup

- Compiling is lazy: a note is built on request and cached with the list of
  files it read.
- The note version is a hash of the modification times and sizes of those
  files (`stat`), taken **at read time**: a file edited during a build makes
  it stale at once. The hash is stable (SipHash, `notes-core::version`). The
  client checks the version on the "Обновить" button, and in the "automatic"
  mode (`refresh.mode`) - also on a server event and when returning to the
  window.
- **The watcher** (`notes-core::watch`, in the server): `notify` watches the
  vault (`Storage::watch`) and the library if it is on disk; changes come as
  a batch after a 100 ms pause. While nothing changes, the link index does not
  walk the vault (but at least once in 10 minutes), `DirStorage` remembers the
  file list, the warm-up wakes up on a change, the client learns about a batch
  by long polling (`/api/vaults/<vault>/events?after=<seq>`: an answer on a
  change or an empty one after 25 s; a vault keeps a log of the last 64
  batches, lost ones - "check everything"). There is no polling every N
  seconds (user's decision): the server does not watch or the mode is "by
  button only" - changes come by the "Обновить" button.
- **No connection to the server** (`state/connection`): a request did not get
  through (including a waiting event request) - a "нет связи" mark in the top
  bar (user's choice; click - check at once), the shown note stays; a probe
  request every 3 s (connection, not changes). The connection is back - a
  change check, a note that did not open loads.
- The watcher is a speedup, not the source of truth: versions still come from
  `stat`, a broken watcher - everything works by walking. A compile error is
  shown over the last successful version.
- **Warm-up** (`notes-core::warm`): all notes are built ahead of time into the
  disk cache (not kept in memory, figures are processed on opening) - first
  those hinted by the client (`POST /api/warm`: tabs, recent), then never
  built ones (small first), then the rest by the time of the last build, new
  books last; what is built (also with an error) is skipped; a user request
  goes out of turn. The mode is a device setting (all, only hinted, off).
  After a large pass Typst memory is freed; also, after a series of builds and
  5 s of idle memory is released after ordinary rebuilds too, so idle RSS
  returns to the level with a ready cache. The idle release fires once per
  idle period, only if there were new builds since the last release. A pass
  runs at startup, on a hint, on file changes and every 10 minutes (without a
  watcher - every minute).

### 9. The server online: storage, sync, sign-in

How to put it on a VPS - `docs/server.md`.

- **`notes-hub` - a server without Typst** (`crates/notes-hub`): sign-in and
  vault files with versions. It listens on HTTP on localhost; HTTPS is the
  reverse proxy's job (user's decision: the VPS already runs nginx), so the
  server trusts `X-Forwarded-Proto` for the `Secure` cookie flag. Each account
  has its own vaults: `<data>/hub/<login>/<vault>/` - `files/` (a plain vault
  folder, the truth), `manifest.json` (hash, size and change number `seq` of
  every file, tombstones of deleted ones), `history/<seq>/` (what a write
  replaced or removed: nothing is lost on the server).
- **Sign-in** (user's decision: login + password from the start;
  `notes_hub::auth`, the same module for the hub and for `notes serve
  --auth`): accounts are created on the machine (`notes users add <login>`,
  Argon2id hashes in `<data>/users.json`). `POST /api/login` -> a session: a
  cookie (`HttpOnly`, `SameSite=Strict`, `Secure` behind HTTPS) for the
  browser, the same token as `Authorization: Bearer` for a device core. A
  token is never passed in an address. A session lives 30 days without
  requests (`--session-days`), each request extends it; the server disk holds
  only hashes of tokens (`<data>/sessions.json`). A wrong password slows the
  next attempt for that login (1 s doubling to 60 s; many failures - for
  everyone); a state-changing request with a foreign `Origin` is refused.
  `POST /api/logout`, `GET /api/session`; changing a password or removing a
  user ends their sessions.
- **Who needs to sign in**: the browser and sync. The app window talks to its
  core over a Unix socket only the user can open - no sign-in (user's
  decision). `notes serve` on a localhost address is open as before; on
  another address it starts only with `--auth`. With sign-in on, the page,
  client files, themes and fonts are public (the sign-in screen needs them),
  everything else answers 401 and the client shows the sign-in screen.
- **Sync** (`notes_store::sync` - the engine, no network; `notes-hub` - the
  API and the client `HttpRemote`; `notes-device` - the device side): file
  versions, no CRDT. A device keeps a full copy of a vault and the state of
  the last sync (hash of every file). A round: scan the folder (only files
  whose size or time changed are hashed), ask the server for changes after
  the known `seq`, then per file - changed only there: download; only here:
  upload with "I replace version X" (the server refuses if it has another
  one); both sides differently: the device setting `device.sync_prefer`
  decides ("the computer writes first, the others receive": this device wins
  on a computer, the server wins on a phone). A local file that sync replaces
  or removes is moved to `<data>/sync/removed/` (kept 30 days), never just
  deleted.
- **The device side** (`crates/notes-device`; the core stays free of HTTP for
  WASM): `<data>/sync/` holds the account (server, login, session token), the
  state of every linked vault, the last result and a lock per vault. `notes
  sync login | link | now | status | unlink | logout`; `notes serve` (so the
  window's core too) runs a worker per linked vault: a round at start, 2 s
  after a local change (the file watcher), on a server change (long polling
  `changes?after=&wait=`), a retry with a growing pause when the server is
  unreachable. Pulled files are ordinary file changes: open notes refresh by
  themselves (§8). The client gets the state and the actions at
  `/api/device/sync...`.
- **A vault from the server is untrusted**: note HTML is sanitized, the client
  sets a strict CSP. See the section below.
- Reading the history, backups - later. Encryption - far away.

### 10. Fonts

The layout fonts (`fonts/README.md`) are embedded in the binary together with
the Typst fonts and shadow system ones: rendering does not depend on the
machine. The browser gets WOFF2 in parts with `unicode-range`
(`notes-core::webfonts`: subsetting - `fontcull`, WOFF2 - `ttf2woff2` with the
`glyf` transform for TrueType, for CFF - our own encoder): a Russian page needs
Latin and Cyrillic, ~35 KB per style instead of ~0.9 MB. The math font (CFF +
`MATH`) is not subset, only WOFF2 (1.3 -> 0.66 MB). The server compresses the
parts in the background at startup (ready ones in `<data>/cache/fonts`). The
browser needs all theme fonts, main and fallback (a glyph missing from the
main one looks as in the PDF): `baluk/css.typ` exports them together with the
colors (formula fonts as a separate list). The browser downloads a part of a
fallback font only if the page has a glyph of it missing from the main font.
The CFF text font (New Computer Modern) is first converted to TrueType
(`webfonts::cff_to_truetype`: quadratic outlines, no hints and no `MATH`, ~30
ms per style): Chrome rejects its CFF entirely (OTS: hints), and TrueType can
also be subset. The formula font is whole, but also with `unicode-range` by its
own glyphs.

### Typst packages

A note takes only packages from the whitelist with versions
(`notes-core::packages::ALLOWED`: CeTZ and its dependency) and those the user
allowed beyond it (`device.packages`, with a warning: it is foreign code).
Another package is a build error, it is not even downloaded
(`world::Loader`). The version of a page that read a package includes the list
of allowed ones (vault data `/_vault/packages/policy`): changing it rebuilds
notes with packages - with the library that is almost all of them.

### HTML security

- The page HTML is sanitized before serialization (the first tree pass in
  `passes::TREE`): only an allowlist of tags and attributes from the real
  output of the library, the passes and Typst (HTML, MathML, SVG) is allowed,
  the rest is removed.
- Always removed: the tags `script`, `iframe`, `object`, `embed`, `frame`,
  `base`, `form` and form elements. `meta` and `link` are kept only in
  `<head>` (Typst's service tags), outside `<head>` they are removed.
- For SVG, removed are `foreignObject`, `animate`/`set`/`animateTransform`
  with an `attributeName` pointing to `href`/`xlink:href` or `on*`, and `<use>`
  with an external `href` (only local `#...` are allowed).
- All `on*` attributes are removed.
- In `href`/`src`/`xlink:href`/`action`/`formaction` `javascript:` and
  `vbscript:` are forbidden. `data:` is allowed only as `image/*`.
- URLs are normalized before the check: case, spaces/control characters and
  HTML entities (`jav&#x61;script:` and `JaVa\nScRiPt:` are blocked).
- The `style` attribute and `<style>` are allowed only without `expression(`
  and dangerous `url(...)` (by the same URL rules).
- `data-*` and `aria-*` (including `data-k-*`) are kept.
- If something was removed, `notes check` warns with one line: how many tags,
  attributes and dangerous URLs were removed in this note. Note files do not
  change.
- The client (`notes serve`) is served with a CSP: `default-src 'self'`,
  `script-src 'self'` (without `unsafe-inline` and `unsafe-eval`),
  `object-src 'none'`, `base-uri 'none'`, `img-src 'self' data: blob:`,
  `style-src 'self' 'unsafe-inline'`, `font-src 'self'`, `connect-src 'self'`,
  `frame-ancestors 'self'`. In dev mode (Vite) the CSP is not applied, since
  the dev server serves the page.
