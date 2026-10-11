# Plan

Marks: `[x]` done, `[~]` in progress, `[ ]` not started. What is done - one
line (how it works - `docs/architecture.md`), open questions -
`docs/questions.md`.

## Priority

- [x] The server releases memory after builds (user's decision): a release
      after 5 s of idle, `comemo` frees the accelerators, the `notes`
      allocator is jemalloc. `notes serve` idle after warming `tests/vault` -
      361 MB (633 with glibc, ~1.17 GB before the release); numbers -
      `docs/research/E5.md`
- [x] Autostart of `notes serve`: `notes service install | remove | status` -
      a systemd user service (no root, at login, restart after a crash, log -
      journald); `tools/install.sh` restarts it
- [x] Rust per the `rust-best-practices` skill (user's decision): all code in
      `crates/` follows the skill and the project rules
      (`.claude/rules/crates.md`): panics - lints
      `clippy::unwrap_used`/`expect_used`, `expect` outside tests - under
      `#[expect]`, a Typst panic during a build - a build error of the note;
      `#[allow]` -> `#[expect]`
- [x] The whole project in English except the client interface (user's
      decision): code, comments, logs and messages, `docs/`, all README and
      the rules `.claude/rules/`, commit messages and PRs; the root README
      also has a Russian copy (`README.ru.md`). Models read the code and
      texts: English is cheaper in tokens, the skill does not quote Russian
      strings. There is no separate output mode for models: normal output is
      short (one problem - one line), structure - `--json`

## M0. The layout library in HTML

- [x] `baluk/`: HTML and PDF branches for every block, the public API in
      English only through `lib.typ` (a test snapshots the names)
- [x] Themes `classic` and `night`
- [x] The note language `lang:` and own words `words:`; dictionaries `ru` and
      `en` with the same keys (test)

## M1. Core and CLI

- [x] A compiler over the vault: virtual `/_baluk/`, embedded fonts, packages
      from the cache or network; note and book
- [x] A chain of HTML passes; figures - one SVG for all themes, shared glyphs,
      rounding ("Матан" 3.7 -> 1.7 MB, 0.2 MB over the network)
- [x] Memory and disk cache, warm-up, parallel builds, a watcher
- [x] Link index, backlinks, graph, full-text search
- [x] `notes` from any folder: `serve`, `new`, `list`, `tags`, `check`, `pdf`, `png`,
      `docs`, `info`, `vaults`; installation - `tools/install.sh`
- [x] The `/baluk-note` skill: everything needed is in the skill (workflow,
      reference, samples for every public name), only `notes` commands
- [x] Anchors with symbols: `C++`, `C#`, `a=b` stay in the `id`
- [x] Book properties: the root `main.typ` (`book.with` - shared by all
      chapters), a chapter's own - `chapter.with` (title, tags, label);
      chapter tags in `notes list`/`tags`, on the tag page and in the graph
      filter
- [x] `notes rename`: as in the app (title, file name, `#see` links), the
      skill uses it
- [x] `notes png`: the PDF pages as PNG (`typst-render` of the same version as
      Typst, `--pages`, `--dpi`) - for models that do not read PDF (user's
      decision); the skill checks the look with it
- [x] The browser also gets the theme fallback fonts (New Computer Modern,
      DejaVu Sans Mono): a glyph missing from the main font looks as in the
      PDF; parts only with the page's glyphs (NCM converted to TrueType)

## M2. The client `app/`

- [x] Tree, tabs, history, the Ctrl+K palette and Ctrl+O switching, search
      (also in a book), tags, link previews, contents, "Ссылаются сюда",
      hotkeys, reading mode, settings by the core schema, a narrow screen
- [x] Updates on server events; `refresh.mode`
- [x] Book chapters ahead of time, with a check of the book version
- [x] Several vaults (a choice screen), deleting notes to the trash, titles
      instead of file names
- [x] Ctrl+F in a note and a book: where to search - the chapter, the note or
      book, the whole vault (Tab); a second Ctrl+F - the browser search
- [x] A vault switcher at the bottom of the sidebar (as in Obsidian); rename
      and delete the open vault (to the system trash, with confirmation)
- [x] Vault settings `<vault>/.baluk/settings.json` over the shared ones;
      a change from the interface - for the vault, theme and font size - for
      all ("only for this vault" - optional); the settings window shows where
      a value comes from
- [x] Server connection: a "нет связи" mark in the top bar (the note stays,
      it comes back by itself), no change polling
- [x] Formulas in a title - as source text ("Ряд sum 1/n^2") in the tree,
      tabs and graph; "Ссылаются сюда" - the section heading as in the
      contents
- [x] What the book shares (title, root tags) at the top of the contents in
      any chapter
- [x] Delete a folder from the tree (right click, to the trash with
      everything in it); the theme from the first frame (no white flash);
      switching notes without an empty frame; 3D rotation - the near side
      follows the pointer
- [x] Empty folders in the tree; rename a note, book or folder (right click):
      the title, the file name from it, `#see` links to it - with a plan in
      the dialog (which notes get fixed)
- [x] 3D on a phone: a vertical swipe scrolls the page, one started sideways
      rotates
- [x] The top bar and tabs are one pinned panel (shared background, no gap)
- [x] A note preview on hover and in the tree (the card is to the right of
      the row); the tree menu (right click) names at the top what it is for
- [x] A folder page (`/f/<path>`, user's decision): subfolders and notes, a
      link to the folder graph (`/graph?folder=`, notes with subfolders); the
      path in the top bar - links to folders
- [x] `__NOTES_API__` is only the address (`base`): who may use the server -
      the session cookie (M4)

## M3. Graph and interactivity

- [x] The entry to the vault graph: an icon at the bottom of the sidebar, next
      to "Главная"; the icon is an own "star" of 4 vertices (user's choice)
- [x] Dragging a graph node: a pointer outside the graph does not select page
      text (the graph gesture cancels the default mouse action)
- [x] The `/graph` page and the live graph; a graph in a note `#vault-graph`
- [x] `interactive-plot`, `interactive-surface`, `frames`
- [x] The core layout is Barnes-Hut, 5-10 times faster (`docs/research/E6.md`;
      the look is approved by the user); Ctrl+click on a node and a link - a
      background tab
- [x] A book on the graph: the "книги главами" checkbox on `/graph` - the book
      root and chapters as a cluster around it; a link into a section goes to
      its chapter. The look (user's choice): a chapter is a hollow circle
      smaller than any note, the link is solid and pale; the checkbox is in
      the filter row; not yet on the home page and in `#vault-graph`
- [x] Graph forces are sliders (user's decision): folders, repulsion, center,
      links (the core layout, `Forces` in the filter), neighbors while
      dragging (client physics); a slide-out panel at the side of the graph
      (user's choice; hidden by default, Esc hides it), settings `graph.*`
      for all vaults

## The core on the device, the server is storage

User's decision (architecture §1, §9). The priority is **desktop** (M5): notes
are written on a computer (Claude Code), other devices almost always read. The
desktop works without a server too, sync joins with M4. Research E4 (WASM) -
before M7 starts.

## Device settings - before M5-M6

- [x] The group "Это устройство" (`device.*`): warm-up (the whole vault / only
      the open one / none; a phone - none), simultaneous builds (2), Typst
      memory (10 builds), pages in memory (64 MB), disk cache (512 MB), the
      lifetime of a foreign cache (14 days); a gentle warm-up
- [ ] Phone defaults (memory 32 MB, disk 256 MB, Typst memory 3) are guessed:
      tune them by measurement in M6

## M4. Storage server and sign-in

- [x] Sign-in instead of the shared token (architecture §9): login +
      password (user's decision), accounts - `notes users`; sessions 30 days
      without requests (`--session-days`), token hashes on disk; a pause after
      a wrong password; the sign-in screen on 401 and "Выйти" in the vault
      menu; `notes serve --auth` (required on a non-localhost address), the
      window needs no sign-in
- [x] **A vault from the server is untrusted**: HTML is sanitized by an
      allowlist (allowed tags and attributes), `on*`, dangerous URLs and SVG
      `foreignObject`/`use` pointing outside are removed. The client is served
      with a CSP: `script-src 'self'`, `object-src 'none'`, `base-uri 'none'`,
      `img-src 'self' data: blob:`, `style-src 'self' 'unsafe-inline'`,
      `font-src 'self'`, `connect-src 'self'`, `frame-ancestors 'self'`. The
      Vite dev server is not involved. `notes check` warns with one line if
      something was removed; notes have no JS of their own
- [x] Typst packages - a whitelist with versions (`notes-core::packages`):
      another package is a build error; beyond the list - the device setting
      `device.packages` with a warning (user's decision). Review and extend
      the list regularly: the beauty of notes matters more; the same rules
      apply to sync
- [x] A server without Typst, `notes-hub` (architecture §9): accounts,
      sessions, vault files with versions and history, change long polling;
      the light crate `notes-store` (data directory rules, vault names,
      accounts, sessions, the sync engine)
- [ ] The rest of the core split: storage, paths, settings and the watcher
      move to the light crate when light commands move into `notes`
- [x] Sync: a copy on the device, file versions, changes on events; who wins
      a conflict - `device.sync_prefer` (the computer writes first, the others
      receive - user's decision); `notes sync`, a background worker in the
      core, the API for the client
- [x] Sync in the app settings (the section "Синхронизация"): the server
      address and sign-in, which vaults are synced (a vault only on the
      server - download), the state and the conflicts of the last round
- [x] A guard against mass deletion in sync: a round that deletes a large
      part of the vault waits for `notes sync confirm` or the buttons in the
      settings; `restore` brings the files back (architecture §9)
- [ ] An edit lock (one writer at a time) - groundwork only: a write names
      the version it replaces
- [x] HTTPS - the reverse proxy (user's decision: nginx is already on the
      VPS); how to deploy - `docs/server.md`
- [x] One-command update of the hub on the VPS: `tools/deploy-hub.sh` (the VPS
      builds, a root helper installs; `docs/server.md`, "Updating")
- [x] The hub on the VPS (`docs/server.md`): the API on a subdomain, the
      main name shows the front page `landing/` (user's decision; a first
      version, to polish)

## M5. Desktop - Tauri with the core inside

- [x] Research E7 (`docs/research/E7.md`): WebKitGTK renders notes like
      Chromium (small differences - below), the interface through the
      `notes://` scheme and the same `Router` works fully; SSE through the
      scheme is impossible
- [x] Parts (user's decision, architecture §1): the thin `notes` calls
      `notes-typst` (all current commands) from its folder and passes the
      version; light commands move into `notes` with the core split (M4)
- [x] Events by long polling instead of SSE (one way for the browser and the
      window); `notes serve --socket` - a Unix socket without a token, the
      service listens on it too
- [x] `notes-app`: a Tauri 2 window without Typst, its own address scheme ->
      the core socket; the core is already running (service) - connect, if
      not - start its own (closing the window and SIGTERM stop it); external
      links and PDF go to system programs
- [x] Desktop instead of the browser (user's decision): the app is the
      window - the menu entry or `notes app [--vault <name> [<note>]]`; README,
      `docs/writing.md`, `notes new` and the skill point to it; `notes serve`
      and `notes service` stay for the browser version and self-hosting
      (`tools/install.sh` does not install the service)
- [x] The skill without the service (user's decision): it tells the user the
      `notes app` command, looks at the PNG pages and, with a browser tool,
      at the page of its own temporary `notes serve`
- [x] Windows (user's decision): one app, a second launch goes to it; the
      setting "another vault - in this window or a new one" (this one by
      default); right click on a vault - both choices; Ctrl+click - a new
      window; a vault already open in another window - switch to it
- [x] A `WebKitWebProcess` crash when closing the window (user's task): the
      window kills its WebKit process before exit (tech debt "WebKitGTK in
      the window")
- [x] Page jitter in the window (touchpad, screen scale 1.25): the document
      does not scroll, the page column does (`lib/scroll.ts`) - panels and
      the contents stay in place
- [x] The window opens maximized or fullscreen if it was so when closed
      (user's task; `tauri-plugin-window-state`, size and position are not
      remembered)
- [x] WebKitGTK differences (E7): the gap after an operator with limits or
      scripts (∑, ∫, lim) - the pass `passes/operators.rs` puts it after the
      whole construct for every engine; interface `<select>` and the graph
      checkbox have their own look, the same in both engines; `baluk.css`
      goes through lightningcss with prefixes; no elastic page bounce
- [x] Installation - `tools/install.sh` (parts, a menu entry and icon), later
      a PKGBUILD (user's decision)
- [x] The app icon (user's choice of 7 variants): an italic "b" and lines of
      text; it is also the site favicon
- [x] A vault copy + sync, the server address and sign-in in settings (M4)

## M6. Android - Tauri with the core inside

- [ ] The same app on Android; tabs and the graph on a phone (tapping a node -
      the user decides)
- [ ] Speed on a phone: first the theme being read, the second after showing
      (E1); the warm-up does not disturb reading

## M7. Browser - the core in WASM

- [ ] Research E4: `notes-core` for `wasm32` (no threads, `notify`, `std::fs` -
      `Storage` in OPFS/IndexedDB), build time and memory of the demo and a
      large book in Chromium and on a phone, the weight of fonts and packages;
      report - `docs/research/E4.md`
- [ ] The core in a Web Worker, the API transport - messages to the worker

## Later

- Public notes (to think through): a note opens by a link for everyone, in the
  full client (not a static site); it is visible which ones are public. Who
  builds HTML for a reader without the app - to decide (a server without
  Typst; the owner's core or WASM at the reader). Download - PDF
- Many users: accounts, a vault per user, a database behind `Storage`
- Version history, backups, algorithm animation, running code, encryption,
  iOS
- An alphabet of file names (user's decision; the mechanism is renaming from
  the interface, `notes-core::rename`): allowed characters - Latin, Cyrillic,
  some special characters; some special characters in a file name are
  replaced by words, but stay as they are in the title. Which characters and
  which words - decide with the user; length up to 80 letters and "Name 2"
  for a taken one - as now. The alphabet is a rule for all paths (user's
  decision): together with it - a one-time migration, existing files and
  folders of vaults with other characters are renamed by the same mechanism
  as renaming (`#see` links are rewritten, with a list and confirmation);
  after it `NoteId` accepts no other names. The fixtures `tests/vault/Имена/`
  (`C++ и C#`, `50% готово`) - redo under the rule
- Real formulas in titles (as in the contents) in the tree, tabs, graph - if
  needed: requires built titles of all notes
- Build limits (time and memory limits: a hanging note is an error, not a
  hanging device) - postponed (user's decision: no option was liked). Typst
  cannot interrupt a build; considered: checks inside Typst (copies of two of
  its crates), building in a separate process (the Typst cache is lost, not
  possible on Android and in WASM), a watchdog without Typst changes (the
  build keeps burning CPU, overspent memory - a server restart). For now
  Typst's built-in limits cover it: `while` - 10 000 steps, call depth - 80
- A built-in editor - very low priority
